// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt::{Debug, Formatter};
use std::path::{Component, Path, PathBuf};
use std::process::Child;

use bstr::ByteSlice;
use indexmap::IndexMap;
use logging_timer::time;

use crate::atomic::{Target, atomic_path};
use crate::cmd_env::{ParsedEnv, parse_scie_env_placeholder, prepare_env};
use crate::config::{Cmd, Fmt};
use crate::installer::Installer;
use crate::lift::{File, Lift};
use crate::placeholders::{self, Item, Placeholder, ScieBindingEnv};
use crate::process::{EnvVar, Process};
use crate::{CurrentExe, EnvVars, Jump, Source, config};

#[cfg(all(
    target_os = "linux",
    target_arch = "arm",
    target_pointer_width = "32",
    target_endian = "little"
))]
pub const ARCH: &str = "armv7l";

#[cfg(not(all(
    target_os = "linux",
    target_arch = "arm",
    target_pointer_width = "32",
    target_endian = "little"
)))]
pub const ARCH: &str = env::consts::ARCH;

fn expanduser(path: &Path) -> Result<PathBuf, String> {
    if !<[u8]>::from_path(path)
        .ok_or_else(|| {
            format!(
                "Failed to decode the path {} as utf-8 bytes",
                path.display()
            )
        })?
        .contains(&b'~')
    {
        return Ok(path.to_path_buf());
    }

    let home_dir = dirs::home_dir()
        .ok_or_else(|| format!("Failed to expand home dir in path {}", path.display()))?;
    let mut components = Vec::new();
    for path_component in path.components() {
        match path_component {
            Component::Normal(component) if OsStr::new("~") == component => {
                for home_dir_component in home_dir.components() {
                    components.push(home_dir_component)
                }
            }
            component => components.push(component),
        }
    }
    Ok(components.into_iter().collect())
}

fn path_to_str(path: &Path) -> Result<&str, String> {
    <[u8]>::from_path(path)
        .ok_or_else(|| format!("Failed to decode {} as a utf-8 path name", path.display()))?
        .to_str()
        .map_err(|e| format!("{e}"))
}

#[derive(Clone, Debug)]
struct LiftManifest {
    path: PathBuf,
    jump: Jump,
    lift: Lift,
}

impl LiftManifest {
    fn install(&self) -> Result<(), String> {
        atomic_path(&self.path, Target::File, |path| {
            config(self.jump.clone(), self.lift.clone()).serialize(
                &mut std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map_err(|e| {
                        format!(
                            "Failed top open lift manifest at {path} for writing: {e}",
                            path = self.path.display()
                        )
                    })?,
                Fmt::new().trailing_newline(true).pretty(true),
            )
        })?;
        Ok(())
    }
}

pub(crate) struct LoadProcess {
    lift_manifest: Option<LiftManifest>,
    process: Process,
}

impl LoadProcess {
    pub(crate) fn spawn_stdout(&self, args: &[&str]) -> Result<Child, String> {
        if let Some(ref lift_manifest) = self.lift_manifest {
            lift_manifest.install()?;
        }
        self.process.spawn_stdout(args)
    }

    pub(crate) fn exe(&self) -> &OsStr {
        self.process.exe.as_os_str()
    }
}

impl Debug for LoadProcess {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadProcess")
            .field("process", &self.process)
            .finish()
    }
}

// The two largest variants are Install at ~136 bytes and LoadAndInstall at ~616 bytes.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub(crate) enum FileEntry {
    Skip(usize),
    Install((File, PathBuf)),
    LoadAndInstall((LoadProcess, File, PathBuf)),
    ScieTote((File, Vec<(File, PathBuf)>)),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Binding {
    target: PathBuf,
    process: Process,
}

impl Binding {
    fn execute<F>(&self, install_required_files: F) -> Result<HashMap<String, String>, String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        if let Some(env) = atomic_path(self.target.as_path(), Target::File, |lock| {
            trace!("Installing boot binding {binding:#?}", binding = &self);
            install_required_files()?;

            let result = self
                .process
                .execute(None, vec![("SCIE_BINDING_ENV".into(), Some(lock.into()))]);

            match result {
                Err(err) => Err(format!("Failed to launch boot binding: {err}")),
                Ok(exit_status) if !exit_status.success() => {
                    Err(format!("Boot binding command failed: {exit_status}"))
                }
                _ => std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(lock)
                    .map_err(|e| {
                        format!(
                            "Failed to touch lock file {path}: {e}",
                            path = lock.display()
                        )
                    }),
            }?;
            // We eagerly load the env file before we exit the lock such that malformed env files
            // are detected and the lock is not poisoned.
            Self::load_env_file(lock)
        })? {
            Ok(env)
        } else {
            self.load_env()
        }
    }

    fn load_env(&self) -> Result<HashMap<String, String>, String> {
        Self::load_env_file(self.target.as_path())
    }

    fn load_env_file(env_file: &Path) -> Result<HashMap<String, String>, String> {
        let contents = std::fs::read_to_string(env_file).map_err(|e| {
            format!(
                "Failed to read binding env from {env_file}: {e}",
                env_file = env_file.display()
            )
        })?;
        let mut env = HashMap::new();
        for line in contents.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let mut components = trimmed.splitn(2, '=');
                let key = components.next().ok_or_else(|| {
                    format!("The non-empty line {line} must contain at least an env var name.")
                })?;
                let value = components.next().unwrap_or("");
                env.insert(key.to_string(), value.to_string());
            }
        }
        Ok(env)
    }
}

pub(crate) struct SelectedCmd {
    pub(crate) process: Process,
    pub(crate) files: Vec<FileEntry>,
    pub(crate) argv1_consumed: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Context<'a> {
    scie: &'a CurrentExe,
    lift: &'a Lift,
    base: PathBuf,
    installer: &'a Installer<'a>,
    files_by_name: HashMap<&'a str, &'a File>,
    replacements: HashSet<&'a File>,
    lift_manifest: LiftManifest,
    lift_manifest_dependants: HashSet<Process>,
    lift_manifest_installed: bool,
    bound: HashMap<String, Binding>,
    installed: HashSet<File>,
    ambient_env: IndexMap<OsString, OsString>,
}

impl<'a> Context<'a> {
    #[time("debug", "Context::{}")]
    fn new(
        scie: &'a CurrentExe,
        jump: &'a Jump,
        lift: &'a Lift,
        installer: &'a Installer,
        custom_env: Option<HashMap<String, String>>,
    ) -> Result<Self, String> {
        let mut files_by_name = HashMap::new();
        for file in &lift.files {
            files_by_name.insert(file.name.as_str(), file);
            if let Some(key) = file.key.as_ref() {
                files_by_name.insert(key.as_str(), file);
            }
        }
        let base = if let Ok(base) = env::var("SCIE_BASE") {
            PathBuf::from(base)
        } else if let Some(base) = &lift.base {
            PathBuf::from(base)
        } else if let Some(dir) = dirs::cache_dir() {
            dir.join("nce")
        } else {
            PathBuf::from("~/.nce")
        };
        let base = expanduser(base.as_path())?;
        let mut ambient_env = custom_env
            .map(|c| {
                c.into_iter()
                    .map(|(n, v)| (n.into(), v.into()))
                    .collect::<IndexMap<_, _>>()
            })
            .unwrap_or_else(|| env::vars_os().collect::<IndexMap<_, _>>());
        ambient_env.insert("SCIE".into(), scie.exe.as_os_str().into());
        ambient_env.insert("SCIE_ARGV0".into(), scie.invoked_as.as_os_str().into());
        let mut context = Context {
            scie,
            lift,
            base,
            installer,
            files_by_name,
            replacements: HashSet::new(),
            lift_manifest: LiftManifest {
                path: PathBuf::new(), // N.B.: We replace this empty value below.
                jump: jump.clone(),
                lift: lift.clone(),
            },
            lift_manifest_dependants: HashSet::new(),
            lift_manifest_installed: false,
            bound: HashMap::new(),
            installed: HashSet::new(),
            ambient_env,
        };

        // Now patch up the base and the lift path (which is derived from it) with any placeholder
        // resolving that may be required.
        let (parsed_base, needs_lift_manifest) = context.reify_string(
            None,
            &context
                .base
                .clone()
                .into_os_string()
                .into_string()
                .map_err(|e| {
                    format!("Failed to interpret the scie.lift.base as a utf-8 string: {e:?}")
                })?,
        )?;
        if needs_lift_manifest {
            return Err(format!(
                "The scie.lift.base cannot use the placeholder {{scie.lift}} since that \
                placeholder is calculated from the resolved location of the scie.lift.base, \
                given: {base}",
                base = context.base.display()
            ));
        }
        context.base = PathBuf::from(parsed_base);
        context.lift_manifest.path = context.base.join(&lift.hash).join("lift.json");
        Ok(context)
    }

    fn prepare_process(&mut self, cmd: &'a Cmd) -> Result<Process, String> {
        let mut env = prepare_env(cmd, &self.ambient_env)?;
        let mut needs_lift_manifest = false;
        let (exe, needs_manifest) = self.reify_string(Some(&env), &cmd.exe)?;
        needs_lift_manifest |= needs_manifest;

        let mut args = vec![];
        for arg in &cmd.args {
            let (reified_arg, needs_manifest) = self.reify_string(Some(&env), arg)?;
            needs_lift_manifest |= needs_manifest;
            args.push(reified_arg.into());
        }
        let mut vars = vec![];
        for (key, value) in cmd.env.iter() {
            let final_value = match value {
                Some(val) => {
                    let (reified_value, needs_manifest) = self.reify_string(Some(&env), val)?;
                    needs_lift_manifest |= needs_manifest;
                    match key {
                        config::EnvVar::Default(name) => {
                            if !env.contains_key(name) {
                                env.insert(name.to_owned(), reified_value.clone());
                            }
                        }
                        config::EnvVar::Replace(name) => {
                            env.insert(name.to_owned(), reified_value.clone());
                        }
                    }
                    Some(reified_value)
                }
                None => None,
            };
            vars.push(EnvVar::try_from((key, final_value))?);
        }

        let process = Process {
            env: EnvVars { vars },
            exe: exe.into(),
            args,
        };
        if needs_lift_manifest {
            self.lift_manifest_dependants.insert(process.clone());
        }
        Ok(process)
    }

    fn prepare(&mut self, cmd: &'a Cmd) -> Result<(Process, Vec<FileEntry>), String> {
        let process = self.prepare_process(cmd)?;

        let mut load_entries = vec![];
        for file in &self.lift.files {
            if self.replacements.contains(&file)
                && !self.installed.contains(file)
                && let Source::LoadBinding(binding_name) = &file.source
            {
                let path = self.get_path(file);
                let file_source_process = self.prepare_process(
                    self.lift
                        .boot
                        .bindings
                        .get(binding_name)
                        .ok_or_else(|| format!("No boot binding named {binding_name}."))?,
                )?;
                let lift_manifest = if !self.lift_manifest_installed
                    && self.lift_manifest_dependants.contains(&file_source_process)
                {
                    Some(self.lift_manifest.clone())
                } else {
                    None
                };
                load_entries.push(FileEntry::LoadAndInstall((
                    LoadProcess {
                        lift_manifest,
                        process: file_source_process,
                    },
                    file.clone(),
                    path,
                )))
            }
        }

        let mut scie_tote = vec![];
        let mut file_entries = vec![];
        for (index, file) in self.lift.files.iter().enumerate() {
            if self.replacements.contains(&file) && !self.installed.contains(file) {
                let path = self.get_path(file);
                if file.size == 0 {
                    scie_tote.push((file.clone(), path));
                } else if Source::Scie == file.source {
                    file_entries.push(FileEntry::Install((file.clone(), path)));
                }
            } else if index < self.lift.files.len() - 1 || scie_tote.is_empty() {
                file_entries.push(FileEntry::Skip(if file.source == Source::Scie {
                    file.size
                } else {
                    0
                }))
            }
        }
        if !scie_tote.is_empty() {
            let tote_file = self
                .lift
                .files
                .last()
                .ok_or_else(|| {
                    format!(
                        "The lift manifest contains scie-tote entries (0 size) but no scie-tote \
                        holder:\n{files:#?}",
                        files = self.lift.files
                    )
                })?
                .clone();
            file_entries.push(FileEntry::ScieTote((tote_file, scie_tote)));
        }

        // Load external files last since these may need files in the scie itself to 1st be
        // extracted for use in the load process.
        file_entries.append(&mut load_entries);

        Ok((process, file_entries))
    }

    fn select_cmd(
        &mut self,
        name: &str,
        argv1_consumed: bool,
    ) -> Result<Option<SelectedCmd>, String> {
        if let Some(cmd) = self.lift.boot.commands.get(name) {
            let (process, files) = self.prepare(cmd)?;
            self.maybe_install_lift_manifest(&process)?;
            return Ok(Some(SelectedCmd {
                process,
                files,
                argv1_consumed,
            }));
        }
        Ok(None)
    }

    fn select_command(&mut self, scie_name: &str) -> Result<SelectedCmd, String> {
        // Forced command.
        if let Some(cmd) = env::var_os("SCIE_BOOT") {
            let name = cmd.into_string().map_err(|value| {
                format!("Failed to decode environment variable SCIE_BOOT: {value:?}")
            })?;
            if let Some(selected_cmd) = self.select_cmd(&name, false)? {
                return Ok(selected_cmd);
            } else {
                return Err(format!(
                    "`SCIE_BOOT={name}` was found in the environment but \"{name}\" does \
                        not correspond to any {scie_name} commands."
                ));
            }
        }

        // Default command.
        if let Some(selected_cmd) = self.select_cmd("", false)? {
            return Ok(selected_cmd);
        }

        // BusyBox style where basename indicates command name.
        if let Some(name) = self.scie.name()
            && let Some(selected_command) = self.select_cmd(name, false)?
        {
            return Ok(selected_command);
        }

        // BusyBox style where 1st arg indicates command name.
        if let Some(argv1) = env::args().nth(1)
            && let Some(selected_cmd) = self.select_cmd(&argv1, true)?
        {
            return Ok(selected_cmd);
        }

        Err("Could not determine which command to run.".to_string())
    }

    fn get_path(&self, file: &File) -> PathBuf {
        self.base.join(&file.hash).join(&file.name)
    }

    fn get_bindings_dir(&self) -> PathBuf {
        self.base.join(&self.lift.hash).join("bindings")
    }

    fn maybe_install_lift_manifest(&mut self, process: &Process) -> Result<(), String> {
        if !self.lift_manifest_installed && self.lift_manifest_dependants.contains(process) {
            self.lift_manifest.install()?;
            self.lift_manifest_installed = true;
        }
        Ok(())
    }

    fn parse_env(
        &mut self,
        cmd_env: Option<&IndexMap<String, String>>,
        env_var: &str,
    ) -> Result<(ParsedEnv, bool), String> {
        let (parsed_env, needs_lift_manifest) = self.reify_string(cmd_env, env_var)?;
        Ok((
            parse_scie_env_placeholder(&parsed_env)?,
            needs_lift_manifest,
        ))
    }

    fn bind(&mut self, name: &str) -> Result<HashMap<String, String>, String> {
        if let Some(binding) = self.bound.get(name) {
            binding.load_env()
        } else {
            let (process, files) = self.prepare(
                self.lift
                    .boot
                    .bindings
                    .get(name)
                    .ok_or_else(|| format!("No boot binding named {name}."))?,
            )?;
            let process_hash = process.fingerprint()?;
            let boot_binding = Binding {
                target: self
                    .base
                    .join(&self.lift.hash)
                    .join("locks")
                    .join(format!("{name}-{process_hash}")),
                process,
            };
            let binding_env = boot_binding.execute(|| {
                self.maybe_install_lift_manifest(&boot_binding.process)?;
                self.installer.install(files.as_slice())
            })?;
            self.bound.insert(name.to_string(), boot_binding);
            for file_entry in files {
                match file_entry {
                    FileEntry::Skip(_) => {}
                    FileEntry::Install((file, _)) => {
                        self.installed.insert(file);
                    }
                    FileEntry::LoadAndInstall((_, file, _)) => {
                        self.installed.insert(file);
                    }
                    FileEntry::ScieTote((_, tote_entries)) => {
                        for (file, _) in tote_entries {
                            self.installed.insert(file);
                        }
                    }
                }
            }
            Ok(binding_env)
        }
    }

    fn reify_string(
        &mut self,
        cmd_env: Option<&IndexMap<String, String>>,
        value: &str,
    ) -> Result<(String, bool), String> {
        let mut reified = String::with_capacity(value.len());
        let mut lift_manifest_required = false;

        let parsed = placeholders::parse(value)?;
        for item in &parsed.items {
            match item {
                Item::LeftBrace => reified.push('{'),
                Item::Text(text) => reified.push_str(text),
                Item::Placeholder(Placeholder::FileHash(name)) => {
                    let (parsed_name, needs_manifest) = self.reify_string(cmd_env, name)?;
                    lift_manifest_required |= needs_manifest;
                    let file = self
                        .files_by_name
                        .get(parsed_name.as_str())
                        .ok_or_else(|| {
                            format!("No file named {parsed_name} is stored in this scie.")
                        })?;
                    reified.push_str(&file.hash);
                }
                Item::Placeholder(Placeholder::FileName(name)) => {
                    let (parsed_name, needs_manifest) = self.reify_string(cmd_env, name)?;
                    lift_manifest_required |= needs_manifest;
                    let file = self
                        .files_by_name
                        .get(parsed_name.as_str())
                        .ok_or_else(|| {
                            format!("No file named {parsed_name} is stored in this scie.")
                        })?;
                    let path = self.get_path(file);
                    reified.push_str(path_to_str(&path)?);
                    self.replacements.insert(file);
                }
                Item::Placeholder(Placeholder::Env(env_var)) => {
                    let (parsed_env, needs_manifest) = self.parse_env(cmd_env, env_var)?;
                    lift_manifest_required |= needs_manifest;
                    let value = if let Some(val) = cmd_env
                        .and_then(|e| e.get(&parsed_env.name))
                        .map(String::to_owned)
                        .or_else(|| {
                            self.ambient_env
                                .get(&OsString::from(&parsed_env.name))
                                .and_then(|val| val.clone().into_string().ok())
                        }) {
                        let (parsed_value, needs_manifest) = self.reify_string(cmd_env, &val)?;
                        lift_manifest_required |= needs_manifest;
                        parsed_value
                    } else {
                        parsed_env.default.unwrap_or_default()
                    };
                    reified.push_str(&value)
                }
                Item::Placeholder(Placeholder::UserCacheDir(fallback)) => {
                    let (parsed_fallback, needs_manifest) = self.reify_string(cmd_env, fallback)?;
                    lift_manifest_required |= needs_manifest;
                    reified.push_str(if let Some(user_cache_dir) = dirs::cache_dir() {
                        user_cache_dir.into_os_string().into_string().map_err(|e| {
                            format!("Could not interpret the user cache directory as a utf-8 string: {e:?}")
                        })?
                    } else {
                        parsed_fallback
                    }.as_str())
                }
                Item::Placeholder(Placeholder::Scie) => {
                    reified.push_str(path_to_str(self.scie.exe.as_path())?)
                }
                Item::Placeholder(Placeholder::ScieArgv0) => {
                    reified.push_str(path_to_str(self.scie.invoked_as.as_path())?)
                }
                Item::Placeholder(Placeholder::ScieBase) => {
                    reified.push_str(path_to_str(&self.base)?)
                }
                Item::Placeholder(Placeholder::ScieBindings) => {
                    reified.push_str(path_to_str(self.get_bindings_dir().as_path())?);
                }
                Item::Placeholder(Placeholder::ScieBindingCmd(name)) => {
                    self.bind(name)?;
                    reified.push_str(path_to_str(self.get_bindings_dir().as_path())?);
                }
                Item::Placeholder(Placeholder::ScieBindingEnv(ScieBindingEnv {
                    binding,
                    env: env_var,
                })) => {
                    let binding_env = self.bind(binding)?;
                    let (parsed_env, needs_manifest) = self.parse_env(cmd_env, env_var)?;
                    lift_manifest_required |= needs_manifest;
                    let value = binding_env
                        .get(&parsed_env.name)
                        .map(String::to_owned)
                        .or(parsed_env.default)
                        .unwrap_or_default();
                    reified.push_str(&value)
                }
                Item::Placeholder(Placeholder::ScieLift) => {
                    lift_manifest_required = true;
                    reified.push_str(path_to_str(&self.lift_manifest.path)?);
                }
                Item::Placeholder(Placeholder::SciePlatform) => reified
                    .push_str(format!("{os}-{arch}", os = env::consts::OS, arch = ARCH).as_str()),
                Item::Placeholder(Placeholder::SciePlatformArch) => reified.push_str(ARCH),
                Item::Placeholder(Placeholder::SciePlatformOs) => reified.push_str(env::consts::OS),
            }
        }
        Ok((reified, lift_manifest_required))
    }
}

pub(crate) fn select_command(
    current_exe: &CurrentExe,
    jump: &Jump,
    lift: &Lift,
    installer: &Installer,
    custom_env: Option<HashMap<String, String>>,
) -> Result<SelectedCmd, String> {
    let mut context = Context::new(current_exe, jump, lift, installer, custom_env)?;
    context.select_command(lift.name.as_str())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use indexmap::IndexMap;

    use super::Context;
    use crate::config::{ArchiveType, Boot, Cmd, Compression, FileType};
    use crate::installer::Installer;
    use crate::{CurrentExe, File, Jump, Lift, Process, Source, config, process};

    #[test]
    fn env() {
        let jump = Jump {
            size: 42,
            version: "0.1.0".to_string(),
        };
        let lift = Lift {
            name: "test".to_string(),
            description: None,
            base: Some(
                PathBuf::from("{scie.user.cache_dir={scie.env.USER_CACHE_DIR=/tmp/nce}}")
                    .join("example")
                    .into_os_string()
                    .into_string()
                    .unwrap(),
            ),
            load_dotenv: true,
            size: 137,
            hash: "abc".to_string(),
            boot: Boot {
                commands: Default::default(),
                bindings: Default::default(),
            },
            files: vec![File {
                name: "file".to_string(),
                key: None,
                size: 37,
                hash: "def".to_string(),
                file_type: FileType::Blob,
                executable: None,
                eager_extract: false,
                source: Source::Scie,
            }],
            other: None,
        };
        let installer = Installer::new(&[]);
        let current_exe = CurrentExe::from_path(Path::new("scie_path"));
        let mut context =
            Context::new(&current_exe, &jump, &lift, &installer, Some(HashMap::new())).unwrap();

        let mut env = IndexMap::new();
        assert_eq!(
            ("".to_string(), false),
            context
                .reify_string(Some(&env), "{scie.env.__DNE__}")
                .unwrap()
        );

        env.clear();
        assert_eq!(
            ("default".to_string(), false),
            context
                .reify_string(Some(&env), "{scie.env.__DNE__=default}")
                .unwrap()
        );

        env.clear();
        env.insert("__DNE__".to_owned(), "foo".to_owned());
        assert_eq!(
            ("foo".to_string(), false),
            context
                .reify_string(Some(&env), "{scie.env.__DNE__=default}")
                .unwrap()
        );

        env.clear();
        assert_eq!(
            ("scie_path".to_string(), false),
            context
                .reify_string(Some(&env), "{scie.env.__DNE__={scie}}")
                .unwrap()
        );

        env.clear();
        let expected_scie_base = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp/nce"))
            .join("example");
        assert_eq!(
            (
                expected_scie_base
                    .clone()
                    .into_os_string()
                    .into_string()
                    .unwrap(),
                false
            ),
            context.reify_string(Some(&env), "{scie.base}").unwrap()
        );
        assert_eq!(
            (
                expected_scie_base
                    .join("abc")
                    .join("lift.json")
                    .to_str()
                    .unwrap()
                    .to_string(),
                true
            ),
            context
                .reify_string(Some(&env), "{scie.env.__DNE__={scie.lift}}")
                .unwrap()
        );

        let mut invalid_lift = lift.clone();
        invalid_lift.base = Some("{scie.lift}/circular".to_string());
        assert_eq!(
            "The scie.lift.base cannot use the placeholder {scie.lift} since that placeholder is \
            calculated from the resolved location of the scie.lift.base, given: \
            {scie.lift}/circular"
                .to_string(),
            Context::new(
                &CurrentExe::from_path(Path::new("scie_path")),
                &jump,
                &invalid_lift,
                &installer,
                None
            )
            .unwrap_err()
        );

        env.clear();
        assert_eq!(
            (
                expected_scie_base
                    .join("def")
                    .join("file")
                    .to_str()
                    .unwrap()
                    .to_string(),
                false
            ),
            context
                .reify_string(Some(&env), "{scie.env.__DNE__={file}}")
                .unwrap()
        );

        env.clear();
        assert_eq!(
            ("42".to_string(), false),
            context
                .reify_string(Some(&env), "{scie.env.__DNE__={scie.env.__DNE2__=42}}")
                .unwrap()
        );

        env.clear();
        env.insert("__DNE2__".to_owned(), "bar".to_owned());
        assert_eq!(
            ("bar".to_string(), false),
            context
                .reify_string(Some(&env), "{scie.env.__DNE__={scie.env.__DNE2__=42}}")
                .unwrap()
        );
    }

    #[test]
    fn prepare_process_use_cmd_env() {
        let jump = Jump {
            size: 42,
            version: "0.1.0".to_string(),
        };
        let lift = Lift {
            name: "test".to_string(),
            description: None,
            base: Some("/tmp/nce".to_string()),
            load_dotenv: true,
            size: 137,
            hash: "abc".to_string(),
            boot: Boot {
                commands: vec![(
                    "".to_owned(),
                    Cmd {
                        env: vec![
                            (
                                config::EnvVar::Replace("SUB_SELECT-v1".to_owned()),
                                Some("v1/exe".to_owned()),
                            ),
                            (
                                config::EnvVar::Replace("SUB_SELECT-v2".to_owned()),
                                Some("v2/binary".to_owned()),
                            ),
                        ]
                        .into_iter()
                        .collect::<IndexMap<_, _>>(),
                        exe: "{scie.files.dist-{scie.env.SELECT=v1}}/\
                            {scie.env.SUB_SELECT-{scie.env.SELECT=v2}}"
                            .to_string(),
                        args: vec![],
                        description: None,
                    },
                )]
                .into_iter()
                .collect::<IndexMap<_, _>>(),
                bindings: Default::default(),
            },
            files: vec![
                File {
                    name: "dist-v1".to_string(),
                    key: None,
                    size: 37,
                    hash: "def".to_string(),
                    file_type: FileType::Archive(ArchiveType::CompressedTar(Compression::Zstd)),
                    executable: None,
                    eager_extract: false,
                    source: Source::Scie,
                },
                File {
                    name: "dist-v2".to_string(),
                    key: None,
                    size: 42,
                    hash: "ghi".to_string(),
                    file_type: FileType::Archive(ArchiveType::Zip),
                    executable: None,
                    eager_extract: false,
                    source: Source::Scie,
                },
            ],
            other: None,
        };
        let installer = Installer::new(&[]);
        let current_exe = CurrentExe::from_path(Path::new("scie_path"));
        let mut context =
            Context::new(&current_exe, &jump, &lift, &installer, Some(HashMap::new())).unwrap();

        let cmd = lift.boot.commands.get("").unwrap();
        let expected_env = process::EnvVars {
            vars: vec![
                process::EnvVar::Replace(("SUB_SELECT-v1".into(), "v1/exe".into())),
                process::EnvVar::Replace(("SUB_SELECT-v2".into(), "v2/binary".into())),
            ],
        };

        let process = context.prepare_process(cmd).unwrap();
        assert_eq!(
            Process {
                env: expected_env.clone(),
                exe: PathBuf::from("/tmp/nce")
                    .join("def")
                    .join("dist-v1/v2/binary")
                    .into(),
                args: vec![],
            },
            process
        );

        let current_exe = CurrentExe::from_path(Path::new("scie_path"));
        context = Context::new(
            &current_exe,
            &jump,
            &lift,
            &installer,
            Some([("SELECT".into(), "v1".into())].into()),
        )
        .unwrap();
        let process = context.prepare_process(cmd).unwrap();
        assert_eq!(
            Process {
                env: expected_env.clone(),
                exe: PathBuf::from("/tmp/nce")
                    .join("def")
                    .join("dist-v1/v1/exe")
                    .into(),
                args: vec![],
            },
            process
        );

        let current_exe = CurrentExe::from_path(Path::new("scie_path"));
        context = Context::new(
            &current_exe,
            &jump,
            &lift,
            &installer,
            Some([("SELECT".into(), "v2".into())].into()),
        )
        .unwrap();
        let process = context.prepare_process(cmd).unwrap();
        assert_eq!(
            Process {
                env: expected_env,
                exe: PathBuf::from("/tmp/nce")
                    .join("ghi")
                    .join("dist-v2/v2/binary")
                    .into(),
                args: vec![],
            },
            process
        );
    }

    #[test]
    fn prepare_process_use_cmd_env_recursive() {
        let jump = Jump {
            size: 42,
            version: "0.1.0".to_string(),
        };
        let lift = Lift {
            name: "test".to_string(),
            description: None,
            base: Some("/tmp/nce".to_string()),
            load_dotenv: true,
            size: 137,
            hash: "abc".to_string(),
            boot: Boot {
                commands: vec![(
                    "".to_owned(),
                    Cmd {
                        env: vec![
                            (
                                config::EnvVar::Replace("A".to_owned()),
                                Some("{scie.env.B}".to_owned()),
                            ),
                            (
                                config::EnvVar::Replace("B".to_owned()),
                                Some("{scie.env.C}".to_owned()),
                            ),
                            (
                                config::EnvVar::Replace("C".to_owned()),
                                Some("{scie.env.D=c}".to_owned()),
                            ),
                            (
                                config::EnvVar::Replace("PATH".to_owned()),
                                Some("{scie.env.A}:{scie.env.E=e}:{scie.env.F}".to_owned()),
                            ),
                            (
                                config::EnvVar::Replace("F".to_owned()),
                                Some("{scie.env.PATH}".to_owned()),
                            ),
                        ]
                        .into_iter()
                        .collect::<IndexMap<_, _>>(),
                        exe: "{scie.env.A}".to_string(),
                        args: vec![],
                        description: None,
                    },
                )]
                .into_iter()
                .collect::<IndexMap<_, _>>(),
                bindings: Default::default(),
            },
            files: vec![],
            other: None,
        };
        let installer = Installer::new(&[]);
        let current_exe = CurrentExe::from_path(Path::new("scie_path"));
        let mut context = Context::new(
            &current_exe,
            &jump,
            &lift,
            &installer,
            Some([("PATH".into(), "/test/path".into())].into()),
        )
        .unwrap();

        let cmd = lift.boot.commands.get("").unwrap();

        let process = context.prepare_process(cmd).unwrap();
        let reified_path = "c:e:/test/path";
        assert_eq!(
            Process {
                env: process::EnvVars {
                    vars: vec![
                        process::EnvVar::Replace(("A".into(), "c".into())),
                        process::EnvVar::Replace(("B".into(), "c".into())),
                        process::EnvVar::Replace(("C".into(), "c".into())),
                        process::EnvVar::Replace(("PATH".into(), reified_path.into())),
                        process::EnvVar::Replace(("F".into(), reified_path.into())),
                    ]
                },
                exe: "c".into(),
                args: vec![],
            },
            process
        );

        let current_exe = CurrentExe::from_path(Path::new("scie_path"));
        context = Context::new(
            &current_exe,
            &jump,
            &lift,
            &installer,
            Some(
                [
                    ("D".into(), "d".into()),
                    ("PATH".into(), "/test/path".into()),
                ]
                .into(),
            ),
        )
        .unwrap();
        let process = context.prepare_process(cmd).unwrap();
        let reified_path = "d:e:/test/path";
        assert_eq!(
            Process {
                env: process::EnvVars {
                    vars: vec![
                        process::EnvVar::Replace(("A".into(), "d".into())),
                        process::EnvVar::Replace(("B".into(), "d".into())),
                        process::EnvVar::Replace(("C".into(), "d".into())),
                        process::EnvVar::Replace(("PATH".into(), reified_path.into())),
                        process::EnvVar::Replace(("F".into(), reified_path.into())),
                    ]
                },
                exe: "d".into(),
                args: vec![],
            },
            process
        );
    }

    #[test]
    fn prepare_process_use_scie_and_scie_argv0() {
        let jump = Jump {
            size: 42,
            version: "0.1.0".into(),
        };
        let lift = Lift {
            name: "test".into(),
            description: None,
            base: Some("/tmp/nce".into()),
            load_dotenv: true,
            size: 137,
            hash: "abc".into(),
            boot: Boot {
                commands: vec![(
                    "".to_owned(),
                    Cmd {
                        env: IndexMap::new(),
                        exe: "{scie}".into(),
                        args: vec![
                            "{scie.env.SCIE_ARGV0}".into(),
                            "{scie.argv0}".into(),
                            "{scie.env.SCIE}".into(),
                        ],
                        description: None,
                    },
                )]
                .into_iter()
                .collect::<IndexMap<_, _>>(),
                bindings: Default::default(),
            },
            files: vec![],
            other: None,
        };
        let installer = Installer::new(&[]);
        let current_exe = CurrentExe {
            exe: "exe".into(),
            invoked_as: "invoked_as".into(),
        };
        let mut context = Context::new(
            &current_exe,
            &jump,
            &lift,
            &installer,
            Some(
                [
                    ("SCIE".into(), "replace me".into()),
                    ("SCIE_ARGV0".into(), "replace me too".into()),
                ]
                .into(),
            ),
        )
        .unwrap();

        let cmd = lift.boot.commands.get("").unwrap();
        let process = context.prepare_process(cmd).unwrap();
        assert_eq!(
            Process {
                env: process::EnvVars { vars: vec![] },
                exe: "exe".into(),
                args: vec!["invoked_as".into(), "invoked_as".into(), "exe".into()],
            },
            process
        );
    }
}

// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::ffi::OsStr;
use std::fmt::{Debug, Formatter};
use std::path::{Component, Path, PathBuf};
use std::process::Child;

use bstr::ByteSlice;
use logging_timer::time;

use crate::atomic::{atomic_path, Target};
use crate::config::{Cmd, Fmt};
use crate::installer::Installer;
use crate::lift::{File, Lift};
use crate::placeholders::{self, Item, Placeholder, ScieBindingEnv};
use crate::process::{EnvVar, Process};
use crate::{config, EnvVars, Jump, Source};

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
                std::fs::OpenOptions::new()
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
                .execute(vec![("SCIE_BINDING_ENV".into(), lock.into())]);

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

pub(crate) struct Context<'a> {
    scie: &'a Path,
    lift: &'a Lift,
    base: PathBuf,
    installer: &'a Installer<'a>,
    files_by_name: BTreeMap<&'a str, &'a File>,
    replacements: HashSet<&'a File>,
    lift_manifest: LiftManifest,
    lift_manifest_dependants: HashSet<Process>,
    lift_manifest_installed: bool,
    bound: HashMap<&'a str, Binding>,
    installed: HashSet<File>,
}

fn try_as_str(os_str: &OsStr) -> Option<&str> {
    <[u8]>::from_os_str(os_str).and_then(|bytes| std::str::from_utf8(bytes).ok())
}

impl<'a> Context<'a> {
    #[time("debug", "Context::{}")]
    fn new(
        scie: &'a Path,
        jump: &'a Jump,
        lift: &'a Lift,
        installer: &'a Installer,
    ) -> Result<Self, String> {
        let mut files_by_name = BTreeMap::new();
        for file in &lift.files {
            files_by_name.insert(file.name.as_str(), file);
            if let Some(key) = file.key.as_ref() {
                files_by_name.insert(key.as_str(), file);
            }
        }
        let base = if let Ok(base) = env::var("SCIE_BASE") {
            PathBuf::from(base)
        } else if let Some(base) = &lift.base {
            base.clone()
        } else if let Some(dir) = dirs::cache_dir() {
            dir.join("nce")
        } else {
            PathBuf::from("~/.nce")
        };
        let base = expanduser(base.as_path())?;
        let lift_manifest = base.join(&lift.hash).join("lift.json");
        Ok(Context {
            scie,
            lift,
            base,
            installer,
            files_by_name,
            replacements: HashSet::new(),
            lift_manifest: LiftManifest {
                path: lift_manifest,
                jump: jump.clone(),
                lift: lift.clone(),
            },
            lift_manifest_dependants: HashSet::new(),
            lift_manifest_installed: false,
            bound: HashMap::new(),
            installed: HashSet::new(),
        })
    }

    fn prepare_process(&mut self, cmd: &'a Cmd) -> Result<Process, String> {
        let mut needs_lift_manifest = false;
        let (exe, needs_manifest) = self.reify_string(&cmd.exe)?;
        needs_lift_manifest |= needs_manifest;

        let mut args = vec![];
        for arg in &cmd.args {
            let (reified_arg, needs_manifest) = self.reify_string(arg)?;
            needs_lift_manifest |= needs_manifest;
            args.push(reified_arg.into());
        }
        let mut vars = vec![];
        for (key, value) in cmd.env.iter() {
            let final_value = match value {
                Some(val) => {
                    let (reified_value, needs_manifest) = self.reify_string(val)?;
                    needs_lift_manifest |= needs_manifest;
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
            if self.replacements.contains(&file) && !self.installed.contains(file) {
                if let Source::LoadBinding(binding_name) = &file.source {
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

    fn select_command(&mut self) -> Result<Option<SelectedCmd>, String> {
        if let Some(cmd) = env::var_os("SCIE_BOOT") {
            // Avoid subprocesses that re-execute this SCIE unintentionally getting in an infinite
            // loop.
            env::remove_var("SCIE_BOOT");
            let name = cmd.into_string().map_err(|value| {
                format!("Failed to decode environment variable SCIE_BOOT: {value:?}")
            })?;
            return self.select_cmd(&name, false);
        }
        if let Some(selected_cmd) = self.select_cmd("", false)? {
            return Ok(Some(selected_cmd));
        }

        #[cfg(target_family = "windows")]
        let basename = self.scie.file_stem().and_then(try_as_str);

        #[cfg(not(target_family = "windows"))]
        let basename = self.scie.file_name().and_then(try_as_str);

        if let Some(basename) = basename {
            if let Some(selected_command) = self.select_cmd(basename, false)? {
                return Ok(Some(selected_command));
            }
        }
        if let Some(argv1) = env::args().nth(1) {
            return self.select_cmd(&argv1, true);
        }
        Ok(None)
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

    fn parse_env(&mut self, env: &'a str) -> Result<(String, String, bool), String> {
        let (parsed_env, needs_lift_manifest) = self.reify_string(env)?;
        let (name, default) = match parsed_env.splitn(2, '=').collect::<Vec<_>>()[..] {
            [name] => (name, ""),
            [name, default] => (name, default),
            _ => {
                return Err(
                    "Expected {{scie.env.<name>}} <name> placeholder to be a non-empty \
                            string"
                        .to_string(),
                )
            }
        };
        Ok((name.to_string(), default.to_string(), needs_lift_manifest))
    }

    fn bind(&mut self, name: &'a str) -> Result<HashMap<String, String>, String> {
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
            self.bound.insert(name, boot_binding);
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

    fn reify_string(&mut self, value: &'a str) -> Result<(String, bool), String> {
        let mut reified = String::with_capacity(value.len());
        let mut lift_manifest_required = false;

        let parsed = placeholders::parse(value)?;
        for item in &parsed.items {
            match item {
                Item::LeftBrace => reified.push('{'),
                Item::Text(text) => reified.push_str(text),
                Item::Placeholder(Placeholder::FileHash(name)) => {
                    let (parsed_name, needs_manifest) = self.reify_string(name)?;
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
                    let (parsed_name, needs_manifest) = self.reify_string(name)?;
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
                Item::Placeholder(Placeholder::Env(env)) => {
                    let (name, default, needs_manifest) = self.parse_env(env)?;
                    lift_manifest_required |= needs_manifest;
                    let env_var = env::var_os(&name).unwrap_or_else(|| default.into());
                    let value = env_var.into_string().map_err(|value| {
                        format!("Failed to decode env var {name} as utf-8 value: {value:?}")
                    })?;
                    reified.push_str(&value)
                }
                Item::Placeholder(Placeholder::Scie) => reified.push_str(path_to_str(self.scie)?),
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
                Item::Placeholder(Placeholder::ScieBindingEnv(ScieBindingEnv { binding, env })) => {
                    let binding_env = self.bind(binding)?;
                    let (name, default, needs_manifest) = self.parse_env(env)?;
                    lift_manifest_required |= needs_manifest;
                    let value = binding_env
                        .get(name.as_str())
                        .map(String::to_owned)
                        .unwrap_or(default);
                    reified.push_str(&value)
                }
                Item::Placeholder(Placeholder::ScieLift) => {
                    lift_manifest_required = true;
                    reified.push_str(path_to_str(&self.lift_manifest.path)?);
                }
                Item::Placeholder(Placeholder::SciePlatform) => reified.push_str(
                    format!(
                        "{os}-{arch}",
                        os = env::consts::OS,
                        arch = env::consts::ARCH
                    )
                    .as_str(),
                ),
                Item::Placeholder(Placeholder::SciePlatformArch) => {
                    reified.push_str(env::consts::ARCH)
                }
                Item::Placeholder(Placeholder::SciePlatformOs) => reified.push_str(env::consts::OS),
            }
        }
        Ok((reified, lift_manifest_required))
    }
}

pub(crate) fn select_command(
    scie: &Path,
    jump: &Jump,
    lift: &Lift,
    installer: &Installer,
) -> Result<Option<SelectedCmd>, String> {
    let mut context = Context::new(scie, jump, lift, installer)?;
    context.select_command()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Context;
    use crate::config::{Boot, FileType};
    use crate::installer::Installer;
    use crate::{File, Jump, Lift, Source};

    #[test]
    fn env() {
        let jump = Jump {
            size: 42,
            version: "0.1.0".to_string(),
        };
        let tempdir = tempfile::tempdir().unwrap();
        let lift = Lift {
            name: "test".to_string(),
            description: None,
            base: Some(tempdir.path().to_path_buf()),
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
        let mut context = Context::new(Path::new("scie_path"), &jump, &lift, &installer).unwrap();

        assert!(std::env::var_os("__DNE__").is_none());
        assert_eq!(
            ("".to_string(), false),
            context.reify_string("{scie.env.__DNE__}").unwrap()
        );
        assert_eq!(
            ("default".to_string(), false),
            context.reify_string("{scie.env.__DNE__=default}").unwrap()
        );

        std::env::set_var("__DNE__", "foo");
        assert_eq!(
            ("foo".to_string(), false),
            context.reify_string("{scie.env.__DNE__=default}").unwrap()
        );
        std::env::remove_var("__DNE__");

        assert_eq!(
            ("scie_path".to_string(), false),
            context.reify_string("{scie.env.__DNE__={scie}}").unwrap()
        );
        assert_eq!(
            (
                tempdir
                    .path()
                    .join("abc")
                    .join("lift.json")
                    .to_str()
                    .unwrap()
                    .to_string(),
                true
            ),
            context
                .reify_string("{scie.env.__DNE__={scie.lift}}")
                .unwrap()
        );
        assert_eq!(
            (
                tempdir
                    .path()
                    .join("def")
                    .join("file")
                    .to_str()
                    .unwrap()
                    .to_string(),
                false
            ),
            context.reify_string("{scie.env.__DNE__={file}}").unwrap()
        );

        assert!(std::env::var_os("__DNE2__").is_none());
        assert_eq!(
            ("42".to_string(), false),
            context
                .reify_string("{scie.env.__DNE__={scie.env.__DNE2__=42}}")
                .unwrap()
        );
        std::env::set_var("__DNE2__", "bar");
        assert_eq!(
            ("bar".to_string(), false),
            context
                .reify_string("{scie.env.__DNE__={scie.env.__DNE2__=42}}")
                .unwrap()
        );
        std::env::remove_var("__DNE2__");
    }
}

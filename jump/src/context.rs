// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::collections::{BTreeMap, HashSet};
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

use bstr::ByteSlice;
use indexmap::IndexMap;
use logging_timer::time;

use crate::atomic::{atomic_path, Target};
use crate::config::{Cmd, Fmt};
use crate::lift::{File, Lift};
use crate::placeholders::{self, Item, Placeholder};
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

#[derive(Debug)]
pub(crate) enum FileEntry {
    Skip(usize),
    Install((File, PathBuf)),
    LoadAndInstall((Process, File, PathBuf)),
    ScieTote((File, Vec<(File, PathBuf)>)),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Binding {
    target: PathBuf,
    process: Process,
}

impl Binding {
    pub(crate) fn execute(self) -> Result<Option<()>, String> {
        atomic_path(&self.target.clone(), Target::File, |lock| {
            trace!("Installing boot binding {binding:#?}", binding = &self);
            match self.process.execute() {
                Err(err) => Err(format!("Failed to launch boot binding: {err}")),
                Ok(exit_status) if !exit_status.success() => {
                    Err(format!("Boot binding command failed: {exit_status}"))
                }
                _ => std::fs::write(lock, b"").map_err(|e| {
                    format!(
                        "Failed to touch lock file {path}: {e}",
                        path = lock.display()
                    )
                }),
            }
        })
    }
}

pub(crate) struct SelectedCmd {
    pub(crate) process: Process,
    pub(crate) bindings: Vec<Binding>,
    pub(crate) files: Vec<FileEntry>,
    pub(crate) argv1_consumed: bool,
}

pub(crate) struct Context<'a> {
    scie: &'a Path,
    jump: &'a Jump,
    lift: &'a Lift,
    base: PathBuf,
    files_by_name: BTreeMap<&'a str, &'a File>,
    replacements: HashSet<&'a File>,
    bindings: IndexMap<&'a str, Binding>,
}

fn try_as_str(os_str: &OsStr) -> Option<&str> {
    <[u8]>::from_os_str(os_str).and_then(|bytes| std::str::from_utf8(bytes).ok())
}

impl<'a> Context<'a> {
    #[time("debug")]
    fn new(scie: &'a Path, jump: &'a Jump, lift: &'a Lift) -> Result<Self, String> {
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
        Ok(Context {
            scie,
            jump,
            lift,
            base: expanduser(base.as_path())?,
            files_by_name,
            replacements: HashSet::new(),
            bindings: IndexMap::new(),
        })
    }

    fn prepare_process(&mut self, cmd: &'a Cmd) -> Result<Process, String> {
        let vars = cmd
            .env
            .iter()
            .map(|(key, value)| {
                self.reify_string(value)
                    .map(|v| (EnvVar::from(key), OsString::from(v)))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let exe = self.reify_string(&cmd.exe)?.into();
        let args = cmd
            .args
            .iter()
            .map(|string| self.reify_string(string).map(OsString::from))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Process {
            env: EnvVars { vars },
            exe,
            args,
        })
    }

    fn prepare(&mut self, cmd: &'a Cmd) -> Result<(Process, Vec<Binding>, Vec<FileEntry>), String> {
        let process = self.prepare_process(cmd)?;

        let mut load_entries = vec![];
        for file in &self.lift.files {
            if let Source::LoadBinding(binding_name) = &file.source {
                let path = self.get_path(file);
                load_entries.push(FileEntry::LoadAndInstall((
                    self.prepare_process(
                        self.lift
                            .boot
                            .bindings
                            .get(binding_name)
                            .ok_or_else(|| format!("No boot binding named {binding_name}."))?,
                    )?,
                    file.clone(),
                    path,
                )))
            }
        }

        let mut scie_tote = vec![];
        let mut file_entries = vec![];
        for (index, file) in self.lift.files.iter().enumerate() {
            if self.replacements.contains(&file) {
                let path = self.get_path(file);
                if file.size == 0 {
                    scie_tote.push((file.clone(), path));
                } else if Source::Scie == file.source {
                    file_entries.push(FileEntry::Install((file.clone(), path)));
                }
            } else if index < self.lift.files.len() - 1 || scie_tote.is_empty() {
                file_entries.push(FileEntry::Skip(file.size))
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

        Ok((
            process,
            self.bindings
                .values()
                .map(Binding::clone)
                .collect::<Vec<_>>(),
            file_entries,
        ))
    }

    fn select_cmd(
        &mut self,
        name: &str,
        argv1_consumed: bool,
    ) -> Result<Option<SelectedCmd>, String> {
        if let Some(cmd) = self.lift.boot.commands.get(name) {
            let (process, bindings, files) = self.prepare(cmd)?;
            return Ok(Some(SelectedCmd {
                process,
                bindings,
                files,
                argv1_consumed,
            }));
        }
        Ok(None)
    }

    fn select_command(&mut self) -> Result<Option<SelectedCmd>, String> {
        if let Some(cmd) = env::var_os("SCIE_BOOT") {
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

    fn record_lift_manifest(&self) -> Result<PathBuf, String> {
        let manifest = self.base.join(&self.lift.hash).join("lift.json");
        atomic_path(&manifest, Target::File, |path| {
            config(self.jump.clone(), self.lift.clone()).serialize(
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map_err(|e| {
                        format!(
                            "Failed top open lift manifest at {path} for writing: {e}",
                            path = manifest.display()
                        )
                    })?,
                Fmt::new().trailing_newline(true).pretty(true),
            )
        })?;
        Ok(manifest)
    }

    fn reify_string(&mut self, value: &'a str) -> Result<String, String> {
        let mut reified = String::with_capacity(value.len());

        let parsed = placeholders::parse(value)?;
        for item in &parsed.items {
            match item {
                Item::LeftBrace => reified.push('{'),
                Item::Text(text) => reified.push_str(text),
                Item::Placeholder(Placeholder::FileName(name)) => {
                    let file = self
                        .files_by_name
                        .get(name)
                        .ok_or_else(|| format!("No file named {name} is stored in this scie."))?;
                    let path = self.get_path(file);
                    reified.push_str(path_to_str(&path)?);
                    self.replacements.insert(file);
                }
                Item::Placeholder(Placeholder::Env(text)) => {
                    let parsed_env = self.reify_string(text)?;
                    let (name, default) = match parsed_env.splitn(2, '=').collect::<Vec<_>>()[..] {
                        [name] => (name, ""),
                        [name, default] => (name, default),
                        _ => return Err(
                            "Expected {{scie.env.<name>}} <name> placeholder to be a non-empty \
                            string"
                                .to_string(),
                        ),
                    };
                    let env_var = env::var_os(name).unwrap_or_else(|| default.into());
                    let value = env_var.into_string().map_err(|value| {
                        format!("Failed to decode env var {name} as utf-8 value: {value:?}")
                    })?;
                    reified.push_str(&value)
                }
                Item::Placeholder(Placeholder::Scie) => reified.push_str(path_to_str(self.scie)?),
                Item::Placeholder(Placeholder::ScieBindings) => {
                    reified.push_str(path_to_str(
                        &self.base.join(&self.lift.hash).join("bindings"),
                    )?);
                }
                Item::Placeholder(Placeholder::ScieBindingCmd(name)) => {
                    if !self.bindings.contains_key(name) {
                        let boot_binding = Binding {
                            target: self.base.join(&self.lift.hash).join("locks").join(name),
                            process: self.prepare_process(
                                self.lift
                                    .boot
                                    .bindings
                                    .get(*name)
                                    .ok_or_else(|| format!("No boot binding named {name}."))?,
                            )?,
                        };
                        self.bindings.insert(name, boot_binding);
                    }
                    reified.push_str(path_to_str(
                        &self.base.join(&self.lift.hash).join("bindings"),
                    )?);
                }
                Item::Placeholder(Placeholder::ScieLift) => {
                    let manifest = self.record_lift_manifest()?;
                    reified.push_str(path_to_str(&manifest)?);
                }
            }
        }
        Ok(reified)
    }
}

pub(crate) fn select_command(
    scie: &Path,
    jump: &Jump,
    lift: &Lift,
) -> Result<Option<SelectedCmd>, String> {
    let mut context = Context::new(scie, jump, lift)?;
    context.select_command()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Context;
    use crate::config::{Boot, FileType};
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
        let mut context = Context::new(Path::new("scie_path"), &jump, &lift).unwrap();

        assert!(std::env::var_os("__DNE__").is_none());
        assert_eq!(
            "",
            context.reify_string("{scie.env.__DNE__}").unwrap().as_str()
        );
        assert_eq!(
            "default",
            context
                .reify_string("{scie.env.__DNE__=default}")
                .unwrap()
                .as_str()
        );

        std::env::set_var("__DNE__", "foo");
        assert_eq!(
            "foo",
            context
                .reify_string("{scie.env.__DNE__=default}")
                .unwrap()
                .as_str()
        );
        std::env::remove_var("__DNE__");

        assert_eq!(
            "scie_path",
            context
                .reify_string("{scie.env.__DNE__={scie}}")
                .unwrap()
                .as_str()
        );
        assert_eq!(
            tempdir
                .path()
                .join("abc")
                .join("lift.json")
                .to_str()
                .unwrap(),
            context
                .reify_string("{scie.env.__DNE__={scie.lift}}")
                .unwrap()
                .as_str()
        );
        assert_eq!(
            tempdir.path().join("def").join("file").to_str().unwrap(),
            context
                .reify_string("{scie.env.__DNE__={file}}")
                .unwrap()
                .as_str()
        );

        assert!(std::env::var_os("__DNE2__").is_none());
        assert_eq!(
            "42",
            context
                .reify_string("{scie.env.__DNE__={scie.env.__DNE2__=42}}")
                .unwrap()
                .as_str()
        );
        std::env::set_var("__DNE2__", "bar");
        assert_eq!(
            "bar",
            context
                .reify_string("{scie.env.__DNE__={scie.env.__DNE2__=42}}")
                .unwrap()
                .as_str()
        );
        std::env::remove_var("__DNE2__");
    }
}

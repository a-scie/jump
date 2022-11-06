// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::collections::{BTreeMap, HashSet};
use std::env;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use bstr::ByteSlice;
use logging_timer::time;

use crate::config::Cmd;
use crate::lift::{File, Lift};
use crate::placeholders::{self, Item, Placeholder};

fn expanduser(path: PathBuf) -> Result<PathBuf, String> {
    if !<[u8]>::from_path(&path)
        .ok_or_else(|| {
            format!(
                "Failed to decode the path {} as utf-8 bytes",
                path.display()
            )
        })?
        .contains(&b'~')
    {
        return Ok(path);
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

pub struct Boot {
    pub name: String,
    pub description: Option<String>,
}

pub(crate) struct SelectedCmd {
    pub(crate) scie: PathBuf,
    pub(crate) cmd: Cmd,
    pub(crate) argv1_consumed: bool,
}

pub(crate) struct Context {
    scie: PathBuf,
    commands: BTreeMap<String, Cmd>,
    _bindings: BTreeMap<String, Cmd>,
    base: PathBuf,
    files_by_name: BTreeMap<String, File>,
    pub(crate) files: Vec<File>,
    pub(crate) replacements: HashSet<File>,
    pub(crate) description: Option<String>,
}

fn try_as_str(os_str: &OsStr) -> Option<&str> {
    <[u8]>::from_os_str(os_str).and_then(|bytes| std::str::from_utf8(bytes).ok())
}

impl Context {
    #[time("debug")]
    pub(crate) fn new(scie_path: PathBuf, lift: Lift) -> Result<Self, String> {
        let mut files_by_name = BTreeMap::new();
        for file in &lift.files {
            files_by_name.insert(file.name.clone(), file.clone());
            if let Some(key) = file.key.as_ref() {
                files_by_name.insert(key.clone(), file.clone());
            }
        }
        Ok(Context {
            scie: scie_path,
            description: lift.description,
            commands: lift.boot.commands,
            _bindings: lift.boot.bindings,
            base: expanduser(lift.base)?,
            files_by_name,
            files: lift.files,
            replacements: HashSet::new(),
        })
    }

    #[cfg(target_family = "windows")]
    fn scie_basename(&self) -> Option<&str> {
        self.scie.file_stem().and_then(try_as_str)
    }

    #[cfg(not(target_family = "windows"))]
    fn scie_basename(&self) -> Option<&str> {
        self.scie.file_name().and_then(try_as_str)
    }

    fn select_cmd(&self, name: &str, argv1_consumed: bool) -> Option<SelectedCmd> {
        self.commands.get(name).map(|cmd| SelectedCmd {
            scie: self.scie.clone(),
            cmd: cmd.clone(),
            argv1_consumed,
        })
    }

    pub(crate) fn select_command(&self) -> Result<Option<SelectedCmd>, String> {
        if let Some(cmd) = env::var_os("SCIE_BOOT") {
            let name = cmd.into_string().map_err(|value| {
                format!("Failed to decode environment variable SCIE_BOOT: {value:?}")
            })?;
            return Ok(self.select_cmd(&name, false));
        }
        Ok(self
            .select_cmd("", false)
            .or_else(|| {
                self.scie_basename()
                    .and_then(|basename| self.select_cmd(basename, false))
            })
            .or_else(|| {
                env::args()
                    .nth(1)
                    .and_then(|argv1| self.select_cmd(&argv1, true))
            }))
    }

    pub(crate) fn boots(&self) -> Vec<Boot> {
        self.commands
            .iter()
            .map(|(name, cmd)| Boot {
                name: name.to_string(),
                description: cmd.description.clone(),
            })
            .collect::<Vec<_>>()
    }

    pub(crate) fn get_file(&self, name: &str) -> Option<&File> {
        self.files_by_name.get(name)
    }

    pub(crate) fn get_path(&self, file: &File) -> PathBuf {
        self.base.join(&file.hash).join(&file.name)
    }

    pub(crate) fn reify_string(&mut self, value: &str) -> Result<String, String> {
        let mut reified = String::with_capacity(value.len());

        let parsed = placeholders::parse(value)?;
        for item in &parsed.items {
            match item {
                Item::LeftBrace => reified.push('{'),
                Item::Text(text) => reified.push_str(text),
                Item::Placeholder(Placeholder::FileName(name)) => {
                    let file = self
                        .get_file(name)
                        .ok_or_else(|| format!("No file named {name} is stored in this scie."))?;
                    let path = self.get_path(file);
                    reified.push_str(path_to_str(&path)?);
                    self.replacements.insert(file.clone());
                }
                Item::Placeholder(Placeholder::Env(name)) => {
                    let env_var = env::var_os(name).unwrap_or_else(|| "".into());
                    let value = env_var.into_string().map_err(|value| {
                        format!("Failed to decode env var {name} as utf-8 value: {value:?}")
                    })?;
                    reified.push_str(&value)
                }
                Item::Placeholder(Placeholder::Scie) => reified.push_str(path_to_str(&self.scie)?),
                // TODO(John Sirois): Handle these as part of tackling  #7
                Item::Placeholder(Placeholder::ScieBoot) => {
                    return Err("The {scie.boot} placeholder is not supported yet".to_string())
                }
                Item::Placeholder(Placeholder::ScieBootCmd(_name)) => {
                    return Err("The {scie.boot.<cmd>} placeholder is not supported yet".to_string())
                }
            }
        }
        Ok(reified)
    }
}

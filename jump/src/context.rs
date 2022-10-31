use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use bstr::ByteSlice;
use logging_timer::time;

use crate::config::{Archive, Cmd, File, Scie};
use crate::placeholders;
use crate::placeholders::{Item, Placeholder};

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
    pub(crate) cmd: Cmd,
    pub(crate) argv1_consumed: bool,
}

pub(crate) struct Context {
    scie: PathBuf,
    commands: HashMap<String, Cmd>,
    _bindings: HashMap<String, Cmd>,
    base: PathBuf,
    files_by_name: HashMap<String, File>,
    pub(crate) scie_jump_size: usize,
    pub(crate) config_size: usize,
    pub(crate) files: Vec<File>,
    pub(crate) replacements: HashSet<File>,
}

fn try_as_str(os_str: &OsStr) -> Option<&str> {
    <[u8]>::from_os_str(os_str).and_then(|bytes| std::str::from_utf8(bytes).ok())
}

impl Context {
    #[time("debug")]
    pub(crate) fn new(scie: Scie) -> Result<Self, String> {
        let scie_jump_size = scie
            .jump
            .as_ref()
            .ok_or_else(|| {
                format!(
                    "Creating a context requires a Scie with an identified Jump. Given {scie:?}"
                )
            })?
            .size;

        let mut files_by_name = HashMap::new();
        for file in &scie.lift.files {
            match file {
                File::Archive(Archive {
                    name: Some(ref name),
                    ..
                }) => {
                    files_by_name.insert(name.clone(), file.clone());
                }
                File::Blob(ref blob) => {
                    files_by_name.insert(blob.name.clone(), file.clone());
                }
                _ => (),
            }
        }
        Ok(Context {
            scie: scie.path,
            commands: scie.lift.boot.commands,
            _bindings: scie.lift.boot.bindings,
            base: expanduser(scie.lift.base)?,
            files_by_name,
            scie_jump_size,
            config_size: scie.lift.size,
            files: scie.lift.files,
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
        match file {
            File::Archive(archive) => self.base.join(&archive.hash),
            File::Blob(blob) => self.base.join(&blob.hash).join(&blob.name),
        }
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

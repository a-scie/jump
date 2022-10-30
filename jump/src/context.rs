use bstr::ByteSlice;
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use placeholders::{Item, Placeholder};

use crate::config::{Archive, Config, File};
use crate::Cmd;

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

pub struct Context {
    pub scie: PathBuf,
    pub scie_jump_size: usize,
    pub config_size: usize,
    pub cmd: Cmd,
    pub additional_commands: HashMap<String, Cmd>,
    pub root: PathBuf,
    pub files_by_name: HashMap<String, File>,
    pub replacements: HashSet<File>,
}

impl Context {
    pub fn new(scie: PathBuf, config: Config) -> Result<Self, String> {
        let mut files_by_name = HashMap::new();
        for file in config.files {
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
            scie,
            scie_jump_size: config.scie.size,
            config_size: config.size,
            cmd: config.command,
            additional_commands: config.additional_commands,
            root: expanduser(config.scie.root)?,
            files_by_name,
            replacements: HashSet::new(),
        })
    }

    pub fn command(&self) -> Result<&Cmd, String> {
        if let Some(cmd) = env::var_os("SCIE_CMD") {
            let name = cmd.into_string().map_err(|value| {
                format!("Failed to decode environment variable SCIE_CMD: {value:?}")
            })?;
            return self.additional_commands.get(&name).ok_or_else(|| {
                format!(
                    "The custom command specified by SCIE_CMD={name} is not a configured command \
                    in this binary. The following named commands are available: {commands}",
                    commands = self.additional_commands.keys().join(", ")
                )
            });
        }
        Ok(&self.cmd)
    }
    pub fn get_file(&self, name: &str) -> Option<&File> {
        self.files_by_name.get(name)
    }

    pub fn get_path(&self, file: &File) -> PathBuf {
        match file {
            File::Archive(archive) => self.root.join(&archive.fingerprint.hash),
            File::Blob(blob) => self.root.join(&blob.fingerprint.hash).join(&blob.name),
        }
    }

    pub fn reify_string(&mut self, value: &str) -> Result<String, String> {
        let mut reified = String::with_capacity(value.len());

        // let path_to_str = |path| {
        //     <[u8]>::from_path(path)
        //         .ok_or_else(|| format!("Failed to decode {} as a utf-8 path name", path.display()))?
        //         .to_str()
        //         .map_err(|e| format!("{e}"))
        // };

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

pub(crate) fn determine(current_exe: PathBuf, config: Config) -> Result<Context, String> {
    Context::new(current_exe, config)
}

use bstr::ByteSlice;
use itertools::Itertools;
use lazy_static::lazy_static;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use regex::{Captures, Regex, Replacer};

use crate::config::{Archive, Cmd, Config, File, Scie};

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
        return Ok(path.to_owned());
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

lazy_static! {
    static ref PARSER: Regex = Regex::new(r"\$\{(?P<name>[^}]+)}")
        .map_err(|e| format!("Invalid regex for replacing ${{name}}: {}", e))
        .unwrap();
}

struct FileIndex {
    root: PathBuf,
    files_by_name: HashMap<String, File>,
    errors: HashSet<String>,
}

impl FileIndex {
    fn new(scie: &Scie, files: &[File]) -> Result<Self, String> {
        let mut files_by_name = HashMap::new();
        for file in files {
            match file {
                File::Archive(Archive {
                    name: Some(name), ..
                }) => {
                    files_by_name.insert(name.clone(), file.to_owned());
                }
                File::Blob(blob) => {
                    files_by_name.insert(blob.name.clone(), file.to_owned());
                }
                _ => (),
            }
        }
        Ok(FileIndex {
            root: expanduser(&scie.root)?,
            files_by_name,
            errors: HashSet::new(),
        })
    }

    fn get_file(&self, name: &str) -> Option<&File> {
        self.files_by_name.get(name)
    }

    fn reify_string(&mut self, value: String) -> String {
        String::from(PARSER.replace_all(value.as_str(), self.by_ref()))
    }
}

impl Replacer for FileIndex {
    fn replace_append(&mut self, caps: &Captures<'_>, dst: &mut String) {
        let name = caps.name("name").unwrap().as_str();

        if let Some(file) = self.files_by_name.get(name) {
            let path = match file {
                File::Archive(archive) => self.root.join(&archive.fingerprint.hash),
                File::Blob(blob) => self.root.join(&blob.fingerprint.hash).join(&blob.name),
            };
            match <[u8]>::from_path(&path) {
                Some(path) => match std::str::from_utf8(path) {
                    Ok(path) => dst.push_str(path),
                    Err(err) => {
                        self.errors.insert(format!(
                            "Failed to convert file {} to path. Not UTF-8: {:?}: {}",
                            name, path, err
                        ));
                    }
                },
                None => {
                    self.errors
                        .insert(format!("Failed to convert file {} to a path.", name));
                }
            }
        } else {
            dst.push_str(name);
            self.errors.insert(name.to_string());
        }
    }
}

pub fn extract(_data: &[u8], mut config: Config) -> Result<Cmd, String> {
    let command =
        match std::env::var_os("SCIE_CMD") {
            Some(cmd) => {
                let name = cmd.into_string().map_err(|e| {
                    format!("Failed to decode environment variable SCIE_CMD: {:?}", e)
                })?;
                config.additional_commands.remove(&name).ok_or_else(|| {
                    format!(
                    "The custom command specified by SCIE_CMD={} is not a configured command in \
                    this binary. The following named commands are available: {}",
                    name, config.additional_commands.keys().join(", "))
                })?
            }
            None => config.command,
        };

    let mut file_index = FileIndex::new(&config.scie, &config.files)?;
    let mut to_extract = vec![];
    for name in &command.additional_files {
        let file = file_index.get_file(name.as_str()).ok_or_else(|| {
            format!(
                "The additional file {} requested by {:#?} was not found in this executable.",
                name, command
            )
        })?;
        to_extract.push(file);
    }

    let prepared_cmd = Cmd {
        exe: file_index.reify_string(command.exe),
        args: command
            .args
            .into_iter()
            .map(|string| file_index.reify_string(string))
            .collect::<Vec<_>>(),
        env: command
            .env
            .into_iter()
            .map(|(key, value)| (key, file_index.reify_string(value)))
            .collect::<HashMap<_, _>>(),
        additional_files: command.additional_files.clone(),
    };
    if !file_index.errors.is_empty() {
        return Err(format!(
            "Failed to find the following named files in this executable: {}",
            file_index.errors.iter().join(", ")
        ));
    }
    eprintln!("Prepared command:\n{:#?}", prepared_cmd);
    // TODO(John Sirois): XXX: Extract!
    Ok(prepared_cmd)
}

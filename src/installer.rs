use bstr::ByteSlice;
use itertools::Itertools;
use lazy_static::lazy_static;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use regex::{Captures, Regex, Replacer};

use crate::config::{Archive, Blob, Cmd, Config, File, Locator, Scie};

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
    replacements: HashSet<File>,
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
            replacements: HashSet::new(),
            errors: HashSet::new(),
        })
    }

    fn get_file(&self, name: &str) -> Option<&File> {
        self.files_by_name.get(name)
    }

    fn get_path(&self, file: &File) -> PathBuf {
        match file {
            File::Archive(archive) => self.root.join(&archive.fingerprint.hash),
            File::Blob(blob) => self.root.join(&blob.fingerprint.hash).join(&blob.name),
        }
    }

    fn reify_string(&mut self, value: String) -> String {
        String::from(PARSER.replace_all(value.as_str(), self.by_ref()))
    }
}

impl Replacer for FileIndex {
    fn replace_append(&mut self, caps: &Captures<'_>, dst: &mut String) {
        let name = caps.name("name").unwrap().as_str();

        if let Some(file) = self.files_by_name.get(name) {
            let path = self.get_path(file);
            match <[u8]>::from_path(&path) {
                Some(path) => match std::str::from_utf8(path) {
                    Ok(path) => {
                        dst.push_str(path);
                        self.replacements.insert(file.clone());
                    }
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
    let mut to_extract = HashSet::new();
    for name in &command.additional_files {
        let file = file_index.get_file(name.as_str()).ok_or_else(|| {
            format!(
                "The additional file {} requested by {:#?} was not found in this executable.",
                name, command
            )
        })?;
        to_extract.insert(file.clone());
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
        additional_files: command.additional_files,
    };
    if !file_index.errors.is_empty() {
        return Err(format!(
            "Failed to find the following named files in this executable: {}",
            file_index.errors.iter().join(", ")
        ));
    }
    eprintln!("Prepared command:\n{:#?}", prepared_cmd);

    for file in &file_index.replacements {
        to_extract.insert(file.clone());
    }
    eprintln!("To extract:\n{:#?}", to_extract);

    // TODO(John Sirois): XXX: Extract!
    // 1. rip through files in order -> if to_extract and Size extract and bump location.
    // 2. if still to_extract, open final slice as zip -> rip through files in order -> if to_extract and Entry extract from zip.
    let mut sized = vec![];
    let mut entries = vec![];
    if !to_extract.is_empty() {
        for file in config.files.iter().filter(|f| to_extract.contains(f)) {
            match file {
                File::Archive(Archive {
                    locator: Locator::Size(size),
                    fingerprint,
                    archive_type,
                    ..
                }) => sized.push((
                    size,
                    fingerprint,
                    file_index.get_path(file),
                    Some(archive_type),
                )),
                File::Blob(Blob {
                    locator: Locator::Size(size),
                    fingerprint,
                    ..
                }) => sized.push((size, fingerprint, file_index.get_path(file), None)),
                File::Archive(Archive {
                    locator: Locator::Entry(path),
                    fingerprint,
                    archive_type,
                    ..
                }) => entries.push((
                    path,
                    fingerprint,
                    file_index.get_path(file),
                    Some(archive_type),
                )),
                File::Blob(Blob {
                    locator: Locator::Entry(path),
                    fingerprint,
                    ..
                }) => entries.push((path, fingerprint, file_index.get_path(file), None)),
            };
        }
    }
    let mut step = 1;
    let mut location = config.scie.size;
    for (size, fingerprint, dst, archive_type) in sized {
        if (archive_type.is_some() && dst.is_dir()) || dst.is_file() {
            eprintln!("Step {}: already extracted: {}", step, dst.display());
        } else {
            eprintln!(
                "Step {}: extract {} bytes with fingerprint {:?} starting at {} to {} of archive type {:?}",
                step, size, fingerprint, location, dst.display(), archive_type
            );
        }
        step += 1;
        location += size;
    }
    for (path, fingerprint, dst, archive_type) in entries {
        if (archive_type.is_some() && dst.is_dir()) || dst.is_file() {
            eprintln!("Step {}: already extracted: {}", step, dst.display());
        } else {
            eprintln!(
                "Step {}: extract {} with fingerprint {:?} from trailer zip at {} to {} with archive type {:?}",
                step,
                path.display(),
                fingerprint,
                location,
                dst.display(),
                archive_type
            );
        }
        step += 1;
    }

    Ok(prepared_cmd)
}

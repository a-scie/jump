// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::path::Path;

use bstr::ByteSlice;
use logging_timer::time;

use crate::config::{ArchiveType, Boot, Config, FileType, Jump, Other};
use crate::{archive, fingerprint};

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum Source {
    Scie,
    LoadBinding(String),
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct File {
    pub name: String,
    pub key: Option<String>,
    pub size: usize,
    pub hash: String,
    pub file_type: FileType,
    pub executable: Option<bool>,
    pub eager_extract: bool,
    pub source: Source,
}

impl From<File> for crate::config::File {
    fn from(value: File) -> Self {
        Self {
            name: value.name,
            key: value.key,
            size: match value.size {
                0 => None,
                size => Some(size),
            },
            hash: Some(value.hash),
            file_type: Some(value.file_type),
            executable: value.executable,
            eager_extract: value.eager_extract,
            source: match value.source {
                Source::Scie => None,
                Source::LoadBinding(binding_name) => Some(binding_name),
            },
        }
    }
}

#[derive(Clone, Debug)]
pub struct Lift {
    pub name: String,
    pub description: Option<String>,
    pub base: Option<String>,
    pub(crate) load_dotenv: bool,
    pub size: usize,
    pub hash: String,
    pub boot: Boot,
    pub files: Vec<File>,
    pub(crate) other: Option<Other>,
}

pub struct ScieBoot {
    pub name: String,
    pub description: Option<String>,
    pub default: bool,
}

impl Lift {
    pub(crate) fn boots(&self) -> Vec<ScieBoot> {
        self.boot
            .commands
            .iter()
            .map(|(name, cmd)| {
                let default = name.is_empty();
                let name = if default {
                    self.name.clone()
                } else {
                    name.to_string()
                };
                let description = cmd.description.clone();
                ScieBoot {
                    name,
                    description,
                    default,
                }
            })
            .collect::<Vec<_>>()
    }
}

impl From<Lift> for crate::config::Lift {
    fn from(value: Lift) -> Self {
        crate::config::Lift {
            name: value.name,
            description: value.description,
            base: value.base,
            load_dotenv: if value.load_dotenv { Some(true) } else { None },
            boot: value.boot,
            files: value
                .files
                .into_iter()
                .map(|file| file.into())
                .collect::<Vec<_>>(),
        }
    }
}

fn determine_file_type(path: &Path) -> Result<FileType, String> {
    if path.is_dir() {
        return Ok(FileType::Directory);
    }
    if path.is_file() {
        if let Some(basename) = path.file_name() {
            let name = <[u8]>::from_os_str(basename)
                .ok_or_else(|| format!("Failed to decode {basename:?} as a utf-8 path name"))?
                .to_str()
                .map_err(|e| {
                    format!("Failed to interpret file name {basename:?} as a utf-8 string: {e}")
                })?;
            let ext = match name.rsplitn(3, '.').collect::<Vec<_>>()[..] {
                [_, "tar", stem] => name.trim_start_matches(stem).trim_start_matches('.'),
                [ext, ..] => ext,
                _ => {
                    return Err(format!(
                        "This archive has no type declared and it could not be guessed from \
                            its name: {name}",
                    ));
                }
            };
            let file_type = if let Some(archive_type) = ArchiveType::from_ext(ext) {
                FileType::Archive(archive_type)
            } else {
                FileType::Blob
            };
            return Ok(file_type);
        }
    }
    Err(format!(
        "Could not identify the file type of {path}",
        path = path.display()
    ))
}

#[cfg(windows)]
fn is_executable(_path: &Path) -> Result<bool, String> {
    Ok(false)
}

#[cfg(unix)]
fn is_executable(path: &Path) -> Result<bool, String> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = path.metadata().map_err(|e| {
        format!(
            "Failed to read metadata for {path}: {e}",
            path = path.display()
        )
    })?;
    Ok(metadata.permissions().mode() & 0o111 != 0)
}

#[time("debug", "lift::{}")]
fn assemble(
    resolve_base: &Path,
    config_files: Vec<crate::config::File>,
    reconstitute: bool,
) -> Result<Vec<File>, String> {
    let mut files = vec![];
    for file in config_files {
        let mut path = resolve_base.join(&file.name);

        let file_type = if let Some(file_type) = file.file_type {
            file_type
        } else if reconstitute {
            determine_file_type(&path)?
        } else {
            return Err(format!("A file type is required. Found: {file:?}"));
        };

        if reconstitute && file_type == FileType::Directory {
            path = archive::create(resolve_base, &file.name)?;
        }

        let (size, hash) = match file {
            crate::config::File {
                size: Some(size),
                hash: Some(hash),
                ..
            } => (size, hash),
            crate::config::File {
                size: None,
                hash: Some(hash),
                ..
            } => (0, hash), // A scie-tote entry.
            _ if reconstitute => fingerprint::digest_file(&path)?,
            file => {
                return Err(format!(
                    "Both file size and hash are required. Found: {file:?}"
                ));
            }
        };

        let executable = if let Some(executable) = file.executable {
            Some(executable)
        } else if reconstitute && path.is_file() && is_executable(&path)? {
            Some(true)
        } else {
            None
        };

        files.push(File {
            name: file.name,
            key: file.key,
            size,
            hash,
            file_type,
            executable,
            eager_extract: file.eager_extract,
            source: match file.source {
                None => Source::Scie,
                Some(binding_name) => Source::LoadBinding(binding_name),
            },
        });
    }
    Ok(files)
}

#[time("debug", "lift::{}")]
pub(crate) fn load_scie(scie_path: &Path, scie_data: &[u8]) -> Result<(Jump, Lift), String> {
    let end_of_zip = crate::zip::end_of_zip(scie_data, Config::MAXIMUM_CONFIG_SIZE)?;
    let result = load(scie_path, &scie_data[end_of_zip..], false).map_err(|e| {
        format!(
            "The scie at {scie_path} has missing information in its lift manifest: {e}",
            scie_path = scie_path.display()
        )
    })?;
    match result {
        (Some(jump), lift) => Ok((jump, lift)),
        _ => Err(format!(
            "The scie at {path} has a lift manifest with no scie-jump information.",
            path = scie_path.display()
        )),
    }
}

#[time("debug", "lift::{}")]
pub fn load_lift(manifest_path: &Path) -> Result<(Option<Jump>, Lift), String> {
    let data = std::fs::read(manifest_path).map_err(|e| {
        format!(
            "Failed to open lift manifest at {manifest}: {e}",
            manifest = manifest_path.display()
        )
    })?;
    load(manifest_path, &data, true)
}

fn load(
    manifest_path: &Path,
    data: &[u8],
    reconstitute: bool,
) -> Result<(Option<Jump>, Lift), String> {
    let config = Config::parse(data)?;
    let manifest_absolute_path = manifest_path.canonicalize().map_err(|e| {
        format!(
            "Failed to resolve an absolute path for the lift manifest {manifest}: {e}",
            manifest = manifest_path.display()
        )
    })?;
    let resolve_base = manifest_absolute_path
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let lift = config.scie.lift;
    let files = assemble(resolve_base, lift.files, reconstitute)?;
    Ok((
        config.scie.jump,
        Lift {
            name: lift.name,
            description: lift.description,
            base: lift.base,
            load_dotenv: lift.load_dotenv.unwrap_or(false),
            boot: lift.boot,
            size: data.len(),
            hash: fingerprint::digest(data),
            files,
            other: config.other,
        },
    ))
}

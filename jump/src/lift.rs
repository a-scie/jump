use std::path::{Path, PathBuf};

use bstr::ByteSlice;
use logging_timer::time;

use crate::config::{ArchiveType, BaseArchive, Boot, Config, Jump, Locator};
use crate::{fingerprint, pack};

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct Blob {
    pub name: String,
    pub key: Option<String>,
    pub locator: Locator,
    pub hash: String,
    pub always_extract: bool,
}

impl From<Blob> for crate::config::Blob {
    fn from(value: Blob) -> Self {
        crate::config::Blob(crate::config::BaseFile {
            name: value.name,
            key: value.key,
            locator: Some(value.locator),
            hash: Some(value.hash),
            always_extract: value.always_extract,
        })
    }
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub enum ArchiveSource {
    File,
    Directory,
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct Archive {
    pub name: String,
    pub key: Option<String>,
    pub locator: Locator,
    pub hash: String,
    pub archive_type: ArchiveType,
    pub source: ArchiveSource,
    pub always_extract: bool,
}

impl From<Archive> for crate::config::Archive {
    fn from(value: Archive) -> Self {
        crate::config::Archive {
            name: value.name,
            key: value.key,
            locator: Some(value.locator),
            hash: Some(value.hash),
            archive_type: value.archive_type,
            always_extract: value.always_extract,
        }
    }
}

impl From<Archive> for crate::config::Directory {
    fn from(value: Archive) -> Self {
        crate::config::Directory(BaseArchive {
            base: crate::config::BaseFile {
                name: value.name,
                key: value.key,
                locator: Some(value.locator),
                hash: Some(value.hash),
                always_extract: value.always_extract,
            },
            archive_type: Some(value.archive_type),
        })
    }
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub enum File {
    Archive(Archive),
    Blob(Blob),
}

impl From<File> for crate::config::File {
    fn from(value: File) -> Self {
        match value {
            File::Archive(archive) => match archive.source {
                ArchiveSource::File => crate::config::File::Archive(archive.into()),
                ArchiveSource::Directory => crate::config::File::Directory(archive.into()),
            },
            File::Blob(blob) => crate::config::File::Blob(blob.into()),
        }
    }
}

#[derive(Debug)]
pub struct Lift {
    pub name: String,
    pub description: Option<String>,
    pub base: PathBuf,
    pub size: usize,
    pub hash: String,
    pub boot: Boot,
    pub files: Vec<File>,
}

impl From<Lift> for crate::config::Lift {
    fn from(value: Lift) -> Self {
        crate::config::Lift {
            name: value.name,
            description: value.description,
            base: value.base,
            boot: value.boot,
            files: value
                .files
                .into_iter()
                .map(|file| file.into())
                .collect::<Vec<_>>(),
        }
    }
}

#[derive(Debug)]
pub struct Scie {
    pub jump: Option<Jump>,
    pub lift: Lift,
}

impl From<Scie> for crate::config::Scie {
    fn from(value: Scie) -> Self {
        crate::config::Scie {
            jump: value.jump,
            lift: value.lift.into(),
        }
    }
}

#[time("debug")]
fn assemble(
    resolve_base: &Path,
    config_files: Vec<crate::config::File>,
) -> Result<Vec<File>, String> {
    let mut files = vec![];
    for file in config_files {
        let assembled_file = match file {
            crate::config::File::Archive(archive) => {
                let (locator, hash) =
                    if let (Some(locator), Some(hash)) = (archive.locator, archive.hash) {
                        (locator.clone(), hash.to_string())
                    } else {
                        let path = resolve_base.join(&archive.name);
                        let (size, hash) = fingerprint::digest_file(&path)?;
                        (Locator::Size(size), hash)
                    };
                File::Archive(Archive {
                    name: archive.name,
                    key: archive.key,
                    locator,
                    hash,
                    archive_type: archive.archive_type,
                    source: ArchiveSource::File,
                    always_extract: archive.always_extract,
                })
            }
            crate::config::File::Blob(blob) => {
                let (locator, hash) =
                    if let (Some(locator), Some(hash)) = (blob.0.locator, blob.0.hash) {
                        (locator, hash)
                    } else {
                        let path = resolve_base.join(&blob.0.name);
                        let (size, hash) = fingerprint::digest_file(&path)?;
                        (Locator::Size(size), hash)
                    };
                File::Blob(Blob {
                    name: blob.0.name,
                    key: blob.0.key,
                    locator,
                    hash,
                    always_extract: blob.0.always_extract,
                })
            }
            crate::config::File::Directory(directory) => {
                let (name, locator, hash, archive_type) =
                    if let (Some(locator), Some(hash), Some(archive_type)) = (
                        directory.0.base.locator,
                        directory.0.base.hash,
                        directory.0.archive_type,
                    ) {
                        (directory.0.base.name, locator, hash, archive_type)
                    } else {
                        let (directory_archive, archive_type) = pack::create_archive(
                            resolve_base,
                            &directory.0.base.name,
                            directory.0.archive_type,
                        )?;
                        let relative_archive_path =
                            directory_archive.strip_prefix(resolve_base).map_err(|e| {
                                format!(
                                    "Failed to relativize archive path of {archive} created from \
                                    {directory} for a base directory of {base}: {e}",
                                    archive = directory_archive.display(),
                                    directory = directory.0.base.name,
                                    base = resolve_base.display()
                                )
                            })?;
                        let name_bytes =
                            <[u8]>::from_path(relative_archive_path).ok_or_else(|| {
                                format!("Failed read {relative_archive_path:?} as bytes")
                            })?;
                        let name = String::from_utf8(name_bytes.to_vec()).map_err(|e| {
                            format!(
                                "Failed to parse relative path {path} as utf8 string: {e}",
                                path = relative_archive_path.display()
                            )
                        })?;
                        let (size, hash) = fingerprint::digest_file(&directory_archive)?;
                        (name, Locator::Size(size), hash, archive_type)
                    };
                File::Archive(Archive {
                    name,
                    key: directory.0.base.key,
                    locator,
                    hash,
                    archive_type,
                    source: ArchiveSource::Directory,
                    always_extract: directory.0.base.always_extract,
                })
            }
        };
        files.push(assembled_file);
    }
    Ok(files)
}

#[time("debug")]
pub(crate) fn load_scie(scie_path: &Path, scie_data: &[u8]) -> Result<(Jump, Lift), String> {
    let end_of_zip = crate::zip::end_of_zip(scie_data, Config::MAXIMUM_CONFIG_SIZE)?;
    match load(scie_path, &scie_data[end_of_zip..])? {
        (Some(jump), lift) => Ok((jump, lift)),
        _ => Err(format!(
            "The scie at {path} has a lift manifest with no scie-jump information.",
            path = scie_path.display()
        )),
    }
}

#[time("debug")]
pub fn load_lift(manifest_path: &Path) -> Result<(Option<Jump>, Lift), String> {
    let data = std::fs::read(manifest_path).map_err(|e| {
        format!(
            "Failed to open lift manifest at {manifest}: {e}",
            manifest = manifest_path.display()
        )
    })?;
    load(manifest_path, &data)
}

fn load(manifest_path: &Path, data: &[u8]) -> Result<(Option<Jump>, Lift), String> {
    let config = Config::parse(data)?;
    let resolve_base = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .canonicalize()
        .map_err(|e| {
            format!(
                "Failed to resolve an absolute path for the parent directory of the lift \
                manifest {manifest}: {e}",
                manifest = manifest_path.display()
            )
        })?;
    let lift = config.scie.lift;
    let files = assemble(&resolve_base, lift.files)?;
    Ok((
        config.scie.jump,
        Lift {
            name: lift.name,
            description: lift.description,
            base: lift.base,
            boot: lift.boot,
            size: data.len(),
            hash: fingerprint::digest(data),
            files,
        },
    ))
}

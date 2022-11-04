use std::path::{Path, PathBuf};

use bstr::ByteSlice;
use logging_timer::time;

use crate::config::{ArchiveType, Boot, Config, FileType, Jump};
use crate::{archive, fingerprint};

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct File {
    pub name: String,
    pub key: Option<String>,
    pub size: usize,
    pub hash: String,
    pub file_type: FileType,
    pub always_extract: bool,
}

impl From<File> for crate::config::File {
    fn from(value: File) -> Self {
        Self {
            name: value.name,
            key: value.key,
            size: Some(value.size),
            hash: Some(value.hash),
            file_type: Some(value.file_type),
            always_extract: value.always_extract,
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
                    ))
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

#[time("debug")]
fn assemble(
    resolve_base: &Path,
    config_files: Vec<crate::config::File>,
) -> Result<Vec<File>, String> {
    let mut files = vec![];
    for file in config_files {
        let mut path = resolve_base.join(&file.name);

        let file_type = if let Some(file_type) = file.file_type {
            file_type
        } else {
            determine_file_type(&path)?
        };

        let (size, hash) = if let (Some(size), Some(hash)) = (file.size, file.hash) {
            (size, hash)
        } else {
            if FileType::Directory == file_type {
                path = archive::create(resolve_base, &file.name)?;
            }
            fingerprint::digest_file(&path)?
        };

        files.push(File {
            name: file.name,
            key: file.key,
            size,
            hash,
            file_type,
            always_extract: file.always_extract,
        });
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

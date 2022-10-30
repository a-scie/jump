use bstr::ByteSlice;
use logging_timer::{time, timer};
use std::collections::HashSet;
use std::ffi::OsString;
use std::io::Cursor;
use std::io::Write;

use crate::config::{
    Archive, ArchiveType, Blob, Compression, EnvVar as ConfigEnvVar, File, Locator,
};
use crate::context::Context;

#[derive(Debug)]
enum EnvVar {
    Default(OsString),
    Replace(OsString),
}

impl From<&ConfigEnvVar> for EnvVar {
    fn from(value: &ConfigEnvVar) -> Self {
        match value {
            ConfigEnvVar::Default(name) => Self::Default(name.to_owned().into()),
            ConfigEnvVar::Replace(name) => Self::Replace(name.to_owned().into()),
        }
    }
}

#[derive(Debug)]
pub struct EnvVars {
    vars: Vec<(EnvVar, OsString)>,
}

impl EnvVars {
    fn into_env_vars(self) -> impl Iterator<Item = (OsString, OsString)> {
        self.vars.into_iter().map(|(env_var, value)| match env_var {
            EnvVar::Default(name) => {
                let value = std::env::var_os(&name).unwrap_or(value);
                (name, value)
            }
            EnvVar::Replace(name) => (name, value),
        })
    }

    pub fn export(self) {
        for (name, value) in self.into_env_vars() {
            std::env::set_var(name, value);
        }
    }
}

#[derive(Debug)]
pub struct Process {
    pub env: EnvVars,
    pub exe: OsString,
    pub args: Vec<OsString>,
}

#[time("debug")]
pub fn prepare(data: &[u8], mut context: Context) -> Result<Process, String> {
    let command = context.command()?.clone();
    let mut to_extract = HashSet::new();
    for name in &command.additional_files {
        let file = context.get_file(name.as_str()).ok_or_else(|| {
            format!(
                "The additional file {name} requested by {command:#?} was not found in this \
                executable.",
            )
        })?;
        to_extract.insert(file.clone());
    }

    let exe = context.reify_string(&command.exe)?.into();
    let args = command
        .args
        .iter()
        .map(|string| context.reify_string(string).map(OsString::from))
        .collect::<Result<Vec<_>, _>>()?;
    let env = command
        .env
        .iter()
        .map(|(key, value)| {
            context
                .reify_string(value)
                .map(|v| (EnvVar::from(key), OsString::from(v)))
        })
        .collect::<Result<Vec<_>, _>>()?;

    for file in &context.replacements {
        to_extract.insert(file.clone());
    }

    // TODO(John Sirois): XXX: Extract!
    // 1. rip through files in order -> if to_extract and Size extract and bump location.
    // 2. if still to_extract, open final slice as zip -> rip through files in order -> if to_extract and Entry extract from zip.
    let mut sized = vec![];
    let mut entries = vec![];
    if !to_extract.is_empty() {
        for file in context
            .files_by_name
            .values()
            .filter(|f| to_extract.contains(f))
        {
            let dst = context.get_path(file);
            match file {
                File::Archive(Archive {
                    locator: Locator::Size(size),
                    fingerprint,
                    archive_type,
                    ..
                }) if !dst.is_dir() => sized.push((size, fingerprint, dst, Some(archive_type))),
                File::Blob(Blob {
                    locator: Locator::Size(size),
                    fingerprint,
                    ..
                }) if !dst.is_file() => sized.push((size, fingerprint, dst, None)),
                File::Archive(Archive {
                    locator: Locator::Entry(path),
                    fingerprint,
                    archive_type,
                    ..
                }) if !dst.is_dir() => entries.push((path, fingerprint, dst, Some(archive_type))),
                File::Blob(Blob {
                    locator: Locator::Entry(path),
                    fingerprint,
                    ..
                }) if !dst.is_file() => entries.push((path, fingerprint, dst, None)),
                _ => (),
            };
        }
    }

    // TODO(John Sirois): XXX: AtomicDirectory
    let mut location = context.scie_jump_size;
    for (size, _fingerprint, dst, archive_type) in sized {
        let bytes = &data[location..(location + size)];
        // TODO(John Sirois): XXX: Use fingerprint - insert hasher in stream stack to compare against.
        match archive_type {
            None => {
                let parent_dir = dst.parent().ok_or_else(|| "".to_owned())?;
                std::fs::create_dir_all(parent_dir).map_err(|e| format!("{e}"))?;
                let mut out = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(dst)
                    .map_err(|e| format!("{e}"))?;
                out.write_all(bytes).map_err(|e| format!("{e}"))?;
            }
            Some(archive) => {
                std::fs::create_dir_all(&dst).map_err(|e| format!("{e}"))?;
                let _timer = timer!("debug", "Unpacking {size} byte {archive:?}");
                match archive {
                    ArchiveType::Zip => {
                        let seekable_bytes = Cursor::new(bytes);
                        let mut zip = zip::ZipArchive::new(seekable_bytes)
                            .map_err(|e| format!("Failed to open {archive:?}: {e}"))?;
                        zip.extract(dst)
                            .map_err(|e| format!("Failed to extract {archive:?}: {e}"))?;
                    }
                    ArchiveType::Tar => {
                        let mut tar = tar::Archive::new(bytes);
                        tar.unpack(dst)
                            .map_err(|e| format!("Failed to unpack {archive:?}: {e}"))?;
                    }
                    ArchiveType::CompressedTar(Compression::Bzip2) => {
                        let bzip2_decoder = bzip2::read::BzDecoder::new(bytes);
                        let mut tar = tar::Archive::new(bzip2_decoder);
                        tar.unpack(dst)
                            .map_err(|e| format!("Failed to unpack {archive:?}: {e}"))?;
                    }
                    ArchiveType::CompressedTar(Compression::Gzip) => {
                        let gz_decoder = flate2::read::GzDecoder::new(bytes);
                        let mut tar = tar::Archive::new(gz_decoder);
                        tar.unpack(dst)
                            .map_err(|e| format!("Failed to unpack {archive:?}: {e}"))?;
                    }
                    ArchiveType::CompressedTar(Compression::Xz) => {
                        let xz_decoder = xz2::read::XzDecoder::new(bytes);
                        let mut tar = tar::Archive::new(xz_decoder);
                        tar.unpack(dst)
                            .map_err(|e| format!("Failed to unpack {archive:?}: {e}"))?;
                    }
                    ArchiveType::CompressedTar(Compression::Zlib) => {
                        let zlib_decoder = flate2::read::ZlibDecoder::new(bytes);
                        let mut tar = tar::Archive::new(zlib_decoder);
                        tar.unpack(dst)
                            .map_err(|e| format!("Failed to unpack {archive:?}: {e}"))?;
                    }
                    ArchiveType::CompressedTar(Compression::Zstd) => {
                        let zstd_decoder =
                            zstd::stream::Decoder::new(bytes).map_err(|e| format!("{e}"))?;
                        let mut tar = tar::Archive::new(zstd_decoder);
                        tar.unpack(dst)
                            .map_err(|e| format!("Failed to unpack {archive:?}: {e}"))?;
                    }
                }
            }
        }
        location += size;
    }

    if !entries.is_empty() {
        let seekable_bytes = Cursor::new(&data[location..(data.len() - context.config_size)]);
        let mut zip = zip::ZipArchive::new(seekable_bytes).map_err(|e| format!("{e}"))?;
        for (path, _fingerprint, dst, _archive_type) in entries {
            std::fs::create_dir_all(&dst).map_err(|e| format!("{e}"))?;
            let name = std::str::from_utf8(<[u8]>::from_path(path).ok_or_else(|| {
                format!(
                    "Failed to decode {} to a utf-8 zip entry name",
                    path.display()
                )
            })?)
            .map_err(|e| format!("{e}"))?;
            let zip_entry = zip.by_name(name).map_err(|e| format!("{e}"))?;
            todo!(
                "Use the extraction logic above to extract zip entry {} to {}",
                zip_entry.name(),
                dst.display()
            )
        }
    }

    Ok(Process {
        exe,
        args,
        env: EnvVars { vars: env },
    })
}

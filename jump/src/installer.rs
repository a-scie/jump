use std::collections::HashSet;
use std::ffi::OsString;
use std::io::Cursor;

use bstr::ByteSlice;
use logging_timer::{time, timer};

use crate::atomic::atomic_directory;
use crate::config::{ArchiveType, Cmd, Compression, Locator};
use crate::context::Context;
use crate::fingerprint;
use crate::lift::File;
use crate::process::{EnvVar, EnvVars, Process};

#[time("debug")]
pub(crate) fn prepare(mut context: Context, command: Cmd, data: &[u8]) -> Result<Process, String> {
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
            .files
            .clone()
            .into_iter()
            .filter(|f| to_extract.contains(f))
        {
            let dst = context.get_path(&file)?;
            match file {
                File::Archive(archive) if !dst.is_dir() => match archive.locator {
                    Locator::Size(size) => {
                        sized.push((size, archive.hash, dst, Some(archive.archive_type)))
                    }
                    Locator::Entry(path) => entries.push((
                        path.to_path_buf(),
                        archive.hash,
                        dst,
                        Some(archive.archive_type),
                    )),
                },
                File::Blob(blob) if !dst.is_file() => match &blob.locator {
                    Locator::Size(size) => sized.push((*size, blob.hash.clone(), dst, None)),
                    Locator::Entry(path) => {
                        entries.push((path.to_path_buf(), blob.hash.clone(), dst, None))
                    }
                },
                _ => {
                    debug!("Cache hit {dst} for {file:?}", dst = dst.display())
                }
            };
        }
    }

    let mut location = context.scie_jump_size;
    for (size, expected_hash, dst, archive_type) in sized {
        let bytes = &data[location..(location + size)];
        let actual_hash = fingerprint::digest(bytes);
        if expected_hash != actual_hash {
            return Err(format!(
                "Destination {dst} of size {size} had unexpected hash: {actual_hash}",
                dst = dst.display(),
            ));
        } else {
            debug!(
                "Destination {dst} of size {size} had expected hash",
                dst = dst.display()
            );
        }
        match archive_type {
            None => {
                let _timer = timer!("debug", "Unpacking {size} byte blob.");
                let parent_dir = dst.parent().ok_or_else(|| "".to_owned())?;
                atomic_directory(parent_dir, |work_dir| {
                    let blob_dst = work_dir.join(dst.file_name().ok_or_else(|| {
                        format!(
                            "Blob destination {dst} has no file name.",
                            dst = dst.display()
                        )
                    })?);
                    std::fs::write(&blob_dst, bytes).map_err(|e| {
                        format!(
                            "Failed to open blob destination {blob_dst} for writing: {e}",
                            blob_dst = blob_dst.display()
                        )
                    })
                })?
            }
            Some(archive) => {
                let _timer = timer!("debug", "Unpacking {size} byte {archive:?}.");
                atomic_directory(&dst, |work_dir| match archive {
                    ArchiveType::Zip => {
                        let seekable_bytes = Cursor::new(bytes);
                        let mut zip = zip::ZipArchive::new(seekable_bytes)
                            .map_err(|e| format!("Failed to open {archive:?}: {e}"))?;
                        zip.extract(work_dir)
                            .map_err(|e| format!("Failed to extract {archive:?}: {e}"))
                    }
                    ArchiveType::Tar => {
                        let mut tar = tar::Archive::new(bytes);
                        tar.unpack(work_dir)
                            .map_err(|e| format!("Failed to unpack {archive:?}: {e}"))
                    }
                    ArchiveType::CompressedTar(Compression::Bzip2) => {
                        let bzip2_decoder = bzip2::read::BzDecoder::new(bytes);
                        let mut tar = tar::Archive::new(bzip2_decoder);
                        tar.unpack(work_dir)
                            .map_err(|e| format!("Failed to unpack {archive:?}: {e}"))
                    }
                    ArchiveType::CompressedTar(Compression::Gzip) => {
                        let gz_decoder = flate2::read::GzDecoder::new(bytes);
                        let mut tar = tar::Archive::new(gz_decoder);
                        tar.unpack(work_dir)
                            .map_err(|e| format!("Failed to unpack {archive:?}: {e}"))
                    }
                    ArchiveType::CompressedTar(Compression::Xz) => {
                        let xz_decoder = xz2::read::XzDecoder::new(bytes);
                        let mut tar = tar::Archive::new(xz_decoder);
                        tar.unpack(work_dir)
                            .map_err(|e| format!("Failed to unpack {archive:?}: {e}"))
                    }
                    ArchiveType::CompressedTar(Compression::Zlib) => {
                        let zlib_decoder = flate2::read::ZlibDecoder::new(bytes);
                        let mut tar = tar::Archive::new(zlib_decoder);
                        tar.unpack(work_dir)
                            .map_err(|e| format!("Failed to unpack {archive:?}: {e}"))
                    }
                    ArchiveType::CompressedTar(Compression::Zstd) => {
                        let zstd_decoder =
                            zstd::stream::Decoder::new(bytes).map_err(|e| format!("{e}"))?;
                        let mut tar = tar::Archive::new(zstd_decoder);
                        tar.unpack(work_dir)
                            .map_err(|e| format!("Failed to unpack {archive:?}: {e}"))
                    }
                })?
            }
        }
        location += size;
    }

    if !entries.is_empty() {
        let seekable_bytes = Cursor::new(&data[location..(data.len() - context.config_size)]);
        let mut zip = zip::ZipArchive::new(seekable_bytes).map_err(|e| format!("{e}"))?;
        for (path, _fingerprint, dst, _archive_type) in entries {
            std::fs::create_dir_all(&dst).map_err(|e| format!("{e}"))?;
            let name = std::str::from_utf8(<[u8]>::from_path(&path).ok_or_else(|| {
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

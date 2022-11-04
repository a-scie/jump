use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{Cursor, Read, Seek};
use std::path::Path;

use bstr::ByteSlice;
use logging_timer::time;

use crate::atomic::atomic_directory;
use crate::config::{ArchiveType, Cmd, Compression, Locator};
use crate::context::Context;
use crate::fingerprint;
use crate::lift::File;
use crate::process::{EnvVar, EnvVars, Process};

#[time("debug")]
fn unpack_tar<R: Read>(archive_type: ArchiveType, tar_stream: R, dst: &Path) -> Result<(), String> {
    let mut tar = tar::Archive::new(tar_stream);
    tar.unpack(dst)
        .map_err(|e| format!("Failed to unpack {archive_type:?}: {e}"))
}

#[time(debug)]
fn unpack_archive<R: Read + Seek>(
    archive: ArchiveType,
    bytes: R,
    dst: &Path,
) -> Result<(), String> {
    atomic_directory(dst, |work_dir| match archive {
        ArchiveType::Zip => {
            let mut zip = zip::ZipArchive::new(bytes)
                .map_err(|e| format!("Failed to open {archive:?}: {e}"))?;
            zip.extract(work_dir)
                .map_err(|e| format!("Failed to extract {archive:?}: {e}"))
        }
        ArchiveType::Tar => unpack_tar(archive, bytes, work_dir),
        ArchiveType::CompressedTar(Compression::Bzip2) => {
            let bzip2_decoder = bzip2::read::BzDecoder::new(bytes);
            unpack_tar(archive, bzip2_decoder, work_dir)
        }
        ArchiveType::CompressedTar(Compression::Gzip) => {
            let gz_decoder = flate2::read::GzDecoder::new(bytes);
            unpack_tar(archive, gz_decoder, work_dir)
        }
        ArchiveType::CompressedTar(Compression::Xz) => {
            let xz_decoder = xz2::read::XzDecoder::new(bytes);
            unpack_tar(archive, xz_decoder, work_dir)
        }
        ArchiveType::CompressedTar(Compression::Zlib) => {
            let zlib_decoder = flate2::read::ZlibDecoder::new(bytes);
            unpack_tar(archive, zlib_decoder, work_dir)
        }
        ArchiveType::CompressedTar(Compression::Zstd) => {
            let zstd_decoder = zstd::stream::Decoder::new(bytes).map_err(|e| {
                format!(
                    "Failed to create a zstd decoder for unpacking to {dst}: {e}",
                    dst = dst.display()
                )
            })?;
            unpack_tar(archive, zstd_decoder, work_dir)
        }
    })
}

#[time("debug")]
fn unpack_blob<R: Read>(mut bytes: R, dst: &Path) -> Result<(), String> {
    let parent_dir = dst.parent().ok_or_else(|| "".to_owned())?;
    atomic_directory(parent_dir, |work_dir| {
        let blob_dst = work_dir.join(dst.file_name().ok_or_else(|| {
            format!(
                "Blob destination {dst} has no file name.",
                dst = dst.display()
            )
        })?);
        let mut blob_out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&blob_dst)
            .map_err(|e| {
                format!(
                    "Failed to open blob destination {blob_dst} for writing: {e}",
                    blob_dst = blob_dst.display()
                )
            })?;
        std::io::copy(&mut bytes, &mut blob_out)
            .map(|_| ())
            .map_err(|e| format!("{e}"))
    })
}

fn unpack<R: Read + Seek>(
    archive_type: Option<ArchiveType>,
    bytes: R,
    dst: &Path,
) -> Result<(), String> {
    if let Some(archive) = archive_type {
        unpack_archive(archive, bytes, dst)
    } else {
        unpack_blob(bytes, dst)
    }
}

#[time("debug")]
pub(crate) fn prepare(
    mut context: Context,
    command: Cmd,
    payload: &[u8],
) -> Result<Process, String> {
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

    let mut entries = vec![];
    let mut location = 0;
    for file in &context.files {
        match file {
            File::Archive(ref archive) => match &archive.locator {
                Locator::Size(size) => {
                    if to_extract.contains(file) {
                        let dst = context.get_path(file)?;
                        let bytes = &payload[location..(location + size)];
                        let actual_hash = fingerprint::digest(bytes);
                        if archive.hash != actual_hash {
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
                        unpack(Some(archive.archive_type), Cursor::new(bytes), &dst)?;
                    }
                    location += size
                }
                Locator::Entry(path) => {
                    if to_extract.contains(file) {
                        let dst = context.get_path(file)?;
                        entries.push((
                            path.to_path_buf(),
                            archive.hash.clone(),
                            dst,
                            Some(archive.archive_type),
                        ))
                    }
                }
            },
            File::Blob(ref blob) => match &blob.locator {
                Locator::Size(size) => {
                    if to_extract.contains(file) {
                        let dst = context.get_path(file)?;
                        let bytes = &payload[location..(location + size)];
                        let actual_hash = fingerprint::digest(bytes);
                        if blob.hash != actual_hash {
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
                        unpack(None, Cursor::new(bytes), &dst)?;
                    }
                    location += size
                }
                Locator::Entry(path) => {
                    if to_extract.contains(file) {
                        let dst = context.get_path(file)?;
                        entries.push((path.to_path_buf(), blob.hash.clone(), dst, None))
                    }
                }
            },
        }
    }

    if !entries.is_empty() {
        let seekable_bytes = Cursor::new(&payload[location..]);
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
                "Use unpack to extract zip entry {} to {}",
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

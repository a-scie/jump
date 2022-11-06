// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{Cursor, Read, Seek};
use std::path::Path;

use logging_timer::time;

use crate::atomic::{atomic_path, Target};
use crate::config::{ArchiveType, Cmd, Compression, FileType};
use crate::context::Context;
use crate::process::{EnvVar, EnvVars, Process};
use crate::{fingerprint, File};

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
    atomic_path(dst, Target::Directory, |work_dir| match archive {
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
    atomic_path(parent_dir, Target::Directory, |work_dir| {
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

fn unpack<R: Read + Seek>(file_type: FileType, bytes: R, dst: &Path) -> Result<(), String> {
    match file_type {
        FileType::Archive(archive_type) => unpack_archive(archive_type, bytes, dst),
        FileType::Blob => unpack_blob(bytes, dst),
        FileType::Directory => unpack_archive(ArchiveType::Zip, bytes, dst),
    }
}

fn file_type_to_unpack(file: &File, dst: &Path) -> Option<FileType> {
    match file.file_type {
        archive_type @ FileType::Archive(_) if !dst.is_dir() => Some(archive_type),
        FileType::Blob if !dst.is_file() => Some(FileType::Blob),
        FileType::Directory if !dst.is_dir() => Some(FileType::Directory),
        _ => None,
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

    let mut scie_tote = vec![];
    let mut location = 0;
    for file in &context.files {
        if to_extract.contains(file) {
            if file.size == 0 {
                scie_tote.push(file);
            } else {
                let dst = context.get_path(file);
                if let Some(file_type) = file_type_to_unpack(file, &dst) {
                    let bytes = &payload[location..(location + file.size)];
                    let actual_hash = fingerprint::digest(bytes);
                    if file.hash != actual_hash {
                        return Err(format!(
                            "Destination {dst} of size {size} had unexpected hash: {actual_hash}",
                            size = file.size,
                            dst = dst.display(),
                        ));
                    } else {
                        debug!(
                            "Destination {dst} of size {size} had expected hash",
                            size = file.size,
                            dst = dst.display()
                        );
                    }
                    unpack(file_type, Cursor::new(bytes), &dst)?;
                } else {
                    debug!("Cache hit {dst} for {file:?}", dst = dst.display())
                };
            }
        }
        location += file.size;
    }

    if !scie_tote.is_empty() {
        let tote_file = context.files.last().ok_or_else(|| {
            format!(
                "Expected the last file to be the scie-tote holding these files: {scie_tote:#?}"
            )
        })?;
        let scie_tote_dst = context.get_path(tote_file);
        let bytes = &payload[(location - tote_file.size)..location];
        unpack(tote_file.file_type, Cursor::new(bytes), &scie_tote_dst)?;
        for file in scie_tote {
            let dst = context.get_path(file);
            if let Some(file_type) = file_type_to_unpack(file, &dst) {
                let src_path = scie_tote_dst.join(&file.name);
                let src = std::fs::File::open(&src_path).map_err(|e| {
                    format!(
                        "Failed to open {file:?} at {src} from the unpacked scie-tote: {e}",
                        src = src_path.display()
                    )
                })?;
                unpack(file_type, &src, &dst)?;
            } else {
                debug!("Cache hit {dst} for {file:?}", dst = dst.display())
            };
        }
    }

    Ok(Process {
        exe,
        args,
        env: EnvVars { vars: env },
    })
}

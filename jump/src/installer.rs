// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::fs::OpenOptions;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;

use logging_timer::time;

use crate::atomic::{atomic_path, Target};
use crate::config::{ArchiveType, Compression, FileType};
use crate::context::FileEntry;
use crate::fingerprint;

fn check_hash<R: Read + Seek>(
    file_type: &str,
    mut bytes: R,
    expected_hash: &str,
    dst: &Path,
) -> Result<R, String> {
    let (size, actual_hash) = fingerprint::digest_reader(&mut bytes)?;
    if expected_hash != actual_hash.as_str() {
        return Err(format!(
            "The {file_type} destination {dst} of size {size} had unexpected hash: {actual_hash}",
            dst = dst.display(),
        ));
    } else {
        // TODO(John Sirois): Hash in-line with extraction.
        bytes
            .seek(SeekFrom::Start(0))
            .map_err(|e| format!("Failed to re-wind {file_type} after hashing: {e}"))?;
        debug!(
            "The {file_type} destination {dst} of size {size} had expected hash",
            dst = dst.display()
        );
        Ok(bytes)
    }
}

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
    expected_hash: &str,
    dst: &Path,
) -> Result<(), String> {
    atomic_path(dst, Target::Directory, |work_dir| {
        let hashed_bytes = check_hash(archive.as_ext(), bytes, expected_hash, dst)?;
        match archive {
            ArchiveType::Zip => {
                let mut zip = zip::ZipArchive::new(hashed_bytes)
                    .map_err(|e| format!("Failed to open {archive:?}: {e}"))?;
                zip.extract(work_dir)
                    .map_err(|e| format!("Failed to extract {archive:?}: {e}"))
            }
            ArchiveType::Tar => unpack_tar(archive, hashed_bytes, work_dir),
            ArchiveType::CompressedTar(Compression::Bzip2) => {
                let bzip2_decoder = bzip2::read::BzDecoder::new(hashed_bytes);
                unpack_tar(archive, bzip2_decoder, work_dir)
            }
            ArchiveType::CompressedTar(Compression::Gzip) => {
                let gz_decoder = flate2::read::GzDecoder::new(hashed_bytes);
                unpack_tar(archive, gz_decoder, work_dir)
            }
            ArchiveType::CompressedTar(Compression::Xz) => {
                let xz_decoder = xz2::read::XzDecoder::new(hashed_bytes);
                unpack_tar(archive, xz_decoder, work_dir)
            }
            ArchiveType::CompressedTar(Compression::Zlib) => {
                let zlib_decoder = flate2::read::ZlibDecoder::new(hashed_bytes);
                unpack_tar(archive, zlib_decoder, work_dir)
            }
            ArchiveType::CompressedTar(Compression::Zstd) => {
                let zstd_decoder = zstd::stream::Decoder::new(hashed_bytes).map_err(|e| {
                    format!(
                        "Failed to create a zstd decoder for unpacking to {dst}: {e}",
                        dst = dst.display()
                    )
                })?;
                unpack_tar(archive, zstd_decoder, work_dir)
            }
        }
    })
}

#[time("debug")]
fn unpack_blob<R: Read + Seek>(bytes: R, expected_hash: &str, dst: &Path) -> Result<(), String> {
    let parent_dir = dst.parent().ok_or_else(|| "".to_owned())?;
    atomic_path(parent_dir, Target::Directory, |work_dir| {
        let mut hashed_bytes = check_hash("blob", bytes, expected_hash, dst)?;
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
        std::io::copy(&mut hashed_bytes, &mut blob_out)
            .map(|_| ())
            .map_err(|e| format!("{e}"))
    })
}

fn unpack<R: Read + Seek>(
    file_type: FileType,
    bytes: R,
    expected_hash: &str,
    dst: &Path,
) -> Result<(), String> {
    match file_type {
        FileType::Archive(archive_type) => unpack_archive(archive_type, bytes, expected_hash, dst),
        FileType::Blob => unpack_blob(bytes, expected_hash, dst),
        FileType::Directory => unpack_archive(ArchiveType::Zip, bytes, expected_hash, dst),
    }
}

#[time("debug")]
pub(crate) fn install(files: &[FileEntry], payload: &[u8]) -> Result<(), String> {
    let mut scie_tote = vec![];
    let mut location = 0;
    for file_entry in files {
        let advance = match file_entry {
            FileEntry::Skip(size) => *size,
            FileEntry::Install((file, dst)) => {
                if file.size == 0 {
                    scie_tote.push((file, file.file_type, dst.clone()));
                } else {
                    let bytes = &payload[location..(location + file.size)];
                    unpack(file.file_type, Cursor::new(bytes), file.hash.as_str(), dst)?;
                }
                file.size
            }
        };
        location += advance;
    }

    if !scie_tote.is_empty() {
        if let FileEntry::Install((tote_file, _)) = files.last().ok_or_else(|| {
            format!(
                "Expected the last file to be the scie-tote holding these files: {scie_tote:#?}"
            )
        })? {
            let scie_tote_tmpdir = tempfile::TempDir::new().map_err(|e| {
                format!("Failed to create a temporary directory to extract the scie-tote to: {e}")
            })?;
            let scie_tote_path = scie_tote_tmpdir.path().join(&tote_file.name);
            let bytes = &payload[(location - tote_file.size)..location];
            unpack(
                tote_file.file_type,
                Cursor::new(bytes),
                tote_file.hash.as_str(),
                &scie_tote_path,
            )?;
            for (file, file_type, dst) in scie_tote {
                let src_path = scie_tote_path.join(&file.name);
                let src = std::fs::File::open(&src_path).map_err(|e| {
                    format!(
                        "Failed to open {file:?} at {src} from the unpacked scie-tote: {e}",
                        src = src_path.display()
                    )
                })?;
                unpack(file_type, &src, file.hash.as_str(), &dst)?;
            }
        }
    }

    Ok(())
}

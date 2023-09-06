// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::fs::{OpenOptions, Permissions};
use std::io::{Cursor, Read, Seek};
use std::path::Path;

use logging_timer::time;
use tempfile::TempDir;

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
        Err(format!(
            "The {file_type} destination {dst} of size {size} had unexpected hash: {actual_hash}",
            dst = dst.display(),
        ))
    } else {
        // TODO(John Sirois): Hash in-line with extraction.
        bytes
            .rewind()
            .map_err(|e| format!("Failed to re-wind {file_type} after hashing: {e}"))?;
        debug!(
            "The {file_type} destination {dst} of size {size} had expected hash",
            dst = dst.display()
        );
        Ok(bytes)
    }
}

#[time("debug", "installer::{}")]
fn unpack_tar<R: Read>(archive_type: ArchiveType, tar_stream: R, dst: &Path) -> Result<(), String> {
    let mut tar = tar::Archive::new(tar_stream);
    tar.unpack(dst)
        .map_err(|e| format!("Failed to unpack {archive_type:?}: {e}"))
}

#[time("debug", "installer::{}")]
fn unpack_archive<R: Read + Seek, T, F>(
    archive: ArchiveType,
    bytes_source: F,
    expected_hash: &str,
    dst: &Path,
) -> Result<Option<T>, String>
where
    F: FnOnce() -> Result<(R, T), String>,
{
    atomic_path(dst, Target::Directory, |work_dir| {
        let (bytes, result) = bytes_source()?;
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
        }?;
        Ok::<T, String>(result)
    })
}

#[cfg(not(target_family = "unix"))]
fn executable_permissions() -> Option<Permissions> {
    None
}

#[cfg(target_family = "unix")]
fn executable_permissions() -> Option<Permissions> {
    use std::os::unix::fs::PermissionsExt;
    Some(Permissions::from_mode(0o755))
}

#[time("debug", "installer::{}")]
fn unpack_blob<R: Read + Seek, T, F>(
    executable: bool,
    bytes_source: F,
    expected_hash: &str,
    dst: &Path,
) -> Result<Option<T>, String>
where
    F: FnOnce() -> Result<(R, T), String>,
{
    atomic_path(dst, Target::File, |blob_dst| {
        let (bytes, result) = bytes_source()?;
        let mut hashed_bytes = check_hash("blob", bytes, expected_hash, dst)?;
        let mut blob_out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(blob_dst)
            .map_err(|e| {
                format!(
                    "Failed to open blob destination {blob_dst} for writing: {e}",
                    blob_dst = blob_dst.display()
                )
            })?;
        if executable {
            if let Some(permissions) = executable_permissions() {
                blob_out.set_permissions(permissions).map_err(|e| {
                    format!(
                        "Failed to set executable premissions on {dst}: {e}",
                        dst = dst.display()
                    )
                })?;
            }
        }
        std::io::copy(&mut hashed_bytes, &mut blob_out)
            .map(|_| ())
            .map_err(|e| format!("Failed to unpack blob to {dst}: {e}", dst = dst.display()))?;
        Ok::<T, String>(result)
    })
}

fn unpack<R: Read + Seek, T, F>(
    file_type: FileType,
    executable: bool,
    bytes: F,
    expected_hash: &str,
    dst: &Path,
) -> Result<Option<T>, String>
where
    F: FnOnce() -> Result<(R, T), String>,
{
    match file_type {
        FileType::Archive(archive_type) => unpack_archive(archive_type, bytes, expected_hash, dst),
        FileType::Blob => unpack_blob(executable, bytes, expected_hash, dst),
        FileType::Directory => unpack_archive(ArchiveType::Zip, bytes, expected_hash, dst),
    }
}

#[derive(Debug)]
pub(crate) struct Installer<'a> {
    payload: &'a [u8],
}

impl<'a> Installer<'a> {
    pub(crate) fn new(payload: &'a [u8]) -> Self {
        Self { payload }
    }

    #[time("debug", "Installer::{}")]
    pub(crate) fn install(&self, files: &[FileEntry]) -> Result<(), String> {
        let mut scie_tote = vec![];
        let mut location = 0;
        for file_entry in files {
            let advance = match file_entry {
                FileEntry::Skip(size) => *size,
                FileEntry::Install((file, dst)) => {
                    if file.size == 0 {
                        scie_tote.push((file, file.file_type, dst.clone()));
                    } else {
                        let bytes = &self.payload[location..(location + file.size)];
                        unpack(
                            file.file_type,
                            file.executable.unwrap_or(false),
                            || Ok((Cursor::new(bytes), ())),
                            file.hash.as_str(),
                            dst,
                        )?;
                    }
                    file.size
                }
                FileEntry::LoadAndInstall((binding, file, dst)) => {
                    let buffer_source = || {
                        info!(
                            "Loading {file} via {exe:?}...",
                            file = file.name,
                            exe = binding.exe()
                        );
                        let mut buffer = tempfile::tempfile().map_err(|e| {
                            format!(
                                "Failed to establish a temporary file buffer for loading {file:?} via \
                                {binding:?}: {e}"
                            )
                        })?;
                        let mut child =
                            binding.spawn_stdout(vec![file.name.as_str()].as_slice())?;
                        let mut stdout = child.stdout.take().ok_or_else(|| {
                            format!(
                                "Failed to grab stdout attempting to load {file:?} via binding."
                            )
                        })?;
                        std::io::copy(&mut stdout, &mut buffer)
                            .map_err(|e| format!("Failed to load {file:?} via {binding:?}: {e}"))?;
                        buffer.rewind().map_err(|e| {
                            format!(
                                "Failed to re-wind temp file for reading {file:?} loaded by \
                                {binding:?}: {e}"
                            )
                        })?;
                        Ok((buffer, child))
                    };
                    if let Some(mut child) = unpack(
                        file.file_type,
                        file.executable.unwrap_or(false),
                        buffer_source,
                        file.hash.as_str(),
                        dst,
                    )? {
                        let exit_status = child.wait().map_err(|e| {
                            format!(
                                "Failed to await termination of {binding:?} when loading {file:?}: {e}"
                            )
                        })?;
                        if !exit_status.success() {
                            return Err(format!("Failed to load file {file:?}: {exit_status:?}"));
                        }
                    }
                    0
                }
                FileEntry::ScieTote((tote_file, entries)) => {
                    let mut scie_tote: Option<TempDir> = None;
                    let mut scie_tote_src = || {
                        if let Some(tempdir) = scie_tote.as_ref() {
                            return Ok::<_, String>(tempdir.path().join(&tote_file.name));
                        }
                        let scie_tote_tmpdir = TempDir::new().map_err(|e| {
                            format!(
                                "Failed to create a temporary directory to extract the scie-tote \
                                to: {e}"
                            )
                        })?;
                        let path = scie_tote_tmpdir.path().join(&tote_file.name);
                        let bytes = &self.payload[location..(location + tote_file.size)];
                        unpack(
                            tote_file.file_type,
                            tote_file.executable.unwrap_or(false),
                            || Ok((Cursor::new(bytes), ())),
                            tote_file.hash.as_str(),
                            &path,
                        )?;
                        scie_tote = Some(scie_tote_tmpdir);
                        Ok(path)
                    };

                    for (file, dst) in entries {
                        let file_src = || {
                            let scie_tote_path = scie_tote_src()?;
                            let src_path = scie_tote_path.join(&file.name);
                            let file = std::fs::File::open(&src_path).map_err(|e| {
                                format!(
                                    "Failed to open {file:?} at {src} from the unpacked scie-tote: {e}",
                                    src = src_path.display()
                                )
                            })?;
                            Ok((file, ()))
                        };
                        unpack(
                            file.file_type,
                            file.executable.unwrap_or(false),
                            file_src,
                            file.hash.as_str(),
                            dst,
                        )?;
                    }
                    tote_file.size
                }
            };
            location += advance;
        }

        Ok(())
    }
}

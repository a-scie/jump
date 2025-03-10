// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::collections::HashSet;
use std::fmt::Debug;
use std::fs::Permissions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::{cmp, env, io};

use jump::config::{FileType, Fmt};
use jump::{File, Jump, Lift, Source};
use proc_exit::{Code, Exit, ExitResult};
use zip::ZipArchive;

fn ensure_parent_dir(base: &Path, file: &File) -> Result<PathBuf, Exit> {
    let dst = base.join(&file.name);
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to establish parent directory for writing {dst} to: {e}",
                dst = dst.display()
            ))
        })?;
    }
    Ok(dst)
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

struct ChosenFiles {
    files: HashSet<String>,
}

impl ChosenFiles {
    fn new() -> Self {
        Self {
            files: HashSet::new(),
        }
    }

    fn add(&mut self, file: String) {
        self.files.insert(file);
    }

    fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    fn contains(&self, name: &str) -> bool {
        self.files.is_empty() || self.files.contains(name)
    }

    fn selected<'b, 'a: 'b>(&'a self, file: &'b File) -> Option<Option<&'b String>> {
        if self.files.is_empty() {
            Some(None)
        } else if self.files.contains(file.name.as_str()) {
            Some(Some(&file.name))
        } else if file
            .key
            .as_ref()
            .map(|key| self.files.contains(key))
            .unwrap_or(false)
        {
            Some(file.key.as_ref())
        } else {
            None
        }
    }
}

pub(crate) fn split(jump: Jump, mut lift: Lift, scie_path: PathBuf) -> ExitResult {
    let mut extra_args_seen = false;
    let mut dry_run = false;
    let mut chosen_files = ChosenFiles::new();
    let mut custom_base: Option<PathBuf> = None;
    for arg in env::args().skip(1) {
        match arg.as_str() {
            "-n" | "--dry-run" if !extra_args_seen => dry_run = true,
            "--" => extra_args_seen = true,
            _ if extra_args_seen => {
                chosen_files.add(arg);
            }
            path => {
                if let Some(custom) = custom_base {
                    return Err(Code::FAILURE.with_message(format!(
                        "Cannot split to {path} in addition to {custom}. Only one split \
                                dir is allowed.",
                        custom = custom.display()
                    )));
                } else {
                    custom_base = Some(PathBuf::from(path));
                }
            }
        }
    }

    let base = custom_base.unwrap_or_default();

    let mut scie = std::fs::File::open(&scie_path).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to open scie at {scie_path} for splitting: {e}",
            scie_path = scie_path.display()
        ))
    })?;

    let scie_jump_path = base
        .join("scie-jump")
        .with_extension(std::env::consts::EXE_EXTENSION);

    if dry_run {
        eprintln!("Would extract:");
        eprintln!();
        eprintln!("[destination file] [extracted size in bytes] [type] ([alt key for file])?");
        eprintln!("-------------------------------------------------------------------------");
        io::stderr().lock().flush().ok();
    } else {
        std::fs::create_dir_all(&base).map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to create target directory {base} for split: {e}",
                base = base.display()
            ))
        })?;
    }

    if chosen_files.contains("scie-jump") {
        if dry_run {
            println!(
                "{path} {size} executable",
                path = scie_jump_path.display(),
                size = jump.size
            );
        } else {
            let mut dst = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(scie_jump_path)
                .map_err(|e| {
                    Code::FAILURE
                        .with_message(format!("Failed to open scie-jump for extraction: {e}"))
                })?;
            mark_executable(&mut dst)?;
            let mut src = scie
                .try_clone()
                .map_err(|e| Code::FAILURE.with_message(format!("Failed to dup scie handle: {e}")))?
                .take(jump.size as u64);
            std::io::copy(&mut src, &mut dst).map_err(|e| {
                Code::FAILURE.with_message(format!("Failed to extract scie-jump: {e}"))
            })?;
        }
    }

    let mut have_scie_tote = false;
    let scie_tote_index = lift.files.len() - 1;
    let mut offset = jump.size;
    for (index, file) in lift.files.iter().enumerate() {
        if file.source != Source::Scie {
            continue;
        } else if file.size == 0 {
            have_scie_tote = true;
        } else if (file.file_type == FileType::Directory && chosen_files.selected(file).is_some())
            || (index == scie_tote_index && have_scie_tote)
        {
            let mut zip_archive = open_embedded_zip(&mut scie, offset as u64, file)?;
            if chosen_files.is_empty()
                || (file.file_type == FileType::Directory && chosen_files.selected(file).is_some())
            {
                if dry_run && file.file_type == FileType::Directory {
                    print_directory_entry(&base, &zip_archive, file);
                } else if dry_run {
                    for file in lift.files.iter() {
                        if file.size == 0 && file.source == Source::Scie {
                            print!(
                                "{path} {size} {file_type}",
                                path = base.join(&file.name).display(),
                                size = zip_archive
                                    .by_name(&file.name)
                                    .map_err(|e| Code::FAILURE.with_message(format!(
                                        "Expected to find {file} in the scie-tote: {e}",
                                        file = file.name
                                    )))?
                                    .size(),
                                file_type = file_type(file)
                            );
                            if let Some(key) = &file.key {
                                print!(" ({key})");
                            }
                            println!();
                        }
                    }
                } else {
                    let dst = if file.file_type == FileType::Directory {
                        ensure_parent_dir(&base, file)?
                    } else {
                        base.to_path_buf()
                    };
                    zip_archive.extract(&dst).map_err(|e| {
                        Code::FAILURE.with_message(format!(
                            "Failed to extract scie-tote to {base}: {e}",
                            base = base.display()
                        ))
                    })?;
                }
            } else {
                for file in lift.files.iter() {
                    if let Some(maybe_selected_file) = chosen_files.selected(file) {
                        let selected_file =
                            maybe_selected_file.expect("Split files were selected.");
                        let mut src = zip_archive.by_name(file.name.as_str()).map_err(|e| {
                            Code::FAILURE.with_message(format!(
                                "The selected file {selected_file} could not be found in this \
                                scie: {e}"
                            ))
                        })?;
                        if dry_run {
                            print!(
                                "{path} {size} {file_type}",
                                path = base.join(&file.name).display(),
                                size = src.size(),
                                file_type = file_type(file)
                            );
                            if let Some(key) = &file.key {
                                print!(" ({key})");
                            }
                            println!();
                        } else {
                            extract_to(&base, file, &mut src)?;
                        }
                    }
                }
            }
        } else if chosen_files.selected(file).is_some() {
            if dry_run && file.file_type == FileType::Directory {
                let zip_archive = open_embedded_zip(&mut scie, offset as u64, file)?;
                print_directory_entry(&base, &zip_archive, file);
            } else if dry_run {
                print!(
                    "{path} {size} {file_type}",
                    path = base.join(&file.name).display(),
                    size = file.size,
                    file_type = file_type(file)
                );
                if let Some(key) = &file.key {
                    print!(" ({key})");
                }
                println!();
            } else {
                let file_size = file.size as u64;
                let mut reader = scie
                    .try_clone()
                    .map_err(|e| {
                        Code::FAILURE.with_message(format!("Failed to dup scie handle: {e}"))
                    })?
                    .take(file_size);
                extract_to(&base, file, &mut reader)?;
            }
        }
        offset += file.size;
    }

    if chosen_files.contains("lift.json") {
        if have_scie_tote {
            let scie_tote = lift.files.remove(scie_tote_index);
            let start = scie.seek(SeekFrom::Start(jump.size as u64)).map_err(|e| {
                Code::FAILURE.with_message(format!("Failed to seek to scie-tote: {e}"))
            })?;
            let mut zip_archive = open_embedded_zip(&mut scie, start, &scie_tote)?;
            for file in lift.files.iter_mut() {
                if file.size == 0 && file.source == Source::Scie {
                    file.size = zip_archive
                        .by_name(&file.name)
                        .map_err(|e| {
                            Code::FAILURE.with_message(format!(
                                "Expected to find {file} in the scie-tote: {e}",
                                file = file.name
                            ))
                        })?
                        .size() as usize;
                }
            }
        }

        if dry_run {
            let mut writer = io::Cursor::new(vec![]);
            serialize_manifest(jump, lift, &mut writer)?;
            println!(
                "{file} {size} blob",
                file = base.join("lift.json").display(),
                size = writer.position()
            );
        } else {
            serialize_manifest(
                jump,
                lift,
                &mut std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(base.join("lift.json"))
                    .map_err(|e| {
                        Code::FAILURE
                            .with_message(format!("Failed to open lift manifest for writing: {e}"))
                    })?,
            )?;
        };
    }

    Code::SUCCESS.ok()
}

fn file_type(file: &File) -> &str {
    match file.file_type {
        FileType::Archive(_) => "archive",
        FileType::Blob if file.executable.unwrap_or_default() => "executable",
        FileType::Blob => "blob",
        FileType::Directory => "directory",
    }
}

fn print_directory_entry<P: AsRef<Path>, R>(base: P, zip_archive: &ZipArchive<R>, file: &File) {
    // N.B.: If file is a Directory then we store it in the scie as a zip and file.size represents
    // the compressed size of the zipped up directory. We want the uncompressed size of the zipped
    // up directory here as the proper fair warning on a split, which will extract the directory
    // loose.
    print!(
        "{path} {size} {file_type}",
        path = base.as_ref().join(&file.name).display(),
        size = zip_archive.decompressed_size().unwrap_or(file.size as u128),
        file_type = file_type(file)
    );
    if let Some(key) = &file.key {
        print!(" ({key})");
    }
    println!();
}

fn extract_to<P: AsRef<Path>, R: Read>(base: P, file: &File, reader: &mut R) -> Result<u64, Exit> {
    let dst = ensure_parent_dir(base.as_ref(), file)?;
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&dst)
        .map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to open {dst} for extraction: {e}",
                dst = dst.display()
            ))
        })?;
    if file.executable.unwrap_or_default() {
        mark_executable(&mut out)?;
    }
    std::io::copy(reader, &mut out).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to extract {file} to {dst}: {e}",
            file = file.name,
            dst = dst.display()
        ))
    })
}

fn mark_executable(file: &mut std::fs::File) -> Result<(), Exit> {
    if let Some(permissions) = executable_permissions() {
        file.set_permissions(permissions).map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to open file metadata for the scie-jump: {e}"
            ))
        })
    } else {
        Ok(())
    }
}

struct EmbeddedZipReader<'a, S> {
    scie: &'a mut S,
    start: u64,
    length: u64,
    offset: u64,
}

impl<'a, S> EmbeddedZipReader<'a, S> {
    pub fn new(scie: &'a mut S, start: u64, length: u64) -> Self {
        Self {
            scie,
            start,
            length,
            offset: 0,
        }
    }
}

impl<R: Read + Debug> Read for EmbeddedZipReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let written = self.scie.take(self.length - self.offset).read(buf)?;
        self.offset += written as u64;
        Ok(written)
    }
}

impl<S: Seek + Debug> Seek for EmbeddedZipReader<'_, S> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let offset = match pos {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::End(offset) => self.length.checked_add_signed(offset),
            SeekFrom::Current(offset) => self.offset.checked_add_signed(offset),
        };
        match offset {
            None => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid seek to a negative or overflowing position",
            )),
            Some(offset) => {
                let clamped_offset = cmp::min(self.length, offset);
                let new_inner_offset = self
                    .scie
                    .seek(SeekFrom::Start(self.start + clamped_offset))?;
                self.offset = new_inner_offset - self.start;
                Ok(self.offset)
            }
        }
    }
}

fn open_embedded_zip<'a, S: Seek + Read + Debug>(
    scie: &'a mut S,
    start: u64,
    file: &File,
) -> Result<ZipArchive<EmbeddedZipReader<'a, S>>, Exit> {
    let length = file.size as u64;
    ZipArchive::new(EmbeddedZipReader::<'a, S>::new(scie, start, length)).map_err(|e| {
        Code::FAILURE.with_message(format!("Failed to open {file} zip: {e}", file = file.name))
    })
}

fn serialize_manifest<W: Write>(jump: Jump, lift: Lift, writer: &mut W) -> Result<(), Exit> {
    jump::config(jump, lift)
        .serialize(
            writer,
            Fmt::new()
                .pretty(true)
                .leading_newline(false)
                .trailing_newline(true),
        )
        .map_err(|e| Code::FAILURE.with_message(format!("Failed to serialize lift manifest: {e}")))
}

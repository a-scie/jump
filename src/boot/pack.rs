// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::env;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

use jump::config::{ArchiveType, FileType, Fmt};
use jump::{
    BOOT_PACK_HELP, File, Jump, Lift, Source, check_is_zip, create_options, fingerprint, hash_jump,
    load_jump, load_lift,
};
use logging_timer::time;
use proc_exit::{Code, ExitResult};
use semver::Version;
use zip::{CompressionMethod, ZipWriter};

#[time("debug", "pack::{}")]
fn load_manifest(path: &Path, jump: &Jump) -> Result<(Lift, PathBuf), String> {
    let manifest_path = if path.is_dir() {
        path.join("lift.json")
    } else {
        path.to_path_buf()
    };
    if !manifest_path.is_file() {
        return Err(format!(
            "The given path does not contain a lift manifest: {path}",
            path = path.display()
        ));
    }
    let (maybe_jump, lift) = load_lift(&manifest_path, true)?;
    if let Some(ref configured_jump) = maybe_jump
        && jump != configured_jump
    {
        return Err(format!(
            "The lift manifest {manifest} specifies a scie jump binary of \
                    {configured_jump:?} that does not match the current of {jump:?}.",
            manifest = manifest_path.display()
        ));
    }
    Ok((lift, manifest_path))
}

#[cfg(windows)]
fn finalize_executable(path: &Path) -> Result<PathBuf, String> {
    use std::ffi::OsStr;
    if let Some(env::consts::EXE_EXTENSION) = path.extension().and_then(OsStr::to_str) {
        Ok(path.to_path_buf())
    } else {
        let exe = path.with_file_name(format!(
            "{file_name}.{ext}",
            file_name = path
                .file_name()
                .and_then(OsStr::to_str)
                .ok_or_else(|| format!(
                    "Failed to determine the file name of {path}",
                    path = path.display()
                ))?,
            ext = env::consts::EXE_EXTENSION
        ));
        std::fs::rename(path, &exe).map_err(|e| {
            format!(
                "Failed to rename executable from {path} to {exe}: {e}",
                path = path.display(),
                exe = exe.display()
            )
        })?;
        Ok(exe)
    }
}

#[cfg(unix)]
fn finalize_executable(path: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)
        .map_err(|e| {
            format!(
                "Failed to access permissions metadata for {binary}: {e}",
                binary = path.display()
            )
        })?
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).map_err(|e| {
        format!(
            "Failed to mark {binary} as executable: {e}",
            binary = path.display()
        )
    })?;
    Ok(path.to_path_buf())
}

struct ScieTote {
    zip_file: std::fs::File,
    zip_writer: ZipWriter<std::fs::File>,
}

impl ScieTote {
    fn new() -> Result<Self, String> {
        let zip_file = tempfile::tempfile().map_err(|e| {
            format!("Failed to create a temporary file to built the scie-tote with: {e}")
        })?;
        let zip_writer = ZipWriter::new(
            zip_file
                .try_clone()
                .map_err(|e| format!("Failed to dup temporary file fd: {e}"))?,
        );
        Ok(Self {
            zip_file,
            zip_writer,
        })
    }
}

#[time("debug", "pack::{}")]
fn pack(
    mut lift: Lift,
    manifest_path: &Path,
    jump: &Jump,
    scie_jump_path: &Path,
    single_line: bool,
) -> Result<PathBuf, String> {
    let binary_path = env::current_dir()
        .map(|cwd| cwd.join(&lift.name))
        .map_err(|e| format!("Failed to determine the output directory for scies: {e}"))?;
    let mut binary = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&binary_path)
        .map_err(|e| {
            format!(
                "Failed to open binary {path} for writing {lift:?}: {e}",
                path = binary_path.display(),
            )
        })?;
    let mut scie_jump = std::fs::File::open(scie_jump_path)
        .map_err(|e| {
            format!(
                "Failed to open scie-jump binary {path} for writing to the tip of {binary}: {e}",
                path = scie_jump_path.display(),
                binary = binary_path.display()
            )
        })?
        .take(u64::from(jump.size));
    std::io::copy(&mut scie_jump, &mut binary).map_err(|e| {
        format!(
            "Failed to write first {scie_jump_size} bytes of the scie-jump binary {path} to \
            {binary}: {e}",
            scie_jump_size = jump.size,
            path = scie_jump_path.display(),
            binary = binary_path.display()
        )
    })?;
    let resolve_base = manifest_path.parent().unwrap_or_else(|| Path::new(""));
    let mut scie_tote: Option<ScieTote> = None;
    if let Some(last_file) = lift.files.last() {
        let mut path = resolve_base.join(&last_file.name);
        if FileType::Directory == last_file.file_type {
            path = path.with_extension("zip");
        }
        if check_is_zip(&path).is_err() {
            scie_tote = Some(ScieTote::new()?)
        }
    }
    for file in lift.files.iter_mut() {
        if Source::Scie != file.source {
            continue;
        }
        let mut path = resolve_base.join(&file.name);
        if FileType::Directory == file.file_type {
            path = path.with_extension("zip");
        }
        let mut blob = std::fs::File::open(&path).map_err(|e| {
            format!(
                "Failed to open {src} / {file:?} for writing to {binary}: {e}",
                src = path.display(),
                binary = binary_path.display()
            )
        })?;
        if let Some(tote) = scie_tote.as_mut() {
            let metadata = blob.metadata().map_err(|e| {
                format!(
                    "Failed to read metadata for {path}: {e}",
                    path = path.display()
                )
            })?;
            let options = create_options(&metadata)?.compression_method(CompressionMethod::Stored);
            tote.zip_writer
                .start_file(&file.name, options)
                .map_err(|e| {
                    format!(
                        "Failed to start a scie-tote file entry for {path}: {e}",
                        path = path.display()
                    )
                })?;
            std::io::copy(&mut blob, &mut tote.zip_writer).map_err(|e| {
                format!(
                    "Failed to append {src} / {file:?} to {binary}: {e}",
                    src = path.display(),
                    binary = binary_path.display()
                )
            })?;
            file.size = 0;
        } else {
            std::io::copy(&mut blob, &mut binary).map_err(|e| {
                format!(
                    "Failed to append {src} / {file:?} to {binary}: {e}",
                    src = path.display(),
                    binary = binary_path.display()
                )
            })?;
        };
    }
    if let Some(mut tote) = scie_tote {
        tote.zip_writer
            .finish()
            .map_err(|e| format!("Failed to finalize the scie-tote zip: {e}"))?;

        tote.zip_file.rewind().map_err(|e| {
            format!(
                "Failed to re-wind the scie-tote file to make a second pass calculation of \
                    its hash: {e}"
            )
        })?;
        let (size, hash) = fingerprint::digest_reader(&tote.zip_file)?;
        let tote_file = File {
            name: "scie-tote".to_string(),
            key: None,
            size,
            hash,
            file_type: FileType::Archive(ArchiveType::Zip),
            executable: None,
            eager_extract: false,
            source: Source::Scie,
        };

        tote.zip_file.rewind().map_err(|e| {
            format!("Failed to re-wind the sci-tote file to append its contents to the scie: {e}")
        })?;
        std::io::copy(&mut tote.zip_file, &mut binary).map_err(|e| {
            format!(
                "Failed to append {tote_file:?} to {binary}: {e}",
                binary = binary_path.display()
            )
        })?;
        lift.files.push(tote_file);
    }
    let jump = if let Some(hash) = hash_jump(jump, scie_jump_path)? {
        jump.with_hash(hash)
    } else {
        jump.clone()
    };
    let config = jump::config(jump, lift);
    // We configure the lift manifest format to allow for easiest inspection via standard tools.
    // In the single line case in particular, this configuration allows for inspection via
    // `tail -1 scie` or `tail -1 scie | jq .` on systems with these common tools.
    let fmt = Fmt::new()
        .pretty(!single_line)
        .leading_newline(true)
        .trailing_newline(true);
    config.serialize(&mut binary, fmt).map_err(|e| {
        format!(
            "Failed to serialize the lift manifest to {binary}: {e}",
            binary = binary_path.display()
        )
    })?;
    finalize_executable(&binary_path)
}

fn print_help() -> ExitResult {
    println!("{BOOT_PACK_HELP}");
    Code::SUCCESS.ok()
}

fn print_usage(message: &str) -> ExitResult {
    println!(
        "Usage: {argv0}",
        argv0 = env::args().next().unwrap_or_else(|| env::current_exe()
            .ok()
            .map(|pb| pb.display().to_string())
            .unwrap_or_else(|| "scie-jump".to_string()))
    );
    println!("{BOOT_PACK_HELP}");
    println!();
    Err(Code::FAILURE.with_message(message.to_string()))
}

pub(crate) fn set(
    mut jump: Jump,
    mut scie_jump_path: PathBuf,
    current_scie_jump_version: &Version,
    argv_skip: usize,
) -> ExitResult {
    let mut lifts = vec![];
    let mut single_line = true;
    let mut args = env::args().skip(argv_skip);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return print_help(),
            "-V" | "--version" => {
                println!("{version}", version = jump.version);
                return Code::SUCCESS.ok();
            }
            "-1" | "--single-lift-line" => single_line = true,
            "--no-single-lift-line" => single_line = false,
            "-sj" | "--jump" | "--scie-jump" => {
                scie_jump_path = std::path::absolute(args.next().ok_or_else(|| {
                    Code::FAILURE.with_message(format!(
                        "The {arg} flag requires an argument pointing to an alternate \
                        scie-jump binary to pack in the scie tip."
                    ))
                })?)
                .map_err(|e| {
                    Code::FAILURE
                        .with_message(format!("Failed to convert {arg} to an absolute path: {e}"))
                })?;
                jump = load_jump(&scie_jump_path, current_scie_jump_version)
                    .map_err(|e| {
                        Code::FAILURE.with_message(format!(
                            "Failed to determine the version and size or the alternate scie-jump \
                            at {path}: {e}",
                            path = scie_jump_path.display()
                        ))
                    })?
                    .ok_or_else(|| {
                        Code::FAILURE.with_message(format!(
                            "The file at {path} does not appear to be a scie-jump binary.",
                            path = scie_jump_path.display()
                        ))
                    })?;
            }
            _ => {
                let (lift, path) = load_manifest(Path::new(arg.as_str()), &jump)
                    .map_err(|e| Code::FAILURE.with_message(e))?;
                lifts.push((lift, path));
            }
        }
    }
    if lifts.is_empty()
        && let Ok(cwd) = env::current_dir()
    {
        if let Ok((lift, path)) =
            load_manifest(&cwd, &jump).map_err(|e| Code::FAILURE.with_message(e))
        {
            lifts.push((lift, path));
        } else {
            return print_usage(
                "\
Found no lift manifests to process. Either include paths to lift manifest
files as arguments or else paths to directories containing lift manifest files
named `lift.json`.",
            );
        }
    }

    let results = lifts
        .into_iter()
        .map(|(lift, manifest)| {
            pack(lift, &manifest, &jump, &scie_jump_path, single_line)
                .map(|binary| (manifest, binary))
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Code::FAILURE.with_message(e))?;
    for (manifest, binary) in results {
        println!(
            "{manifest}: {binary}",
            manifest = manifest.display(),
            binary = binary.display()
        );
    }
    Code::SUCCESS.ok()
}

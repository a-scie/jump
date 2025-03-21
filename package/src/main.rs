// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::env;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Command;

use byteorder::{LittleEndian, WriteBytesExt};
use clap::Parser;
use jump::{ARCH, EOF_MAGIC};
use proc_exit::{Code, Exit, ExitResult};
use sha2::{Digest, Sha256};

const BINARY: &str = "scie-jump";

#[cfg(windows)]
const PATHSEP: &str = ";";

#[cfg(not(windows))]
const PATHSEP: &str = ":";

fn add_magic(path: &Path) -> ExitResult {
    let mut binary = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to open {BINARY} at {path} for appending the trailer magic bytes: {e}",
                path = path.display()
            ))
        })?;
    let metadata = binary.metadata().map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to load metadata for {path} to determine its size: {e}",
            path = path.display()
        ))
    })?;
    let size = u32::try_from(metadata.len()).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "The {BINARY} at {path} is larger than expected: {e}",
            path = path.display()
        ))
    })?;
    binary
        .write_u32::<LittleEndian>(size + 8)
        .and_then(|()| binary.write_u32::<LittleEndian>(EOF_MAGIC))
        .map_err(|e| {
            Code::FAILURE.with_message(format!("Problem writing {BINARY} trailer magic bytes: {e}"))
        })
}

fn execute(command: &mut Command) -> ExitResult {
    let mut child = command
        .spawn()
        .map_err(|e| Code::FAILURE.with_message(format!("{e}")))?;
    let exit_status = child.wait().map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to gather exit status of command: {command:?}: {e}"
        ))
    })?;
    if !exit_status.success() {
        return Err(Code::FAILURE.with_message(format!(
            "Command {command:?} failed with exit code: {code:?}",
            code = exit_status.code()
        )));
    }
    Ok(())
}

fn path_as_str(path: &Path) -> Result<&str, Exit> {
    path.to_str().ok_or_else(|| {
        Code::FAILURE.with_message(format!("Failed to convert path {path:?} into a UTF-8 str."))
    })
}

#[derive(Clone)]
struct SpecifiedPath(PathBuf);

impl SpecifiedPath {
    fn new(path: &str) -> Self {
        Self::from(path.to_string())
    }
}

impl From<String> for SpecifiedPath {
    fn from(path: String) -> Self {
        SpecifiedPath(PathBuf::from(path))
    }
}

impl Deref for SpecifiedPath {
    type Target = PathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<Path> for SpecifiedPath {
    fn as_ref(&self) -> &Path {
        self.0.as_path()
    }
}

impl Display for SpecifiedPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

#[derive(Parser)]
#[command(about, version)]
struct Args {
    #[arg(long, help = "Override the default --target for this platform.")]
    target: Option<String>,
    #[arg(
        help = "The destination directory for the scie-jump binary and checksum file.",
        default_value_t = SpecifiedPath::new("dist")
    )]
    dest_dir: SpecifiedPath,
}

fn main() -> ExitResult {
    let args = Args::parse();
    let dest_dir = args.dest_dir;
    if dest_dir.is_file() {
        return Err(Code::FAILURE.with_message(format!(
            "The specified dest_dir of {} is a file. Not overwriting",
            dest_dir.display()
        )));
    }

    let cargo = env!("CARGO");
    let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");

    // N.B.: OUT_DIR and TARGET are not normally available under compilation, but our custom build
    // script forwards them.
    let out_dir = env!("OUT_DIR");
    let target = args.target.unwrap_or_else(|| env!("TARGET").to_string());

    // Just in case this target is not already installed.
    if let Ok(rustup) = which::which("rustup") {
        execute(Command::new(rustup).args(["target", "add", &target]))?;
    }

    let workspace_root = PathBuf::from(cargo_manifest_dir).join("..");
    let output_root = PathBuf::from(out_dir).join("dist");
    let output_bin_dir = output_root.join("bin");
    execute(
        Command::new(cargo)
            .args([
                "install",
                "--path",
                path_as_str(&workspace_root)?,
                "--target",
                &target,
                "--root",
                path_as_str(&output_root)?,
            ])
            // N.B.: This just suppresses a warning about adding this bin dir to your PATH.
            .env(
                "PATH",
                [output_bin_dir.to_str().unwrap(), env!("PATH")].join(PATHSEP),
            ),
    )?;

    let src = output_bin_dir
        .join(BINARY)
        .with_extension(env::consts::EXE_EXTENSION);
    add_magic(&src)?;
    let mut reader = std::fs::File::open(&src).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to open {src} for hashing: {e}",
            src = src.display()
        ))
    })?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut reader, &mut hasher)
        .map_err(|e| Code::FAILURE.with_message(format!("Failed to digest stream: {e}")))?;
    let digest = hasher.finalize();

    let file_name = format!(
        "{BINARY}-{os}-{arch}{exe}",
        os = env::consts::OS,
        arch = ARCH,
        exe = env::consts::EXE_SUFFIX
    );
    let dst = dest_dir.join(&file_name);

    std::fs::create_dir_all(&dest_dir).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to create dest_dir {dest_dir}: {e}",
            dest_dir = dest_dir.display()
        ))
    })?;
    std::fs::copy(&src, &dst).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to copy {src} to {dst}: {e}",
            src = src.display(),
            dst = dst.display()
        ))
    })?;

    let fingerprint_file = dst.with_file_name(format!("{file_name}.sha256"));
    std::fs::write(&fingerprint_file, format!("{digest:x} *{file_name}\n")).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to write fingerprint file {fingerprint_file}: {e}",
            fingerprint_file = fingerprint_file.display()
        ))
    })?;

    eprintln!(
        "Wrote the {BINARY} (target: {target}) to {dst}",
        dst = dst.display()
    );
    Code::SUCCESS.ok()
}

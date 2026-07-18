// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::borrow::Cow;
use std::env;
use std::fmt::{Display, Formatter};
use std::io::Write;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Command;

use byteorder::{LittleEndian, WriteBytesExt};
use cargo_toml::{Inheritable, Manifest};
use clap::ArgAction::Append;
use clap::Parser;
use jump::{ARCH, EOF_MAGIC, hash_reader};
use proc_exit::{Code, Exit, ExitResult};
use sha2::{Digest, Sha256};

const BINARY: &str = "scie-jump";

#[cfg(windows)]
const PATHSEP: &str = ";";

#[cfg(not(windows))]
const PATHSEP: &str = ":";

#[cfg(windows)]
fn expected_binaries(output_bin_dir: PathBuf) -> Vec<PathBuf> {
    vec![
        output_bin_dir.join(BINARY).with_extension("exe"),
        output_bin_dir
            .join(format!("{binary}w", binary = crate::BINARY))
            .with_extension("exe"),
    ]
}

#[cfg(unix)]
fn expected_binaries(output_bin_dir: PathBuf) -> Vec<PathBuf> {
    vec![output_bin_dir.join(BINARY)]
}

fn add_magic(path: &Path, version: &str) -> ExitResult {
    let mut binary = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to open {BINARY} at {path} for appending the trailer magic bytes: {e}",
                path = path.display()
            ))
        })?;

    let version_bytes = version.as_bytes();
    let version_bytes_len = version_bytes.len() as u8;
    binary
        .write_all(version_bytes)
        .map_err(|e| Code::FAILURE.with_message(format!("Failed to append the version: {e}")))?;
    binary.write_u8(version_bytes_len).map_err(|e| {
        Code::FAILURE.with_message(format!("Failed to append the version length: {e}"))
    })?;
    binary.flush().map_err(|e| {
        Code::FAILURE.with_message(format!("Failed to flush trailer version information: {e}"))
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
    let mut child = command.spawn().map_err(|e| {
        Code::FAILURE.with_message(format!("Failed to spawn command {command:?}: {e}"))
    })?;
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
    #[arg(long, help = "Provide an alternate suffix for the scie-jump binary.")]
    suffix: Option<String>,
    #[arg(
        help = "The destination directory for the scie-jump binary and checksum file.",
        default_value_t = SpecifiedPath::new("dist")
    )]
    dest_dir: SpecifiedPath,
    #[arg(
        long = "scie-jump",
        action = Append,
        help = "Add magic to this scie-jump binary instead of building one."
    )]
    scie_jumps: Vec<PathBuf>,
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
    if !args.scie_jumps.is_empty() && args.target.is_some() {
        return Err(Code::FAILURE.with_message(
            "Cannot specify a scie jump to add magic to in combination with a --target.",
        ));
    }

    let cargo = env!("CARGO");
    let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = PathBuf::from(cargo_manifest_dir).join("..");

    let (srcs, target) = if args.scie_jumps.is_empty() {
        // N.B.: OUT_DIR and TARGET are not normally available under compilation, but our custom build
        // script forwards them.
        let out_dir = env!("OUT_DIR");
        let target = args.target.unwrap_or_else(|| env!("TARGET").to_string());

        // Just in case this target is not already installed.
        if let Ok(rustup) = which::which("rustup") {
            execute(Command::new(rustup).args(["target", "add", &target]))?;
        }
        let output_root = PathBuf::from(out_dir).join("dist");
        let output_bin_dir = output_root.join("bin");
        let mut command = Command::new(cargo);
        command.args([
            "install",
            "--path",
            path_as_str(&workspace_root)?,
            "--target",
            &target,
            "--root",
            path_as_str(&output_root)?,
        ]);
        #[cfg(windows)]
        command.args(["--features", "windows"]);
        execute(
            command
                // N.B.: This just suppresses a warning about adding this bin dir to your PATH.
                .env(
                    "PATH",
                    [output_bin_dir.to_str().unwrap(), env!("PATH")].join(PATHSEP),
                )
                // N.B.: This avoids a spurious warning of this sort:
                // warning: default toolchain implicitly overridden with `1.96.0-x86_64-unknown-linux-gnu` by rustup toolchain file
                //   |
                //   = help: use `cargo +stable install` if you meant to use the stable toolchain
                //   = note: rustup selects the toolchain based on the parent environment and not the environment of the package being installed
                .env_remove("RUSTUP_TOOLCHAIN_SOURCE"),
        )?;
        let srcs = expected_binaries(output_bin_dir);
        (srcs, Some(target))
    } else {
        (args.scie_jumps, None)
    };

    let scie_jump_manifest_path = workspace_root.join("Cargo.toml");
    let scie_jump_manifest = Manifest::from_path(&scie_jump_manifest_path).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to read manifest at {path}: {e}",
            path = scie_jump_manifest_path.display()
        ))
    })?;
    let version = if let Some(package) = scie_jump_manifest.package
        && let Inheritable::Set(version) = package.version
    {
        version
    } else {
        return Err(Code::FAILURE.with_message(format!(
            "The scie-jump manifest at {manifest} is missing a package version.",
            manifest = scie_jump_manifest_path.display()
        )));
    };

    for src in srcs {
        add_magic(&src, &version.to_string())?;
        let reader = std::fs::File::open(&src).map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to open {src} for hashing: {e}",
                src = src.display()
            ))
        })?;
        let mut hasher = Sha256::new();
        hash_reader(reader, &mut hasher)
            .map_err(|e| Code::FAILURE.with_message(format!("Failed to digest stream: {e}")))?;
        let digest = hasher.finalize();

        let exe_suffix = if let Some(file_name) = src.file_name()
            && file_name.as_encoded_bytes().ends_with(b"w.exe")
        {
            "w.exe"
        } else if let Some(file_name) = src.file_name()
            && file_name.as_encoded_bytes().ends_with(b".exe")
        {
            ".exe"
        } else {
            env::consts::EXE_SUFFIX
        };
        let file_name = format!(
            "{BINARY}-{suffix}",
            suffix = args
                .suffix
                .as_deref()
                .map(Cow::Borrowed)
                .unwrap_or_else(|| Cow::Owned(format!(
                    "{os}-{arch}{exe_suffix}",
                    os = env::consts::OS,
                    arch = ARCH,
                )))
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
        std::fs::write(
            &fingerprint_file,
            format!("{digest} *{file_name}\n", digest = hex::encode(digest)),
        )
        .map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to write fingerprint file {fingerprint_file}: {e}",
                fingerprint_file = fingerprint_file.display()
            ))
        })?;

        if let Some(target) = target.as_deref() {
            eprintln!(
                "Wrote the {BINARY} (target: {target}) to {dst}",
                dst = dst.display()
            );
        } else {
            eprintln!(
                "Wrote the {src} with magic appended to {dst}",
                src = src.display(),
                dst = dst.display()
            );
        }
    }
    Code::SUCCESS.ok()
}

use std::env;
use std::fs::Permissions;
use std::io::Read;
use std::path::{Path, PathBuf};

use jump::config::{Config, FileType, Fmt, Scie};
use jump::{File, Jump, Lift};
use log::debug;
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

pub(crate) fn split(jump: Jump, mut lift: Lift, scie_path: PathBuf) -> ExitResult {
    let base = if let Some(base) = env::args().nth(1) {
        PathBuf::from(base)
    } else {
        env::current_dir().map_err(|e| {
            Code::FAILURE.with_message(format!(
                "No target directory for the split was passed and the current directory could not \
                be determined: {e}"
            ))
        })?
    };
    std::fs::create_dir_all(&base).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to create target directory {base} for split: {e}",
            base = base.display()
        ))
    })?;

    let scie = std::fs::File::open(&scie_path).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to open scie at {scie_path} for splitting: {e}",
            scie_path = scie_path.display()
        ))
    })?;

    let mut dst = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(
            base.join("scie-jump")
                .with_extension(std::env::consts::EXE_EXTENSION),
        )
        .map_err(|e| {
            Code::FAILURE.with_message(format!("Failed to open scie-jump for extraction: {e}"))
        })?;
    if let Some(permissions) = executable_permissions() {
        dst.set_permissions(permissions).map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to open file metadata for the scie-jump: {e}"
            ))
        })?;
    }
    let mut src = scie
        .try_clone()
        .map_err(|e| Code::FAILURE.with_message(format!("Failed to dup scie handle: {e}")))?
        .take(jump.size as u64);
    std::io::copy(&mut src, &mut dst)
        .map_err(|e| Code::FAILURE.with_message(format!("Failed to extract scie-jump: {e}")))?;

    let mut scie_tote = vec![];
    let scie_tote_index = lift.files.len() - 1;
    for (index, file) in lift.files.iter().enumerate() {
        if file.size == 0 {
            scie_tote.push(file);
        } else if file.file_type == FileType::Directory
            || (index == scie_tote_index && !scie_tote.is_empty())
        {
            let mut src = scie
                .try_clone()
                .map_err(|e| Code::FAILURE.with_message(format!("Failed to dup scie handle: {e}")))?
                .take(file.size as u64);
            let mut zip_file = tempfile::tempfile().map_err(|e| {
                Code::FAILURE.with_message(format!(
                    "Failed to create a temporary file to extract {file} to: {e}",
                    file = file.name
                ))
            })?;
            std::io::copy(&mut src, &mut zip_file).map_err(|e| {
                Code::FAILURE.with_message(format!(
                    "Failed to extract {file} zip to a temp file: {e}",
                    file = file.name
                ))
            })?;
            let mut zip_archive = ZipArchive::new(zip_file).map_err(|e| {
                Code::FAILURE
                    .with_message(format!("Failed to open {file} zip: {e}", file = file.name))
            })?;
            let dst = if file.file_type == FileType::Directory {
                ensure_parent_dir(&base, file)?
            } else {
                base.to_path_buf()
            };
            debug!("Extracting {file:?} to {dst}...", dst = dst.display());
            zip_archive.extract(&dst).map_err(|e| {
                Code::FAILURE.with_message(format!(
                    "Failed to extract scie-tote to {base}: {e}",
                    base = base.display()
                ))
            })?;
        } else {
            let dst = ensure_parent_dir(&base, file)?;
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
            let file_size = file.size as u64;
            let mut src = scie
                .try_clone()
                .map_err(|e| Code::FAILURE.with_message(format!("Failed to dup scie handle: {e}")))?
                .take(file_size);
            std::io::copy(&mut src, &mut out).map_err(|e| {
                Code::FAILURE.with_message(format!(
                    "Failed to extract {file:?} to {dst}: {e}",
                    dst = dst.display()
                ))
            })?;
        }
    }

    if !scie_tote.is_empty() {
        lift.files.remove(lift.files.len() - 1);
        for mut file in lift.files.iter_mut() {
            let metadata = base.join(&file.name).metadata().map_err(|e| {
                Code::FAILURE.with_message(format!(
                    "Failed to determine size of {file}: {e}",
                    file = file.name
                ))
            })?;
            file.size = metadata.len() as usize;
        }
    }

    let manifest = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(base.join("lift.json"))
        .map_err(|e| {
            Code::FAILURE.with_message(format!("Failed to open lift manifest for writing: {e}"))
        })?;
    Config {
        scie: Scie {
            jump: Some(jump),
            lift: lift.into(),
        },
    }
    .serialize(
        manifest,
        Fmt::new()
            .pretty(true)
            .leading_newline(false)
            .trailing_newline(true),
    )
    .map_err(|e| Code::FAILURE.with_message(format!("Failed to serialize lift manifest: {e}")))?;

    Code::SUCCESS.ok()
}

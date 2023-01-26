// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::env;
use std::path::{Path, PathBuf};

use jump::config::Fmt;
use jump::{Jump, Lift, ScieBoot, SelectBoot};
use log::warn;
use proc_exit::{Code, ExitResult};

mod pack;
mod split;
pub(crate) use pack::set as pack;
pub(crate) use split::split;

pub(crate) fn help(message: String, exit_code: i32) -> ExitResult {
    let code = Code::from(exit_code);
    if code.is_err() {
        Err(code.with_message(message))
    } else {
        print!("{message}");
        code.ok()
    }
}

pub(crate) fn inspect(jump: Jump, lift: Lift) -> ExitResult {
    let config = jump::config(jump, lift);
    let fmt = Fmt::new().pretty(true).trailing_newline(true);
    config
        .serialize(std::io::stdout(), fmt)
        .map_err(|e| Code::FAILURE.with_message(format!("Failed to serialize lift manifest: {e}")))
}

pub(crate) fn select(select_boot: SelectBoot) -> ExitResult {
    let header = if select_boot.boots.iter().any(|boot| boot.default) {
        ""
    } else {
        "This Scie binary has no default boot command.\n"
    };
    Err(Code::FAILURE.with_message(format!(
        "{description}\n\
            Please select from the following boot commands:\n\
            \n\
            {boot_commands}\n\
            \n\
            You can select a boot command by passing it as the 1st argument or else by \
            setting the SCIE_BOOT environment variable.\n\
            {error_message}",
        description = select_boot
            .description
            .map(|message| format!("{header}{message}\n"))
            .unwrap_or_default(),
        boot_commands = select_boot
            .boots
            .into_iter()
            .map(|boot| if let Some(description) = boot.description {
                format!(
                    "{name}: {description}",
                    name = if boot.default {
                        "<default>"
                    } else {
                        boot.name.as_str()
                    }
                )
            } else {
                boot.name
            })
            .collect::<Vec<_>>()
            .join("\n"),
        error_message = select_boot
            .error_message
            .map(|err| format!("\nERROR: {err}"))
            .unwrap_or_default()
    )))
}

#[cfg(target_family = "windows")]
fn symlink_file(src: &Path, dst: &Path) -> ExitResult {
    use std::os::windows::fs::symlink_file;
    symlink_file(src, dst).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to symlink {src} -> {dst}: {e}",
            src = src.display(),
            dst = dst.display()
        ))
    })
}

#[cfg(target_family = "unix")]
fn symlink_file(src: &Path, dst: &Path) -> ExitResult {
    use std::os::unix::fs::symlink;
    let resolved_src = src.canonicalize().map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to resolve symlink source {src}: {e}",
            src = src.display()
        ))
    })?;
    symlink(resolved_src, dst).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to symlink {src} -> {dst}: {e}",
            src = src.display(),
            dst = dst.display()
        ))
    })
}

pub(crate) fn install(scie: PathBuf, commands: Vec<ScieBoot>) -> ExitResult {
    let mut symlink = false;
    let mut dest_dirs = vec![];
    for arg in env::args().skip(1) {
        match arg.as_str() {
            "-s" | "--symlink" => symlink = true,
            path => dest_dirs.push(PathBuf::from(path)),
        }
    }
    if dest_dirs.is_empty() {
        dest_dirs.push(env::current_dir().map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to determine the current directory for installing scie commands to: {e}"
            ))
        })?);
    }
    for dest_dir in dest_dirs {
        std::fs::create_dir_all(&dest_dir).map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to create destination directory {dest_dir}: {e}",
                dest_dir = dest_dir.display()
            ))
        })?;
        let mut hardlink = true;
        for command in &commands {
            let dest = dest_dir
                .join(command.name.as_str())
                .with_extension(env::consts::EXE_EXTENSION);
            if dest != scie {
                if symlink {
                    symlink_file(&scie, &dest)?;
                } else {
                    if hardlink {
                        if let Err(e) = std::fs::hard_link(&scie, &dest) {
                            hardlink = false;
                            warn!(
                                "Failed to hard link {src} to {dst}, switching to copy instead: \
                                {e}",
                                src = scie.display(),
                                dst = dest.display()
                            );
                        } else {
                            continue;
                        }
                    }
                    std::fs::copy(&scie, &dest).map_err(|e| {
                        Code::FAILURE.with_message(format!(
                            "Failed to copy {src} to {dst}: {e}",
                            src = scie.display(),
                            dst = dest.display()
                        ))
                    })?;
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn list(commands: Vec<ScieBoot>) -> ExitResult {
    for command in commands {
        println!("{}", command.name);
    }
    Ok(())
}

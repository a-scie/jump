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
    let default_cmd = select_boot
        .boots
        .iter()
        .find(|boot| boot.default)
        .map(|boot| {
            (
                "<default> (when SCIE_BOOT is not set in the environment)".to_string(),
                boot.description.as_ref().cloned().unwrap_or_default(),
            )
        });
    let mut selectable_cmds = select_boot
        .boots
        .iter()
        .filter(|boot| !boot.default)
        .filter_map(|boot| {
            boot.description
                .as_ref()
                .map(|desc| (boot.name.clone(), desc.clone()))
        })
        .collect::<Vec<_>>();

    // Only include hidden named commands when that's all there is.
    if selectable_cmds.is_empty() && default_cmd.is_none() {
        selectable_cmds.extend(
            select_boot
                .boots
                .iter()
                .filter(|boot| !boot.default)
                .map(|boot| (boot.name.clone(), "".to_string())),
        );
    }

    if selectable_cmds.is_empty() && default_cmd.is_none() {
        return Err(Code::FAILURE.with_message(format!(
            "The {scie} scie is malformed - it has no boot commands.\n\
                \n\
                You might begin debugging by inspecting the output of `SCIE=inspect {scie}`.",
            scie = select_boot.scie.invoked_as()
        )));
    }

    if default_cmd.is_some() && selectable_cmds.is_empty() {
        return Err(Code::FAILURE.with_message(format!(
            "{error_message}\n\
                \n\
                The {scie} scie contains no alternate boot commands.",
            scie = select_boot.scie.invoked_as(),
            error_message = select_boot.error_message
        )));
    }

    let maybe_scie_description = select_boot
        .description
        .map(|description| format!("{description}\n\n"))
        .unwrap_or_default();
    let max_name_width = default_cmd
        .iter()
        .chain(selectable_cmds.iter())
        .map(|(name, _)| name.len())
        .max()
        .expect("We verified we have at least one boot command earlier");
    Err(Code::FAILURE.with_message(format!(
        "{error_message}\n\
            \n\
            {maybe_scie_description}\
            Please select from the following boot commands:\n\
            \n\
            {boot_commands}\n\
            \n\
            You can select a boot command by setting the SCIE_BOOT environment variable\
            {or_else_by}.",
        boot_commands = default_cmd
            .iter()
            .chain(selectable_cmds.iter())
            .map(|(name, description)| if description.is_empty() {
                name.to_string()
            } else {
                format!("{name:<max_name_width$}  {description}")
            })
            .collect::<Vec<_>>()
            .join("\n"),
        or_else_by = if default_cmd.is_none() {
            " or else by passing it as the 1st argument"
        } else {
            ""
        },
        error_message = select_boot.error_message
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

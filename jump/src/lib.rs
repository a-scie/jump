// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

#[macro_use]
extern crate log;

#[macro_use]
extern crate structure;

mod archive;
mod atomic;
mod cmd_env;
mod comparable_regex;
pub mod config;
mod context;
pub mod fingerprint;
mod installer;
mod jump;
mod lift;
mod placeholders;
mod process;
mod zip;

use std::env;
use std::env::current_exe;
use std::path::PathBuf;

use log::Level;
use logging_timer::{time, timer};

pub use crate::archive::create_options;
use crate::config::Config;
pub use crate::config::Jump;
use crate::installer::Installer;
// Exposed for the package crate post-processing of the scie-jump binary.
pub use crate::jump::EOF_MAGIC;
pub use crate::lift::{load_lift, File, Lift, ScieBoot, Source};
pub use crate::process::{execute, EnvVar, EnvVars, Process};
pub use crate::zip::check_is_zip;

pub struct SelectBoot {
    pub boots: Vec<ScieBoot>,
    pub description: Option<String>,
    pub error_message: Option<String>,
}

const HELP: &str = "\
For SCIE=<boot_command> you can select from the following:

boot-pack
    (-sj|--jump|--scie-jump [PATH])
    (-1|--single-lift-line|--no-single-lift-line)
    [lift manifest]*

    Pack the given lift manifests into scie executables. If no manifests
    are given, looks for `lift.json` in the current directory. By
    default the current scie-jump is used as the scie tip, but an
    alternate scie-jump binary can be specified using --path. By default
    the lift manifest is appended to the tail of the scie as a single
    line JSON document, but can be made a multi-line pretty-printed JSON
    document by passing --no-single-lift-line.

help: Display this help message.

inspect: Pretty-print this scie's lift manifest to stdout.

install (-s|--symlink) [dest dir]*

    Install all the commands in this scie to each dest dir given. If no
    dest dirs are given, installs them in the current directory.

list: List the names of the commands contained in this scie.

split [directory]?

    Split this scie into its component files in the given directory or
    else the current directory if no argument is given.
";

pub enum BootAction {
    Execute((Process, bool)),
    Help((String, i32)),
    Inspect((Jump, Lift)),
    Install((PathBuf, Vec<ScieBoot>)),
    List(Vec<ScieBoot>),
    Pack((Jump, PathBuf)),
    Select(SelectBoot),
    Split((Jump, Lift, PathBuf)),
}

pub fn config(jump: Jump, mut lift: Lift) -> Config {
    let other = lift.other.take();
    Config::new(jump, lift, other)
}

struct CurrentExe {
    exe: PathBuf,
    invoked_as: PathBuf,
}

fn find_current_exe() -> Result<CurrentExe, String> {
    let exe =
        current_exe().map_err(|e| format!("Failed to find path of the current executable: {e}"))?;
    let invoked_as = if let Some(arg) = env::args_os().next() {
        PathBuf::from(arg)
    } else {
        exe.clone()
    };
    Ok(CurrentExe { exe, invoked_as })
}

#[time("debug", "jump::{}")]
pub fn prepare_boot() -> Result<BootAction, String> {
    let current_exe = find_current_exe()?;
    let file = std::fs::File::open(&current_exe.exe).map_err(|e| {
        format!(
            "Failed to open current exe at {exe} for reading: {e}",
            exe = current_exe.exe.display(),
        )
    })?;
    let data = unsafe {
        memmap::Mmap::map(&file)
            .map_err(|e| format!("Failed to mmap {exe}: {e}", exe = current_exe.exe.display()))?
    };

    if let Some(jump) = jump::load(&data, &current_exe.exe)? {
        return Ok(BootAction::Pack((jump, current_exe.exe)));
    }

    let (jump, lift) = lift::load_scie(&current_exe.exe, &data)?;
    trace!(
        "Loaded lift manifest from {current_exe}:\n{lift:#?}",
        current_exe = current_exe.exe.display()
    );

    if let Some(value) = env::var_os("SCIE") {
        if "boot-pack" == value {
            return Ok(BootAction::Pack((jump, current_exe.exe)));
        } else if "help" == value {
            return Ok(BootAction::Help((format!("{HELP}\n"), 0)));
        } else if "inspect" == value {
            return Ok(BootAction::Inspect((jump, lift)));
        } else if "install" == value {
            return Ok(BootAction::Install((current_exe.exe, lift.boots())));
        } else if "list" == value {
            return Ok(BootAction::List(lift.boots()));
        } else if "split" == value {
            return Ok(BootAction::Split((jump, lift, current_exe.exe)));
        } else if !PathBuf::from(&value).exists() {
            let help_message = format!(
                "The SCIE environment variable is set to {value:?} which is not a scie path\n\
                or one of the known SCIE boot commands.\n\
                \n\
                {HELP}\
                "
            );
            return Ok(BootAction::Help((help_message, 1)));
        }
    }

    if lift.load_dotenv {
        let _timer = timer!(Level::Debug; "jump::load_dotenv");
        if let Ok(dotenv_file) = dotenv::dotenv() {
            debug!("Loaded env file from {path}", path = dotenv_file.display());
        }
    }
    let payload = &data[jump.size..data.len() - lift.size];
    let installer = Installer::new(payload);
    let result = context::select_command(&current_exe, &jump, &lift, &installer);
    if let Ok(Some(selected_command)) = result {
        installer.install(&selected_command.files)?;
        let process = selected_command.process;
        trace!("Prepared {process:#?}");
        env::set_var("SCIE", current_exe.exe.as_os_str());
        env::set_var("SCIE_ARGV0", current_exe.invoked_as.as_os_str());
        Ok(BootAction::Execute((
            process,
            selected_command.argv1_consumed,
        )))
    } else {
        Ok(BootAction::Select(SelectBoot {
            boots: lift.boots(),
            description: lift.description,
            error_message: result.err(),
        }))
    }
}

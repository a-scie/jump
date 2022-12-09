// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

#[macro_use]
extern crate log;

#[macro_use]
extern crate structure;

mod archive;
mod atomic;
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
use std::path::PathBuf;

use logging_timer::time;

pub use crate::archive::create_options;
use crate::config::Config;
pub use crate::config::Jump;
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

boot-pack [lift manifest]*

    Pack the given lift manifests into scie executables. If no manifests
    are given, looks for `lift.json` in the current directory.

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

#[time("debug")]
pub fn prepare_boot(current_exe: PathBuf) -> Result<BootAction, String> {
    let file = std::fs::File::open(&current_exe).map_err(|e| {
        format!(
            "Failed to open current exe at {exe} for reading: {e}",
            exe = current_exe.display(),
        )
    })?;
    let data = unsafe {
        memmap::Mmap::map(&file)
            .map_err(|e| format!("Failed to mmap {exe}: {e}", exe = current_exe.display()))?
    };

    if let Some(jump) = jump::load(&data, &current_exe)? {
        return Ok(BootAction::Pack((jump, current_exe)));
    }

    let (jump, lift) = lift::load_scie(&current_exe, &data)?;
    trace!(
        "Loaded lift manifest from {current_exe}:\n{lift:#?}",
        current_exe = current_exe.display()
    );

    if let Some(value) = env::var_os("SCIE") {
        if "boot-pack" == value {
            return Ok(BootAction::Pack((jump, current_exe)));
        } else if "help" == value {
            return Ok(BootAction::Help((format!("{HELP}\n"), 0)));
        } else if "inspect" == value {
            return Ok(BootAction::Inspect((jump, lift)));
        } else if "install" == value {
            return Ok(BootAction::Install((current_exe, lift.boots())));
        } else if "list" == value {
            return Ok(BootAction::List(lift.boots()));
        } else if "split" == value {
            return Ok(BootAction::Split((jump, lift, current_exe)));
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

    let manifest_size = lift.size;
    let result = context::select_command(&current_exe, &jump, &lift);
    if let Ok(Some(selected_command)) = result {
        let payload = &data[jump.size..data.len() - manifest_size];
        installer::install(&selected_command.files, payload)?;
        let process = selected_command.process;
        trace!("Prepared {process:#?}");
        for binding in selected_command.bindings {
            binding.execute()?;
        }
        env::set_var("SCIE", current_exe.as_os_str());
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

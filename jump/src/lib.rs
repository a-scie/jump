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
use std::io::Write;
use std::path::PathBuf;

use logging_timer::time;

pub use crate::archive::create_options;
pub use crate::config::Jump;
use crate::config::{Config, Fmt};
pub use crate::context::Boot;
use crate::context::Context;
// Exposed for the package crate post-processing of the scie-jump binary.
pub use crate::jump::EOF_MAGIC;
pub use crate::lift::{load_lift, File, Lift, Scie};
pub use crate::process::{execute, EnvVar, EnvVars, Process};
pub use crate::zip::check_is_zip;

pub struct SelectBoot {
    pub boots: Vec<Boot>,
    pub description: Option<String>,
    pub error_message: Option<String>,
}

pub enum BootAction {
    Execute((Process, bool)),
    Inspect((Jump, Lift)),
    Pack((Jump, PathBuf)),
    Select(SelectBoot),
    Split((Jump, Lift, PathBuf)),
}

pub fn serialize<W: Write>(jump: Jump, lift: Lift, mut stream: W) -> Result<(), String> {
    let config = Config {
        scie: config::Scie {
            jump: Some(jump),
            lift: lift.into(),
        },
    };
    config.serialize(&mut stream, Fmt::new().pretty(true).trailing_newline(true))
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
        } else if "inspect" == value {
            return Ok(BootAction::Inspect((jump, lift)));
        } else if "split" == value {
            return Ok(BootAction::Split((jump, lift, current_exe)));
        } else if !PathBuf::from(&value).exists() {
            return Err(format!(
                "The SCIE environment variable is set to {value:?} which is not a scie path \n\
                    or one of the known SCIE boot commands.\n\
                    \n\
                    For SCIE=boot_command you can select from the following:\n\
                    boot-pack: Pack the given lift manifests into scie executables. If no \n\
                               manifests are given, looks for lift.json in the current directory.\n\
                    inspect:   Pretty-print the current scie's lift manifest to stdout.\n\
                    split:     Split this scie into its component files in the given directory \n\
                               or else the current directory if no argument is given.\n\
                    "
            ));
        }
    }

    let manifest_size = lift.size;
    let context = Context::new(current_exe, lift)?;
    let result = context.select_command();
    if let Ok(Some(selected_command)) = result {
        let payload = &data[jump.size..data.len() - manifest_size];
        let process = installer::prepare(context, selected_command.cmd, payload)?;
        trace!("Prepared {process:#?}");
        env::set_var("SCIE", selected_command.scie.as_os_str());
        Ok(BootAction::Execute((
            process,
            selected_command.argv1_consumed,
        )))
    } else {
        Ok(BootAction::Select(SelectBoot {
            boots: context.boots(),
            description: context.description,
            error_message: result.err(),
        }))
    }
}

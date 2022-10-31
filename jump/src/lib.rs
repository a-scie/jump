#[macro_use]
extern crate log;

#[macro_use]
extern crate structure;

mod atomic;
mod config;
mod context;
mod fingerprint;
mod installer;
mod jump;
mod lift;
mod placeholders;
mod process;

use std::env;
use std::fs::File;
use std::path::PathBuf;

use logging_timer::time;

pub use crate::config::Jump;
pub use crate::context::Boot;
use crate::context::Context;
// Exposed for the package crate post-processing of the scie-jump binary.
pub use crate::jump::EOF_MAGIC;
pub use crate::process::{execute, EnvVar, EnvVars, Process};

pub struct SelectBoot {
    pub boots: Vec<Boot>,
    pub description: Option<String>,
    pub error_message: Option<String>,
}

pub enum Action {
    BootPack((Jump, PathBuf)),
    BootSelect(SelectBoot),
    Execute((Process, bool)),
}

#[time("debug")]
pub fn prepare_action(current_exe: PathBuf) -> Result<Action, String> {
    let file = File::open(&current_exe).map_err(|e| {
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
        return Ok(Action::BootPack((jump, current_exe)));
    }

    let scie = lift::load(current_exe, &data)?;
    trace!("Loaded {scie:#?}");

    if let Some(value) = env::var_os("SCIE") {
        if "boot-pack" == value {
            return Ok(Action::BootPack((
                scie.jump.ok_or_else(|| {
                    format!(
                        "The intrinsic boot-pack command was selected via SCIE={value:?} \
                            but the scie at {path} has no scie-jump in its tip.",
                        path = scie.path.display()
                    )
                })?,
                scie.path,
            )));
        }
    }

    let context = Context::new(scie)?;
    let result = context.select_command();
    if let Ok(Some(selected_command)) = result {
        let process = installer::prepare(context, selected_command.cmd, &data)?;
        trace!("Prepared {process:#?}");
        env::set_var("SCIE", selected_command.scie.as_os_str());
        Ok(Action::Execute((process, selected_command.argv1_consumed)))
    } else {
        Ok(Action::BootSelect(SelectBoot {
            boots: context.boots(),
            description: context.description,
            error_message: result.err(),
        }))
    }
}

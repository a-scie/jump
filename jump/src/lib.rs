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

pub use crate::config::Jump;
pub use crate::context::Boot;
use crate::context::Context;
// Exposed for the package crate post-processing of the scie-jump binary.
pub use crate::jump::EOF_MAGIC;
pub use crate::lift::{load_lift, File, Lift, Scie};
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
        return Ok(Action::BootPack((jump, current_exe)));
    }

    let (jump, lift) = lift::load_scie(&current_exe, &data)?;
    trace!(
        "Loaded lift manifest from {current_exe}:\n{lift:#?}",
        current_exe = current_exe.display()
    );

    if let Some(value) = env::var_os("SCIE") {
        if "boot-pack" == value {
            return Ok(Action::BootPack((jump, current_exe)));
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
        Ok(Action::Execute((process, selected_command.argv1_consumed)))
    } else {
        Ok(Action::BootSelect(SelectBoot {
            boots: context.boots(),
            description: context.description,
            error_message: result.err(),
        }))
    }
}

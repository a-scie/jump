#[macro_use]
extern crate log;

#[macro_use]
extern crate structure;

mod atomic;
mod config;
mod context;
mod installer;
mod jmp;
mod placeholders;

use std::fs::File;
use std::io::{Cursor, Seek, SeekFrom};
use std::path::PathBuf;

use byteorder::{LittleEndian, ReadBytesExt};
use logging_timer::time;

pub use crate::context::Boot;
use crate::context::Context;
pub use crate::installer::{EnvVars, Process};

// Exposed for the package crate post-processing of the scie-jump binary.
pub const EOF_MAGIC: u32 = 0x534a7219;

pub struct SelectBoot {
    pub boots: Vec<Boot>,
    pub error_message: Option<String>,
}

pub enum Action {
    BootPack(u32),
    Execute((Process, bool)),
    SelectBoot(SelectBoot),
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

    let mut magic = Cursor::new(&data[data.len() - 8..]);
    magic.seek(SeekFrom::End(-4)).map_err(|e| format!("{e}"))?;
    if let Ok(EOF_MAGIC) = magic.read_u32::<LittleEndian>() {
        magic.seek(SeekFrom::End(-8)).map_err(|e| {
            format!(
                "Failed to read scie-jump size from {exe}: {e}",
                exe = current_exe.display()
            )
        })?;
        let size = magic.read_u32::<LittleEndian>().map_err(|e| {
            format!(
                "The scie-jump size of {exe} is malformed: {e}",
                exe = current_exe.display(),
            )
        })?;
        let actual_size = u32::try_from(data.len())
            .map_err(|e| format!("Expected the scie-jump launcher size to fit in 32 bits: {e}"))?;
        if actual_size != size {
            return Err(format!(
                "The scie-jump launcher at {path} has size {actual_size} but the expected \
                    size is {size}.",
                path = current_exe.display()
            ));
        }
        return Ok(Action::BootPack(size));
    }

    let scie = jmp::load(current_exe, &data)?;
    trace!("Loaded {scie:#?}");

    let context = Context::new(scie)?;
    let result = context.select_command();
    if let Ok(Some(selected_command)) = result {
        let process = installer::prepare(context, selected_command.cmd, &data)?;
        trace!("Prepared {process:#?}");

        Ok(Action::Execute((process, selected_command.argv1_consumed)))
    } else {
        Ok(Action::SelectBoot(SelectBoot {
            boots: context.boots(),
            error_message: result.err(),
        }))
    }
}

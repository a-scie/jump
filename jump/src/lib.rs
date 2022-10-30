#[macro_use]
extern crate structure;

mod config;
mod context;
mod installer;
mod jmp;

use std::fs::File;
use std::io::{Cursor, Seek, SeekFrom};
use std::path::PathBuf;

use byteorder::{LittleEndian, ReadBytesExt};

use crate::config::Cmd;

pub use crate::installer::{EnvVar, Process};

// Exposed for the package crate post-processing of the scie-jump binary.
pub const EOF_MAGIC: u32 = 0x534a7219;

pub enum Action {
    BootPack(u32),
    Execute(Process),
}

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

    let config = jmp::load(&data)?;
    let context = context::determine(current_exe, config)?;
    let process = installer::prepare(&data, context)?;
    Ok(Action::Execute(process))
}

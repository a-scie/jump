mod config;
mod extract;
mod jmp;

#[macro_use]
extern crate structure;

use std::fs::File;
use std::path::Path;

pub use crate::config::Cmd;
use crate::extract::extract;

pub fn prepare_command<P: AsRef<Path>>(current_exe: P) -> Result<Cmd, String> {
    let file = File::open(&current_exe).map_err(|e| {
        format!(
            "Failed to open current exe at {} for reading: {}",
            current_exe.as_ref().display(),
            e
        )
    })?;
    let data = unsafe {
        memmap::Mmap::map(&file)
            .map_err(|e| format!("Failed to mmap {}: {}", current_exe.as_ref().display(), e))?
    };
    let config = jmp::load(&data)?;
    let command = extract(&data, config)?;
    Ok(command)
}

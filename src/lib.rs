#[macro_use]
extern crate structure;

use std::ffi::OsString;
use std::fs::File;
use std::path::{Path, PathBuf};

use expanduser::expanduser;

mod jmp;

#[derive(Debug)]
pub struct Cmd {
    pub exe: OsString,
    pub args: Vec<OsString>,
}

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
    // TODO(John Sirois): ensure the interpreter and app are extracted to the scie root.
    let root = PathBuf::from(expanduser(config.scie.root).map_err(|e| format!("{}", e))?);
    Ok(Cmd {
        exe: root.join(config.interpreter.executable).into_os_string(),
        args: vec![root.join(config.app.script).into_os_string()],
    })
}

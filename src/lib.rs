#[macro_use]
extern crate structure;

use std::ffi::OsStr;
use std::fs::File;
use std::path::{Component, Path, PathBuf};

use bstr::ByteSlice;

mod jmp;
pub use jmp::Cmd;

fn expanduser(path: PathBuf) -> Result<PathBuf, String> {
    if !<[u8]>::from_path(&path)
        .ok_or_else(|| {
            format!(
                "Failed to decode the path {} as utf-8 bytes",
                path.display()
            )
        })?
        .contains(&b'~')
    {
        return Ok(path);
    }

    let home_dir = dirs::home_dir()
        .ok_or_else(|| format!("Failed to expand home dir in path {}", path.display()))?;
    let mut components = Vec::new();
    for path_component in path.components() {
        match path_component {
            Component::Normal(component) if OsStr::new("~") == component => {
                for home_dir_component in home_dir.components() {
                    components.push(home_dir_component)
                }
            }
            component => components.push(component),
        }
    }
    Ok(components.into_iter().collect())
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
    let _root = expanduser(config.scie.root)?;
    Ok(config.command)
}

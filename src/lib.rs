#[macro_use]
extern crate structure;

use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::path::{Component, Path, PathBuf};

mod jmp;

#[derive(Debug)]
pub struct Cmd {
    pub exe: OsString,
    pub args: Vec<OsString>,
}

fn expanduser(path: &str) -> Result<PathBuf, String> {
    let path_buf = PathBuf::from(path);
    if !path.contains('~') {
        return Ok(path_buf);
    }

    let home_dir =
        dirs::home_dir().ok_or_else(|| format!("Failed to expand home dir in path {}", path))?;
    let mut components = Vec::new();
    for path_component in path_buf.components() {
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
    let root = expanduser(&config.scie.root)?;
    Ok(Cmd {
        exe: root
            .join(PathBuf::from(config.interpreter.executable).canonicalize())
            .into_os_string(),
        args: vec![root.join(PathBuf::from(config.app.script)).into_os_string()],
    })
}

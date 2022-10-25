use bstr::ByteSlice;
use std::ffi::OsStr;
use std::path::{Component, PathBuf};

use crate::config::{Cmd, Config};

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

pub fn extract(_data: &[u8], config: Config) -> Result<Cmd, String> {
    let _root = expanduser(config.scie.root)?;
    Ok(config.command)
}

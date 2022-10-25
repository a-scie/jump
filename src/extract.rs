use std::path::Path;

use crate::config::File;

pub fn extract(_data: &[u8], _root: &Path, _files: &[File]) -> Result<(), String> {
    Ok(())
}

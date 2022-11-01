use std::path::Path;

use logging_timer::time;
use sha2::{Digest, Sha256};

#[time("debug")]
pub fn digest(data: &[u8]) -> String {
    format!("{digest:x}", digest = Sha256::digest(data))
}

#[time("debug")]
pub fn digest_file(path: &Path) -> Result<String, String> {
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path).map_err(|e| format!("{e}"))?;
    std::io::copy(&mut file, &mut hasher).map_err(|e| format!("{e}"))?;
    Ok(format!("{digest:x}", digest = hasher.finalize()))
}

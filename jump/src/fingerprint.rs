use std::path::Path;

use logging_timer::time;
use sha2::{Digest, Sha256};

#[time("debug")]
pub fn digest(data: &[u8]) -> String {
    format!("{digest:x}", digest = Sha256::digest(data))
}

#[time("debug")]
pub fn digest_file(path: &Path) -> Result<(usize, String), String> {
    let mut hasher = Sha256::new();
    let mut file = std::fs::File::open(path).map_err(|e| {
        format!(
            "Failed to open {path} for digesting: {e}",
            path = path.display()
        )
    })?;
    let copied_size = std::io::copy(&mut file, &mut hasher)
        .map_err(|e| format!("Failed to digest {path}: {e}", path = path.display()))?;
    let file_size = usize::try_from(copied_size).map_err(|e| {
        format!(
            "Read {copied_size} bytes from {path} which was more than can fit in a usize which \
            is {usize_bits} bits on this platform: {e}",
            path = path.display(),
            usize_bits = usize::BITS
        )
    })?;
    let hash = format!("{digest:x}", digest = hasher.finalize());
    Ok((file_size, hash))
}

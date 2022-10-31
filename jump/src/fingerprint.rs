use logging_timer::time;
use sha2::{Digest, Sha256};

#[time("debug")]
pub(crate) fn digest(data: &[u8]) -> String {
    format!("{digest:x}", digest = Sha256::digest(data))
}

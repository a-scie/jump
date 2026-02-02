// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::io::Read;
use std::path::Path;

use logging_timer::time;
use sha2::{Digest, Sha256};

#[time("debug", "fingerprint::{}")]
pub fn digest(data: &[u8]) -> String {
    format!("{digest:x}", digest = Sha256::digest(data))
}

pub fn digest_file(path: &Path) -> Result<(u64, String), String> {
    let file = std::fs::File::open(path).map_err(|e| {
        format!(
            "Failed to open {path} for digesting: {e}",
            path = path.display()
        )
    })?;
    digest_reader(file)
}

#[time("debug", "fingerprint::{}")]
pub fn digest_reader<R: Read>(mut reader: R) -> Result<(u64, String), String> {
    let mut hasher = Sha256::new();
    let copied_size = std::io::copy(&mut reader, &mut hasher)
        .map_err(|e| format!("Failed to digest stream: {e}"))?;
    let hash = format!("{digest:x}", digest = hasher.finalize());
    Ok((copied_size, hash))
}

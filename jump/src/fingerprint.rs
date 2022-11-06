// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::io::Read;
use std::path::Path;

use logging_timer::time;
use sha2::{Digest, Sha256};

#[time("debug")]
pub fn digest(data: &[u8]) -> String {
    format!("{digest:x}", digest = Sha256::digest(data))
}

pub(crate) fn digest_file(path: &Path) -> Result<(usize, String), String> {
    let file = std::fs::File::open(path).map_err(|e| {
        format!(
            "Failed to open {path} for digesting: {e}",
            path = path.display()
        )
    })?;
    digest_reader(file)
}

#[time("debug")]
pub fn digest_reader<R: Read>(mut reader: R) -> Result<(usize, String), String> {
    let mut hasher = Sha256::new();
    let copied_size = std::io::copy(&mut reader, &mut hasher)
        .map_err(|e| format!("Failed to digest stream: {e}"))?;
    let file_size = usize::try_from(copied_size).map_err(|e| {
        format!(
            "Read {copied_size} bytes from stream which was more than can fit in a usize which \
            is {usize_bits} bits on this platform: {e}",
            usize_bits = usize::BITS
        )
    })?;
    let hash = format!("{digest:x}", digest = hasher.finalize());
    Ok((file_size, hash))
}

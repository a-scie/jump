// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::io;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use logging_timer::time;
use sha2::{Digest, Sha256};

#[time("debug", "fingerprint::{}")]
pub fn digest(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
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
pub fn digest_reader<R: Read>(reader: R) -> Result<(u64, String), String> {
    let mut hasher = Sha256::new();
    let copied_size =
        hash_reader(reader, &mut hasher).map_err(|e| format!("Failed to digest stream: {e}"))?;
    let hash = hex::encode(hasher.finalize());
    let size = u64::try_from(copied_size)
        .map_err(|err| format!("File was bigger than a u64 at {copied_size} bytes!: {err}"))?;
    Ok((size, hash))
}

pub fn hash_reader<R, D>(reader: R, digest: &mut D) -> Result<usize, io::Error>
where
    R: Read,
    D: Digest,
{
    let mut size: usize = 0;
    let mut reader = BufReader::new(reader);
    loop {
        let amount_read = {
            let buf = reader.fill_buf()?;
            if buf.is_empty() {
                return Ok(size);
            }
            digest.update(buf);
            buf.len()
        };
        size += amount_read;
        reader.consume(amount_read);
    }
}

// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::io::{Cursor, Seek, SeekFrom};
use std::path::Path;

use byteorder::{LittleEndian, ReadBytesExt};

pub const EOF_MAGIC: u32 = 0x534a7219;
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

use crate::config::Jump;

pub fn load(data: &[u8], path: &Path) -> Result<Option<Jump>, String> {
    let mut magic = Cursor::new(&data[data.len() - 8..]);
    magic.seek(SeekFrom::End(-4)).map_err(|e| format!("{e}"))?;
    if let Ok(EOF_MAGIC) = magic.read_u32::<LittleEndian>() {
        magic.seek(SeekFrom::End(-8)).map_err(|e| {
            format!(
                "Failed to read scie-jump size from {path}: {e}",
                path = path.display()
            )
        })?;
        let size = magic.read_u32::<LittleEndian>().map_err(|e| {
            format!(
                "The scie-jump size of {path} is malformed: {e}",
                path = path.display()
            )
        })?;
        let actual_size = u32::try_from(data.len())
            .map_err(|e| format!("Expected the scie-jump launcher size to fit in 32 bits: {e}"))?;
        if actual_size != size {
            return Err(format!(
                "The scie-jump launcher at {path} has size {actual_size} but the expected \
                size is {expected_size}.",
                path = path.display(),
                expected_size = size
            ));
        }
        return Ok(Some(Jump {
            version: VERSION.to_string(),
            size: size as usize,
        }));
    }
    Ok(None)
}

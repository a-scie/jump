// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use bstr::ByteSlice;
use byteorder::{LittleEndian, ReadBytesExt};

pub const EOF_MAGIC_V1: u32 = 0x534a7219;
pub const EOF_MAGIC_V2: u32 = 0x4a532520;

use crate::config::Jump;

fn read_size<D: Read + Seek>(data: &mut D, path: &Path) -> Result<usize, String> {
    let size = data
        .seek(SeekFrom::End(-8))
        .and_then(|_| data.read_u32::<LittleEndian>())
        .map_err(|e| {
            format!(
                "Failed to read scie-jump size from {path}: {e}",
                path = path.display()
            )
        })? as u64;
    let actual_size = std::fs::File::open(path)
        .and_then(|file| file.metadata())
        .map_err(|e| {
            format!(
                "Failed to determine the actual size of the scie-jump launcher at {path}: {e}",
                path = path.display()
            )
        })?
        .len();
    if actual_size != size {
        return Err(format!(
            "The scie-jump launcher at {path} has size {actual_size} but the expected \
                size is {expected_size}.",
            path = path.display(),
            expected_size = size
        ));
    }
    Ok(size as usize)
}

fn read_version<D: Read + Seek>(data: &mut D, path: &Path) -> Result<String, String> {
    let version_size = data
        .seek(SeekFrom::End(-9))
        .and_then(|_| data.read_u8())
        .map_err(|e| {
            format!(
                "Failed to read scie-jump version size from {path}: {e}",
                path = path.display()
            )
        })?;
    let mut version = [0; 255];
    data.seek(SeekFrom::End(-9 - (version_size as i64)))
        .and_then(|_| data.read_exact(&mut version[0..(version_size as usize)]))
        .map_err(|e| {
            format!(
                "Failed to read scie-jump version from {path}: {e}",
                path = path.display()
            )
        })?;
    str::from_utf8(&version[0..(version_size as usize)])
        .map(String::from)
        .map_err(|e| {
            format!(
                "Failed to read scie-jump version as a utf8 string from {path}: {e}",
                path = path.display()
            )
        })
}

fn query_version(path: &Path) -> Result<String, String> {
    let output = Command::new(path)
        .arg("-V")
        .stdout(Stdio::piped())
        .spawn()
        .and_then(Child::wait_with_output)
        .map_err(|e| {
            format!(
                "Failed to query scie-jump version via `{path} -V`: {e}",
                path = path.display()
            )
        })?;
    String::from_utf8(output.stdout.trim_end().to_vec()).map_err(|e| {
        format!(
            "Failed to read scie-jump version as a utf8 string from `{path} -V` output: {e}",
            path = path.display()
        )
    })
}

pub fn load<D: Read + Seek>(mut data: D, path: &Path) -> Result<Option<Jump>, String> {
    data.seek(SeekFrom::End(-4)).map_err(|e| {
        format!(
            "Failed to read scie-jump trailer magic from {path}: {e}",
            path = path.display()
        )
    })?;
    match data.read_u32::<LittleEndian>() {
        Ok(EOF_MAGIC_V1) => {
            let size = read_size(&mut data, path)?;
            let version = query_version(path)?;
            Ok(Some(Jump { version, size }))
        }
        Ok(EOF_MAGIC_V2) => {
            let size = read_size(&mut data, path)?;
            let version = read_version(&mut data, path)?;
            Ok(Some(Jump { version, size }))
        }
        _ => Ok(None),
    }
}

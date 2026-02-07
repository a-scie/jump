// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use byteorder::{LittleEndian, ReadBytesExt};
use log::warn;
use semver::Version;

use crate::config::Jump;
use crate::fingerprint;

pub const EOF_MAGIC_V1: u32 = 0x534a7219;
pub const EOF_MAGIC_V2: u32 = 0x4a532520;

const SCIE_JUMP_VERSION_INTRODUCING_HASH: Version = Version::new(1, 11, 0);

pub fn hash_jump(jump: &Jump, path: &Path) -> Result<Option<String>, String> {
    let mut data = File::open(path).map_err(|e| {
        format!(
            "Failed to open {path} to hash scie-jump contents: {e}",
            path = path.display()
        )
    })?;
    if jump.version >= SCIE_JUMP_VERSION_INTRODUCING_HASH {
        data.rewind().map_err(|e| {
            format!(
                "Failed to rewind open scie-jump file {path} to hash it: {e}",
                path = path.display()
            )
        })?;
        let (_, hash) = fingerprint::digest_reader(data.take(u64::from(jump.size)))?;
        Ok(Some(hash))
    } else {
        Ok(None)
    }
}

fn read_size<D: Read + Seek>(data: &mut D, path: &Path) -> Result<u32, String> {
    let size = data
        .seek(SeekFrom::End(-8))
        .and_then(|_| data.read_u32::<LittleEndian>())
        .map_err(|e| {
            format!(
                "Failed to read scie-jump size from {path}: {e}",
                path = path.display()
            )
        })?;
    let actual_size = std::fs::File::open(path)
        .and_then(|file| file.metadata())
        .map_err(|e| {
            format!(
                "Failed to determine the actual size of the scie-jump launcher at {path}: {e}",
                path = path.display()
            )
        })?
        .len();
    if actual_size != u64::from(size) {
        return Err(format!(
            "The scie-jump launcher at {path} has size {actual_size} but the expected \
                size is {expected_size}.",
            path = path.display(),
            expected_size = size
        ));
    }
    Ok(size)
}

fn as_version(version: &[u8], path: &Path) -> Result<Version, String> {
    let version_str = str::from_utf8(version).map_err(|e| {
        format!(
            "Failed to convert scie-jump version of {path} to a utf8 string: {e}",
            path = path.display()
        )
    })?;
    Version::parse(version_str).map_err(|e| {
        format!(
            "Failed to parse scie-jump version {version_str} from {path}: {e}",
            path = path.display()
        )
    })
}

fn read_version<D: Read + Seek>(data: &mut D, path: &Path) -> Result<Version, String> {
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
    data.seek(SeekFrom::End(-9 - (i64::from(version_size))))
        .and_then(|_| data.read_exact(&mut version[0..usize::from(version_size)]))
        .map_err(|e| {
            format!(
                "Failed to read scie-jump version from {path}: {e}",
                path = path.display()
            )
        })?;
    as_version(&version[0..usize::from(version_size)], path)
}

fn query_version(path: &Path) -> Result<Version, String> {
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
    as_version(output.stdout.trim_ascii_end(), path)
}

pub fn load(path: &Path, current_scie_jump_version: &Version) -> Result<Option<Jump>, String> {
    let mut data = std::fs::File::open(path).map_err(|e| {
        format!(
            "Failed to open scie-jump at {path} for reading: {e}",
            path = path.display(),
        )
    })?;
    data.seek(SeekFrom::End(-4)).map_err(|e| {
        format!(
            "Failed to read scie-jump trailer magic from {path}: {e}",
            path = path.display()
        )
    })?;
    match data.read_u32::<LittleEndian>() {
        Ok(EOF_MAGIC_V1) => {
            let size = read_size(&mut data, path)?;
            let version = match query_version(path) {
                Ok(version) => version,
                Err(err) => {
                    // N.B.: The query will fail if the scie-jump is for a foreign platform, and we
                    // fall back to the current scie-jump version in that case. This is buggy, but
                    // its was also a long-standing bug only fixed by the switch to the EOF_MAGIC_V2
                    // scheme; so this is strictly an improvement over the old status quo where the
                    // version, if different in reality, was always incorrect, but now is queried
                    // correctly if the platform matches and warned about if not.
                    warn!(
                        "Failed to determine version of the custom scie-jump at {path}: {err}",
                        path = path.display()
                    );
                    warn!(
                        "Reporting {current_scie_jump_version} (the version of current scie-jump) \
                        in its place which is generally misleading but harmless.\n\
                        You can avoid this problem by using using a custom scie-jump with version \
                        1.8.2 or newer."
                    );
                    current_scie_jump_version.clone()
                }
            };
            Ok(Some(Jump::new(size, version)))
        }
        Ok(EOF_MAGIC_V2) => {
            let size = read_size(&mut data, path)?;
            let version = read_version(&mut data, path)?;
            Ok(Some(Jump::new(size, version)))
        }
        _ => Ok(None),
    }
}

// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::cmp::min;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use itertools::Itertools;

// See "4.3.6 Overall .ZIP file format:" and "4.3.16  End of central directory record:"
// in https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT for Zip file format facts
// leveraged here.

const EOCD_SIGNATURE: (&u8, &u8, &u8, &u8) = (&0x06, &0x05, &0x4b, &0x50);
const EOCD_MIN_SIZE: usize = 22;
const EOCD_MAX_SIZE: usize = EOCD_MIN_SIZE + u16::MAX as usize;

pub(crate) fn end_of_zip(data: &[u8], maximum_trailer_size: usize) -> Result<usize, String> {
    #[allow(clippy::too_many_arguments)]
    let eocd_struct = structure!("<4sHHHHIIH");
    debug_assert!(EOCD_MIN_SIZE == eocd_struct.size());

    let max_scan = EOCD_MAX_SIZE + maximum_trailer_size;
    let max_signature_position = data.len() - EOCD_MIN_SIZE + 4;

    let offset_from_eof = EOCD_MIN_SIZE
        + data[..max_signature_position]
            .iter()
            .rev()
            .take(max_scan)
            .tuple_windows::<(_, _, _, _)>()
            .position(|chunk| EOCD_SIGNATURE == chunk)
            .ok_or_else(|| {
                format!(
                "Failed to find application zip end of central directory record within the last \
                {max_scan} bytes of the file. Invalid NCE."
            )
            })?;
    let eocd_start = data.len() - offset_from_eof;
    let eocd_end = eocd_start + EOCD_MIN_SIZE;
    let (
        _signature,
        _disk_no,
        _cd_disk_no,
        _disk_cd_record_count,
        _total_cd_record_count,
        _cd_size,
        _cd_offset,
        zip_comment_size,
    ) = eocd_struct
        .unpack(&data[eocd_start..eocd_end])
        .map_err(|e| {
            format!(
                "Invalid end of central directory record found starting at byte {eocd_start}: {e}"
            )
        })?;
    Ok(eocd_end + (zip_comment_size as usize))
}

pub fn check_is_zip(path: &Path) -> Result<(), String> {
    let mut file = std::fs::File::open(path).map_err(|e| {
        format!(
            "Failed to open zip {zip} for reading: {e}",
            zip = path.display()
        )
    })?;
    let file_size = file
        .metadata()
        .map_err(|e| {
            format!(
                "Failed to determine the size of the file at {path}: {e}",
                path = path.display()
            )
        })?
        .len();
    let seek = min(EOCD_MAX_SIZE, file_size as usize);
    file.seek(SeekFrom::End(-(seek as i64))).map_err(|e| {
        format!(
            "Failed to reset stream pointer for {file_size} byte file {path} to position \
                {seek} from the end: {e}",
            path = path.display()
        )
    })?;
    let mut buffer = Vec::with_capacity(seek);
    file.read_to_end(&mut buffer).map_err(|e| {
        format!(
            "Failed to read last {seek} bytes of {path} to check for a zip end of central \
            directory record: {e}",
            path = path.display()
        )
    })?;
    end_of_zip(&buffer, 0).map(|_| ())
}

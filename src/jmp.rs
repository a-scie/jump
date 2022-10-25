use itertools::Itertools;

use crate::config::Config;

const MAXIMUM_CONFIG_SIZE: usize = 0xFFFF;

// See "4.3.6 Overall .ZIP file format:" and "4.3.16  End of central directory record:"
// in https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT for Zip file format facts
// leveraged here.

const EOCD_SIGNATURE: (&u8, &u8, &u8, &u8) = (&0x06, &0x05, &0x4b, &0x50);

pub fn end_of_zip(data: &[u8], maximum_trailer_size: usize) -> Result<usize, String> {
    #[allow(clippy::too_many_arguments)]
    let eocd_struct = structure!("<HHHHIIH");

    let eocd_size = eocd_struct.size();
    // N.B.: The variable length comment field can be up to 0xFFFF big.
    let maximum_eocd_size = eocd_size + 0xFFFF;
    let max_scan = maximum_eocd_size + maximum_trailer_size;

    let offset_from_eof = data
        .iter()
        .rev()
        .take(max_scan)
        .tuple_windows::<(_, _, _, _)>()
        .position(|chunk| EOCD_SIGNATURE == chunk)
        .ok_or_else(|| {
            format!(
                "Failed to find application zip end of central directory record within the last \
                {} bytes of the file. Invalid NCE.",
                max_scan
            )
        })?;
    let eocd_start = data.len() - offset_from_eof;
    let eocd_end = eocd_start + eocd_size;
    let (
        _disk_no,
        _cd_disk_no,
        _disk_cd_record_count,
        _total_cd_record_count,
        _cd_size,
        _cd_offset,
        zip_comment_size,
    ) = eocd_struct
        .unpack(&data[eocd_start..eocd_end])
        .map_err(|e| format!("{}", e))?;
    Ok(eocd_end + (zip_comment_size as usize))
}

pub fn load(data: &[u8]) -> Result<Config, String> {
    let end_of_zip = end_of_zip(data, MAXIMUM_CONFIG_SIZE)?;
    serde_json::from_slice(&data[end_of_zip..]).map_err(|e| format!("{}", e))
}

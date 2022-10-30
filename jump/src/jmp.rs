use itertools::Itertools;
use logging_timer::time;

use crate::config::Config;

const MAXIMUM_CONFIG_SIZE: usize = 0xFFFF;

// See "4.3.6 Overall .ZIP file format:" and "4.3.16  End of central directory record:"
// in https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT for Zip file format facts
// leveraged here.

const EOCD_SIGNATURE: (&u8, &u8, &u8, &u8) = (&0x06, &0x05, &0x4b, &0x50);

pub fn end_of_zip(data: &[u8], maximum_trailer_size: usize) -> Result<usize, String> {
    #[allow(clippy::too_many_arguments)]
    let eocd_struct = structure!("<4sHHHHIIH");

    let eocd_size = eocd_struct.size();
    let maximum_eocd_size = eocd_size + u16::MAX as usize;
    let max_scan = maximum_eocd_size + maximum_trailer_size;
    let max_signature_position = data.len() - eocd_size + 4;

    let offset_from_eof = eocd_size
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
    let eocd_end = eocd_start + eocd_size;
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

#[time("debug")]
pub fn load(data: &[u8]) -> Result<Config, String> {
    let end_of_zip = end_of_zip(data, MAXIMUM_CONFIG_SIZE)?;
    let config_bytes = &data[end_of_zip..];
    let mut config: Config = serde_json::from_slice(config_bytes)
        .map_err(|e| format!("Failed to decode scie jmp config: {e}"))?;
    config.size = config_bytes.len();
    Ok(config)
}

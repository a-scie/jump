use std::fs::File;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::os::unix::fs::MetadataExt;

const EOCD_SIGNATURE: &[u8] = b"\x50\x4b\x05\x06";

struct EndOfCentralDirectoryRecord {
    size: u32,
    cd_size: u32,
    cd_offset: u32,
}

impl EndOfCentralDirectoryRecord {
    fn load(file: &mut File) -> Result<Self, String> {
        #[allow(clippy::too_many_arguments)]
        let eocd_struct = structure!("<4sHHHHIIH");

        let eocd_size = eocd_struct.size();

        let file_size = usize::try_from(file.metadata().map_err(|e| format!("{}", e))?.size())
            .map_err(|e| format!("{}", e))?;
        if file_size < eocd_size {
            return Err(format!(
                "File is not big enough to be a zip at {} bytes",
                file_size
            ));
        }

        file.seek(SeekFrom::End(
            -i64::try_from(eocd_size).map_err(|e| format!("{}", e))?,
        ))
        .map_err(|e| format!("{}", e))?;
        let (
            signature,
            _disk_no,
            _cd_disk_no,
            _disk_cd_record_count,
            _total_cd_record_count,
            cd_size,
            cd_offset,
            _zip_comment_size,
        ) = eocd_struct
            .unpack_from(file)
            .map_err(|e| format!("{}", e))?;
        if EOCD_SIGNATURE == signature {
            return Ok(EndOfCentralDirectoryRecord {
                size: u32::try_from(eocd_size).expect(
                    "The Zip end of central directory record should never be bigger than a 32 \
                    bit integer",
                ),
                cd_size,
                cd_offset,
            });
        }

        let max_record_size = std::cmp::min(file_size, eocd_size + 0xFFFF);
        file.seek(SeekFrom::End(
            -i64::try_from(max_record_size).map_err(|e| format!("{}", e))?,
        ))
        .map_err(|e| format!("{}", e))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .map_err(|e| format!("{}", e))?;
        let offset = buffer
            .windows(4)
            .position(|window| EOCD_SIGNATURE == window)
            .ok_or_else(|| "Failed to find the zip end of central directory record.".to_string())?;
        let (
            signature,
            _disk_no,
            _cd_disk_no,
            _disk_cd_record_count,
            _total_cd_record_count,
            cd_size,
            cd_offset,
            _zip_comment_size,
        ) = eocd_struct
            .unpack(&buffer[offset..offset + eocd_struct.size()])
            .map_err(|e| format!("{}", e))?;
        if signature == b"\x50\x4b\x05\x06" {
            return Ok(EndOfCentralDirectoryRecord {
                size: u32::try_from(max_record_size - offset).map_err(|e| format!("{}", e))?,
                cd_size,
                cd_offset,
            });
        }

        Err(format!(
            "Failed to find EOCD. The signature was: {}",
            signature
                .iter()
                .map(|b| format!("{:#x?}", b))
                .collect::<Vec<_>>()
                .join(" ")
        ))
    }
}

pub fn start_offset_from_eof(file: &mut File) -> Result<u64, String> {
    let eocd = EndOfCentralDirectoryRecord::load(file)?;
    Ok((eocd.size + eocd.cd_size + eocd.cd_offset) as u64)
}

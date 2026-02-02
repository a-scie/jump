// Copyright 2026 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::io::{Error, ErrorKind, Read, Seek, SeekFrom};

pub struct WindowedReader<'a, R> {
    reader: &'a mut R,
    start: u64,
    len: u64,
    offset: u64,
}

impl<'a, R: Seek> WindowedReader<'a, R> {
    pub fn new(reader: &'a mut R, start: u64, len: u64) -> Result<Self, String> {
        reader
            .seek(SeekFrom::Start(start))
            .map_err(|e| format!("Failed to seek to start of window at {start}: {e}"))?;
        Ok(Self {
            reader,
            start,
            len,
            offset: 0,
        })
    }
}

impl<'a, R: Read> Read for WindowedReader<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let amount_read = self.reader.take(self.len - self.offset).read(buf)?;
        self.offset += u64::try_from(amount_read).map_err(|e| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("Read more than 2^64 bits: {e}"),
            )
        })?;
        Ok(amount_read)
    }
}

impl<'a, S: Seek> Seek for WindowedReader<'a, S> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let seek_from = match pos {
            SeekFrom::Start(amount) => {
                let seek_to = self.start + amount;
                SeekFrom::Start(seek_to)
            }
            SeekFrom::End(amount) => {
                let end = self.start + self.len;
                let seek_to = end.checked_add_signed(amount).ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidInput,
                        format!("Could not seek {amount} from {end} in underlying stream."),
                    )
                })?;
                SeekFrom::Start(seek_to)
            }
            SeekFrom::Current(amount) => {
                let pos = self.start + self.offset;
                let seek_to = pos.checked_add_signed(amount).ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidInput,
                        format!("Could not seek {amount} from {pos} in underlying stream."),
                    )
                })?;
                SeekFrom::Start(seek_to)
            }
        };
        let pos = self.reader.seek(seek_from)?;
        self.offset = pos.checked_sub(self.start).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("Failed to seek by {seek_from:?} in underlying stream."),
            )
        })?;
        Ok(self.offset)
    }
}

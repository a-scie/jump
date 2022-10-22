#[macro_use]
extern crate structure;

mod zip;

use std::fs::File;

pub fn message(file: &mut File) -> String {
    format!("Hello, world!: {:?}", zip::start_offset_from_eof(file))
}

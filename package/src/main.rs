// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::env;
use std::path::PathBuf;

use proc_exit::{Code, ExitResult};

fn main() -> ExitResult {
    if std::env::args().len() != 2 {
        return Err(Code::FAILURE.with_message(
            "Usage: cargo run -p package <dest dir>\n\
            \n\
            A destination directory for the packaged scie-jump executable is \
            required.",
        ));
    }

    let src: PathBuf = env!("SCIE_STRAP").into();
    let dest_dir: PathBuf = std::env::args().nth(1).unwrap().into();
    let dst = {
        let dst = dest_dir.join(src.file_name().ok_or_else(|| {
            Code::FAILURE.with_message(format!("Expected {} to end in a file name.", src.display()))
        })?);
        if dst.extension().is_none() {
            dst.with_extension(std::env::consts::EXE_EXTENSION)
        } else {
            dst
        }
    };

    if dest_dir.is_file() {
        return Err(Code::FAILURE.with_message(format!(
            "The specified dest_dir of {} is a file. Not overwriting",
            dest_dir.display()
        )));
    }

    std::fs::create_dir_all(&dest_dir).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to create dest_dir {dest_dir}: {e}",
            dest_dir = dest_dir.display()
        ))
    })?;
    std::fs::copy(&src, &dst).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to copy {src} to {dst}: {e}",
            src = src.display(),
            dst = dst.display()
        ))
    })?;

    eprintln!("Wrote the scie-jump to {}", dst.display());
    Code::SUCCESS.ok()
}

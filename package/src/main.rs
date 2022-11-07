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
    let dst_dir: PathBuf = std::env::args().nth(1).unwrap().into();
    let dst = {
        let file_name = src.file_name().ok_or_else(|| {
            Code::FAILURE.with_message(format!(
                "Expected {src} to end in a file name.",
                src = src.display()
            ))
        })?;
        let dst = dst_dir.join(file_name);
        if dst.extension().is_none() {
            dst.with_extension(std::env::consts::EXE_EXTENSION)
        } else {
            dst
        }
    };

    if dst_dir.is_file() {
        return Err(Code::FAILURE.with_message(format!(
            "The specified dest_dir of {} is a file. Not overwriting",
            dst_dir.display()
        )));
    }

    std::fs::create_dir_all(&dst_dir).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to create dest_dir {dest_dir}: {e}",
            dest_dir = dst_dir.display()
        ))
    })?;
    std::fs::copy(&src, &dst).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to copy {src} to {dst}: {e}",
            src = src.display(),
            dst = dst.display()
        ))
    })?;

    let file_name_raw = dst.file_name().ok_or_else(|| {
        Code::FAILURE.with_message(format!(
            "Expected {dst} to end in a file name.",
            dst = dst.display()
        ))
    })?;
    let file_name = file_name_raw.to_os_string().into_string().map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to interpret scie-jump file name as a utf-8 string: {e:?}"
        ))
    })?;
    let fingerprint_file = dst.with_file_name(format!("{file_name}.sha256"));
    std::fs::write(
        &fingerprint_file,
        format!("{hash} *{file_name}\n", hash = env!("SCIE_SHA256")),
    )
    .map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to write fingerprint file {fingerprint_file}: {e}",
            fingerprint_file = fingerprint_file.display()
        ))
    })?;

    eprintln!("Wrote the scie-jump to {}", dst.display());
    Code::SUCCESS.ok()
}

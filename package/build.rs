// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

fn env(name: &str) -> Result<String, String> {
    std::env::var(name).map_err(|e| format!("Expected {name} to be set for build script: {e}"))
}

fn main() -> Result<(), String> {
    println!(
        "cargo:rustc-env=OUT_DIR={out_dir}",
        out_dir = env("OUT_DIR")?
    );
    println!("cargo:rustc-env=TARGET={target}", target = env("TARGET")?);
    Ok(())
}

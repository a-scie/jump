// Copyright 2026 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

#[cfg(windows)]
fn embed_resources() -> Result<(), String> {
    embed_resource::compile_for(
        r"assets\resources\scie-jump-icon-console.rc",
        ["scie-jump"],
        embed_resource::NONE,
    )
    .manifest_required()
    .map_err(|err| format!("Failed to compile Windows icon resource: {err}"))?;
    embed_resource::compile_for(
        r"assets\resources\scie-jump-icon-gui.rc",
        ["scie-jumpw"],
        embed_resource::NONE,
    )
    .manifest_required()
    .map_err(|err| format!("Failed to compile Windows icon resource: {err}"))?;
    Ok(())
}

fn main() -> Result<(), String> {
    #[cfg(windows)]
    embed_resources()?;
    Ok(())
}

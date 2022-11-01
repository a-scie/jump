use std::env;
use std::path::{Path, PathBuf};

use jump::config::Config;
use jump::Jump;
use proc_exit::{Code, ExitResult};

fn load_manifest(path: &Path, jump: &Jump) -> Result<Config, String> {
    let manifest_path = if path.is_dir() {
        path.join("lift.json")
    } else {
        path.to_path_buf()
    };
    if !manifest_path.is_file() {
        return Err(format!(
            "The given path does not contain a lift manifest: {path}",
            path = path.display()
        ));
    }
    let mut config = Config::read_from(std::fs::File::open(&manifest_path).map_err(|e| {
        format!(
            "Failed to open lift manifest at {path}: {e}",
            path = manifest_path.display()
        )
    })?)
    .map_err(|e| {
        format!(
            "Failed to read lift manifest from {path}: {e}",
            path = manifest_path.display()
        )
    })?;
    if let Some(expected_jump) = config.scie.jump {
        if &expected_jump != jump {
            return Err(format!(
                "The manifest at {path} expected to be lifted into a binary with {expected_jump:?} \
                but the current is {jump:?}.",
                path = manifest_path.display()
            ));
        }
    }
    config.scie.jump = Some(jump.to_owned());
    Ok(config)
}

fn pack(config: &Config, scie_jump_path: &PathBuf, single_line: bool) -> Result<PathBuf, String> {
    eprintln!(
        "TODO(John Sirois): \n\
        Using scie-jump: {scie_jump_path:#?}\n\
        Use single_line: {single_line}\n\
        Lift manifest:\n\
        {config:#?}"
    );
    env::current_dir()
        .map(|cwd| cwd.join(&config.scie.lift.name))
        .map_err(|e| format!("{e}"))
}

pub(crate) fn set(jump: Jump, scie_jump_path: PathBuf) -> ExitResult {
    // X1. Optional path: Default name is lift.json in CWD if no path or path is a directory.
    // X2. Open the lift manifest and Config::parse
    // 3. Set CWD to lift manifest parent dir.
    // 3. Must have >=1 file and for each file check hash (and size) or fail.
    // 4. Output name is the lift manifest name, but in original CWD.
    // XOptional --single-lift-line / --no-single-lift-line for lift manifest trailer packing.
    let mut lifts = vec![];
    let mut single_line = false;
    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--single-lift-line" => single_line = true,
            "--no-single-lift-line" => single_line = false,
            _ => {
                lifts.push(
                    load_manifest(Path::new(arg.as_str()), &jump)
                        .map_err(|e| Code::FAILURE.with_message(e))?,
                );
            }
        }
    }
    if lifts.is_empty() {
        if let Ok(cwd) = env::current_dir() {
            lifts.push(load_manifest(&cwd, &jump).map_err(|e| Code::FAILURE.with_message(e))?);
        }
    }

    if lifts.is_empty() {
        return Err(Code::FAILURE.with_message(
            "Found no lift manifests to process. Either include paths to lift manifest \
                files as arguments or else paths to directories containing lift manifest files \
                named `lift.json`.",
        ));
    }

    let results = lifts
        .into_iter()
        .map(|config| pack(&config, &scie_jump_path, single_line).map(|path| (config, path)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Code::FAILURE.with_message(e))?;
    for (config, path) in results {
        println!(
            "{manifest}: {binary}",
            manifest = config.scie.path.display(),
            binary = path.display()
        );
    }
    Code::SUCCESS.ok()
}

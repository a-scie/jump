use std::env;
use std::io::Read;
use std::path::{Path, PathBuf};

use jump::config::Config;
use jump::{load_lift, File, Jump, Lift, Scie};
use logging_timer::time;
use proc_exit::{Code, ExitResult};

#[time("debug")]
fn load_manifest(path: &Path, jump: &Jump) -> Result<(Lift, PathBuf), String> {
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
    let (maybe_jump, lift) = load_lift(&manifest_path)?;
    if let Some(ref configured_jump) = maybe_jump {
        if jump != configured_jump {
            return Err(format!(
                "The lift manifest {manifest} specifies a scie jump binary of \
                    {configured_jump:?} that does not match the current of {jump:?}.",
                manifest = manifest_path.display()
            ));
        }
    }
    Ok((lift, manifest_path))
}

#[cfg(target_family = "windows")]
fn finalize_executable(path: &Path) -> Result<PathBuf, String> {
    if path.extension().is_none() {
        let exe = path.with_extension(env::consts::EXE_EXTENSION);
        std::fs::rename(path, &exe).map_err(|e| {
            format!(
                "Failed to rename executable from {path} to {exe}: {e}",
                path = path.display(),
                exe = exe.display()
            )
        })?;
        return Ok(exe);
    }
    Ok(path.to_path_buf())
}

#[cfg(not(target_family = "windows"))]
fn finalize_executable(path: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)
        .map_err(|e| {
            format!(
                "Failed to access permissions metadata for {binary}: {e}",
                binary = path.display()
            )
        })?
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).map_err(|e| {
        format!(
            "Failed to mark {binary} as executable: {e}",
            binary = path.display()
        )
    })?;
    Ok(path.to_path_buf())
}

#[time("debug")]
fn pack(
    lift: Lift,
    manifest_path: &Path,
    scie_jump_path: &Path,
    scie_jump_size: usize,
    single_line: bool,
) -> Result<PathBuf, String> {
    let binary_path = env::current_dir()
        .map(|cwd| cwd.join(&lift.name))
        .map_err(|e| format!("Failed to determine the output directory for scies: {e}"))?;
    let mut binary = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&binary_path)
        .map_err(|e| {
            format!(
                "Failed to open binary {path} for writing {lift:?}: {e}",
                path = binary_path.display(),
            )
        })?;
    let mut scie_jump = std::fs::File::open(scie_jump_path)
        .map_err(|e| {
            format!(
                "Failed to open scie-jump binary {path} for writing to the tip of {binary}: {e}",
                path = scie_jump_path.display(),
                binary = binary_path.display()
            )
        })?
        .take(scie_jump_size as u64);
    std::io::copy(&mut scie_jump, &mut binary).map_err(|e| {
        format!(
            "Failed to write first {scie_jump_size} bytes of the scie-jump binary {path} to \
            {binary}: {e}",
            path = scie_jump_path.display(),
            binary = binary_path.display()
        )
    })?;
    let resolve_base = manifest_path.parent().unwrap_or_else(|| Path::new(""));
    for file in &lift.files {
        let path = match file {
            File::Archive(archive) => resolve_base.join(&archive.name),
            File::Blob(blob) => resolve_base.join(&blob.name),
        };
        let mut blob = std::fs::File::open(&path).map_err(|e| {
            format!(
                "Failed to open {src} / {file:?} for writing to {binary}: {e}",
                src = path.display(),
                binary = binary_path.display()
            )
        })?;
        std::io::copy(&mut blob, &mut binary).map_err(|e| {
            format!(
                "Failed to append {src} / {file:?} to {binary}: {e}",
                src = path.display(),
                binary = binary_path.display()
            )
        })?;
    }
    let config = Config {
        scie: Scie {
            jump: Some(Jump {
                size: scie_jump_size,
                version: "".to_string(), // TODO(John Sirois): XXX
                bare: false,
            }),
            lift,
        }
        .into(),
    };
    config.serialize(binary, !single_line).map_err(|e| {
        format!(
            "Failed to serialize the lift manifest to {binary}: {e}",
            binary = binary_path.display()
        )
    })?;
    finalize_executable(&binary_path)
}

pub(crate) fn set(jump: Jump, scie_jump_path: PathBuf) -> ExitResult {
    let mut lifts = vec![];
    let mut single_line = true;
    for arg in env::args().skip(1) {
        match arg.as_str() {
            "-1" | "--single-lift-line" => single_line = true,
            "--no-single-lift-line" => single_line = false,
            _ => {
                let (lift, path) = load_manifest(Path::new(arg.as_str()), &jump)
                    .map_err(|e| Code::FAILURE.with_message(e))?;
                lifts.push((lift, path));
            }
        }
    }
    if lifts.is_empty() {
        if let Ok(cwd) = env::current_dir() {
            let (lift, path) =
                load_manifest(&cwd, &jump).map_err(|e| Code::FAILURE.with_message(e))?;
            lifts.push((lift, path));
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
        .map(|(lift, manifest)| {
            pack(lift, &manifest, &scie_jump_path, jump.size, single_line)
                .map(|binary| (manifest, binary))
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Code::FAILURE.with_message(e))?;
    for (manifest, binary) in results {
        println!(
            "{manifest}: {binary}",
            manifest = manifest.display(),
            binary = binary.display()
        );
    }
    Code::SUCCESS.ok()
}

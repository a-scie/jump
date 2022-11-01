use std::collections::HashMap;
use std::env;
use std::io::Read;
use std::path::{Path, PathBuf};

use jump::config::{Config, File, Locator};
use jump::{fingerprint, Jump};
use logging_timer::time;
use proc_exit::{Code, ExitResult};

#[time("debug")]
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
    let mut config = Config::from_file(&manifest_path)?;
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

#[time("debug")]
fn index_files(config: &Config) -> Result<Vec<(PathBuf, &File)>, String> {
    let dir = config.scie.path.parent().ok_or_else(|| {
        format!(
            "Failed to determine directory for lift manifest {file}",
            file = config.scie.path.display()
        )
    })?;
    let mut index = HashMap::new();
    for entry in (std::fs::read_dir(dir).map_err(|e| {
        format!(
            "Failed to list the contents of {dir} for indexing: {e}",
            dir = dir.display()
        )
    })?)
    .flatten()
    {
        if let Ok(file_type) = entry.file_type() {
            if file_type.is_file() {
                let path = entry.path();
                let hash = fingerprint::digest_file(&path)?;
                let size = entry
                    .metadata()
                    .map_err(|e| {
                        format!(
                            "Failed to determine file size for {path}: {e}",
                            path = path.display()
                        )
                    })?
                    .len();
                index.insert(hash, (path, size));
            }
        }
    }
    let mut files = vec![];
    for file in config.scie.lift.files.iter() {
        let (hash, locator) = match file {
            File::Archive(archive) => (&archive.hash, &archive.locator),
            File::Blob(blob) => (&blob.hash, &blob.locator),
        };
        let (path, size) = index.get(hash).ok_or_else(|| {
            format!(
                "Found no files in {dir} with hash {hash}.",
                dir = dir.display()
            )
        })?;
        if let Locator::Size(expected_size) = locator {
            let actual_size = usize::try_from(*size).map_err(|e| {
                format!(
                    "Failed to convert actual file size of {path} to compare to the expected file \
                    size of {file:?}: {e}",
                    path = path.display()
                )
            })?;
            if actual_size != *expected_size {
                return Err(format!(
                    "Found a {size} byte file matching hash {hash} but the expected size was a \
                        mismatch for entry {file:?}"
                ));
            }
        }
        files.push((path.to_path_buf(), file))
    }
    Ok(files)
}

#[cfg(target_family = "windows")]
fn finalize_executable(path: &Path) -> Result<(), String> {
    if path.extension().is_none() {
        let exe = path.with_extension("exe");
        std::fs::rename(path, &exe).map_err(|e| {
            format!(
                "Failed to rename executable from {path} to {exe}: {e}",
                path = path.display(),
                exe = exe.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(not(target_family = "windows"))]
fn finalize_executable(path: &Path) -> Result<(), String> {
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
    })
}

#[time("debug")]
fn pack(
    config: &Config,
    scie_jump_path: &PathBuf,
    scie_jump_size: usize,
    single_line: bool,
) -> Result<PathBuf, String> {
    let index = index_files(config)?;
    let binary_path = env::current_dir()
        .map(|cwd| cwd.join(&config.scie.lift.name))
        .map_err(|e| format!("Failed to determine the output directory for scies: {e}"))?;
    let mut binary = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&binary_path)
        .map_err(|e| {
            format!(
                "Failed to open binary {path} for writing {scie:?}: {e}",
                path = binary_path.display(),
                scie = config.scie
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
    for (path, file) in index {
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
    config.serialize(binary, !single_line).map_err(|e| {
        format!(
            "Failed to serialize the lift manifest to {binary}: {e}",
            binary = binary_path.display()
        )
    })?;
    finalize_executable(&binary_path)?;
    Ok(binary_path)
}

pub(crate) fn set(jump: Jump, scie_jump_path: PathBuf) -> ExitResult {
    let mut lifts = vec![];
    let mut single_line = true;
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
        .map(|config| {
            pack(&config, &scie_jump_path, jump.size, single_line).map(|path| (config, path))
        })
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

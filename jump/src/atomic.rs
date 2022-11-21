// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::fmt::{Display, Formatter};
use std::fs::File;
use std::path::Path;

use serde::Serializer;

#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) enum Target {
    Directory,
    File,
}

impl Display for Target {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Target::Directory => f.serialize_str("directory"),
            Target::File => f.serialize_str("file"),
        }
    }
}

impl Target {
    fn check_exists(&self, target: &Path) -> Result<bool, String> {
        match self {
            Target::Directory => {
                if target.is_dir() {
                    return Ok(true);
                } else if !target.exists() {
                    return Ok(false);
                }
            }
            Target::File => {
                if target.is_file() {
                    return Ok(true);
                } else if !target.exists() {
                    return Ok(false);
                }
            }
        }
        Err(format!(
            "The target path {target} exists but is not a {self}.",
            target = target.display()
        ))
    }
}

fn clean(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    if path.is_dir() {
        std::fs::remove_dir(path)
    } else {
        std::fs::remove_file(path)
    }
    .map_err(|e| format!("Failed to remove path {path}: {e}", path = path.display()))
}

/// Executes work to create the `target` path exactly once across threads and processes.
///
/// If the `target_type` is `Target::Directory` and the `target` directory has not yet been created,
/// then `work` is handed an empty work directory to populate. Upon success that directory will be
/// renamed atomically to the `target` directory path. If the `target_type` is `Target::File` and
/// the `target` file has not been created, then `work` is handed the path of a work file to create.
/// That work file will not exist, but its parent directories will have been already created.
pub(crate) fn atomic_path<E: Display, T, F>(
    target: &Path,
    target_type: Target,
    work: F,
) -> Result<Option<T>, String>
where
    F: FnOnce(&Path) -> Result<T, E>,
{
    // We use an atomic rename under a double-checked exclusive write lock to implement an atomic
    // path creation.

    // First check.
    if target_type.check_exists(target)? {
        debug!(
            "The atomic {target_type} at {path} has already been established.",
            path = target.display()
        );
        return Ok(None);
    }

    // Lock.
    if !target.is_absolute() {
        return Err(format!(
            "The target_dir must be an absolute path, given: {}",
            target.display()
        ));
    }
    let (work_path, lock_file) = {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to establish parent directory of {target}: {e}",
                    target = target.display()
                )
            })?;
        }
        let lock_file = target.with_extension("lck");
        let work_dir = target.with_extension("work");
        (work_dir, lock_file)
    };

    let lock_fd = File::create(&lock_file).map_err(|e| {
        format!(
            "Failed to open lock file {lock_file}: {e}",
            lock_file = lock_file.display()
        )
    })?;
    let mut lock = fd_lock::RwLock::new(lock_fd);
    let _write_lock = lock.write();

    // Second check.
    if target_type.check_exists(target)? {
        debug!(
            "The atomic {target_type} at {path} has already been established \
            (lost double-check race).",
            path = target.display()
        );
        return Ok(None);
    }

    // Act.

    // N.B.: Prior work may have been terminated in ways outside the Rust runtime control (panic
    // handling not installed, signals of various sorts); so, with the lock in hand, we clean up any
    // stray work path before proceeding. Since we need to do this up front anyway, we do not attach
    // cleanup to the work error path.
    clean(&work_path)?;

    if Target::Directory == target_type {
        std::fs::create_dir(&work_path).map_err(|e| {
            format!(
                "Failed to prepare workdir {work_dir}: {e}",
                work_dir = work_path.display()
            )
        })?
    }

    let result = work(&work_path).map_err(|e| {
        format!(
            "Failed to establish atomic directory {target_dir}. Population of work directory \
            failed: {e}",
            target_dir = target.display()
        )
    })?;
    std::fs::rename(work_path, target).map_err(|e| {
        format!(
            "Failed to establish atomic directory {target_dir}. Rename of work directory \
            failed: {e}",
            target_dir = target.display()
        )
    })?;
    Ok(Some(result))
}

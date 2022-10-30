use std::fmt::Display;
use std::fs::File;
use std::path::Path;

pub(crate) fn atomic_directory<E: Display, F>(target_dir: &Path, work: F) -> Result<(), String>
where
    F: FnOnce(&Path) -> Result<(), E>,
{
    if !target_dir.is_absolute() {
        return Err(format!(
            "The target_dir must be an absolute path, given: {}",
            target_dir.display()
        ));
    }
    let (work_dir, lock_file) = {
        if let Some(parent) = target_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to establish parent directory if {target_dir}: {e}",
                    target_dir = target_dir.display()
                )
            })?;
        }
        let lock_file = target_dir.with_extension("lck");
        let work_dir = target_dir.with_extension("work");
        (work_dir, lock_file)
    };

    // We use an atomic rename under a double-checked exclusive write lock to implement an atomic
    // directory creation.
    if work_dir.exists() {
        return Ok(());
    }

    let lock_fd = File::create(&lock_file).map_err(|e| {
        format!(
            "Failed to open lock file {lock_file}: {e}",
            lock_file = lock_file.display()
        )
    })?;
    let mut lock = fd_lock::RwLock::new(lock_fd);
    let _write_lock = lock.write();
    if work_dir.exists() {
        return Ok(());
    }
    work(&work_dir).map_err(|e| {
        format!(
            "Failed to establish atomic directory {target_dir}. Population of work directory \
            failed: {e}",
            target_dir = target_dir.display()
        )
    })?;
    std::fs::rename(work_dir, target_dir).map_err(|e| {
        format!(
            "Failed to establish atomic directory {target_dir}. Rename of work directory \
            failed: {e}",
            target_dir = target_dir.display()
        )
    })
}

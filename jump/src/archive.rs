use std::fs::Metadata;
use std::path::{Path, PathBuf};

use log::debug;
use logging_timer::time;
use walkdir::WalkDir;
use zip::write::FileOptions;

#[cfg(not(target_family = "unix"))]
pub fn create_options(_metadata: &Metadata) -> Result<FileOptions, String> {
    Ok(FileOptions::default())
}

#[cfg(target_family = "unix")]
pub fn create_options(metadata: &Metadata) -> Result<FileOptions, String> {
    use std::os::unix::fs::PermissionsExt;
    let perms = metadata.permissions();
    Ok(FileOptions::default().unix_permissions(perms.mode()))
}

fn create_zip(dir: &Path) -> Result<PathBuf, String> {
    let zip_path = dir.with_extension("zip");
    let mut zip = zip::ZipWriter::new(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&zip_path)
            .map_err(|e| {
                format!(
                    "Failed to open {zip} for packing {dir} into: {e}",
                    zip = zip_path.display(),
                    dir = dir.display()
                )
            })?,
    );
    for entry in WalkDir::new(dir).contents_first(false).follow_links(true) {
        let entry = entry.map_err(|e| {
            format!(
                "Walk failed while trying to create a zip of {dir}: {e}",
                dir = dir.display()
            )
        })?;
        if entry.path() == dir {
            continue;
        }
        let rel_path = entry
            .path()
            .strip_prefix(dir)
            .map_err(|e| format!("Failed to relativize archive path: {e}"))?;
        let entry_name = rel_path
            .iter()
            .map(|component| {
                component.to_str().ok_or_else(|| {
                    format!("Failed to interpreter relative path component as utf8: {component:?}")
                })
            })
            .collect::<Result<Vec<_>, _>>()?
            // N.B.: Zip archive entry names always use / as the directory separator.
            .join("/");
        let options = create_options(&entry.metadata().map_err(|e| {
            format!(
                "Failed to read metadata for {path}: {e}",
                path = entry.path().display()
            )
        })?)?;
        if entry.path().is_dir() {
            debug!("Adding dir entry {entry}", entry = rel_path.display());
            zip.add_directory(entry_name, options)
                .map_err(|e| format!("{e}"))?;
        } else {
            zip.start_file(entry_name, options)
                .map_err(|e| format!("{e}"))?;
            if entry.path_is_symlink() {
                debug!("Resolved symlink {entry}", entry = rel_path.display());
            };
            debug!("Adding file entry {entry}", entry = rel_path.display());
            let mut file = std::fs::File::open(entry.path()).map_err(|e| format!("{e}"))?;
            std::io::copy(&mut file, &mut zip).map_err(|e| format!("{e}"))?;
        }
    }
    zip.finish().map_err(|e| {
        format!(
            "Failed to finalize zip {zip}: {e}",
            zip = zip_path.display()
        )
    })?;
    Ok(zip_path)
}

#[time("debug")]
pub(crate) fn create(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let path = dir.join(name);
    let directory = path.canonicalize().map_err(|e| {
        format!(
            "Cannot create a zip archive from {path}: Directory does not exist: {e}",
            path = path.display()
        )
    })?;
    if !directory.is_dir() {
        return Err(format!(
            "Cannot create a zip archive from {name}: {directory} is a file.",
            directory = directory.display()
        ));
    }
    create_zip(&directory)
}

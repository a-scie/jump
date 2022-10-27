use bstr::ByteSlice;
use itertools::Itertools;
use lazy_static::lazy_static;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::io::Cursor;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use regex::{Captures, Regex, Replacer};

use crate::config::{Archive, ArchiveType, Blob, Cmd, Compression, Config, File, Locator, Scie};

fn expanduser(path: &Path) -> Result<PathBuf, String> {
    if !<[u8]>::from_path(path)
        .ok_or_else(|| {
            format!(
                "Failed to decode the path {} as utf-8 bytes",
                path.display()
            )
        })?
        .contains(&b'~')
    {
        return Ok(path.to_owned());
    }

    let home_dir = dirs::home_dir()
        .ok_or_else(|| format!("Failed to expand home dir in path {}", path.display()))?;
    let mut components = Vec::new();
    for path_component in path.components() {
        match path_component {
            Component::Normal(component) if OsStr::new("~") == component => {
                for home_dir_component in home_dir.components() {
                    components.push(home_dir_component)
                }
            }
            component => components.push(component),
        }
    }
    Ok(components.into_iter().collect())
}

lazy_static! {
    static ref PARSER: Regex = Regex::new(r"\$\{(?P<name>[^}]+)}")
        .map_err(|e| format!("Invalid regex for replacing ${{name}}: {}", e))
        .unwrap();
}

struct FileIndex {
    root: PathBuf,
    files_by_name: HashMap<String, File>,
    replacements: HashSet<File>,
    errors: HashSet<String>,
}

impl FileIndex {
    fn new(scie: &Scie, files: &[File]) -> Result<Self, String> {
        let mut files_by_name = HashMap::new();
        for file in files {
            match file {
                File::Archive(Archive {
                    name: Some(name), ..
                }) => {
                    files_by_name.insert(name.clone(), file.to_owned());
                }
                File::Blob(blob) => {
                    files_by_name.insert(blob.name.clone(), file.to_owned());
                }
                _ => (),
            }
        }
        Ok(FileIndex {
            root: expanduser(&scie.root)?,
            files_by_name,
            replacements: HashSet::new(),
            errors: HashSet::new(),
        })
    }

    fn get_file(&self, name: &str) -> Option<&File> {
        self.files_by_name.get(name)
    }

    fn get_path(&self, file: &File) -> PathBuf {
        match file {
            File::Archive(archive) => self.root.join(&archive.fingerprint.hash),
            File::Blob(blob) => self.root.join(&blob.fingerprint.hash).join(&blob.name),
        }
    }

    fn reify_string(&mut self, value: String) -> String {
        String::from(PARSER.replace_all(value.as_str(), self.by_ref()))
    }
}

impl Replacer for FileIndex {
    fn replace_append(&mut self, caps: &Captures<'_>, dst: &mut String) {
        let name = caps.name("name").unwrap().as_str();

        if let Some(file) = self.files_by_name.get(name) {
            let path = self.get_path(file);
            match <[u8]>::from_path(&path) {
                Some(path) => match std::str::from_utf8(path) {
                    Ok(path) => {
                        dst.push_str(path);
                        self.replacements.insert(file.clone());
                    }
                    Err(err) => {
                        self.errors.insert(format!(
                            "Failed to convert file {} to path. Not UTF-8: {:?}: {}",
                            name, path, err
                        ));
                    }
                },
                None => {
                    self.errors
                        .insert(format!("Failed to convert file {} to a path.", name));
                }
            }
        } else {
            dst.push_str(name);
            self.errors.insert(name.to_string());
        }
    }
}

pub fn extract(data: &[u8], mut config: Config) -> Result<Cmd, String> {
    let command =
        match std::env::var_os("SCIE_CMD") {
            Some(cmd) => {
                let name = cmd.into_string().map_err(|e| {
                    format!("Failed to decode environment variable SCIE_CMD: {:?}", e)
                })?;
                config.additional_commands.remove(&name).ok_or_else(|| {
                    format!(
                    "The custom command specified by SCIE_CMD={} is not a configured command in \
                    this binary. The following named commands are available: {}",
                    name, config.additional_commands.keys().join(", "))
                })?
            }
            None => config.command,
        };

    let mut file_index = FileIndex::new(&config.scie, &config.files)?;
    let mut to_extract = HashSet::new();
    for name in &command.additional_files {
        let file = file_index.get_file(name.as_str()).ok_or_else(|| {
            format!(
                "The additional file {} requested by {:#?} was not found in this executable.",
                name, command
            )
        })?;
        to_extract.insert(file.clone());
    }

    let prepared_cmd = Cmd {
        exe: file_index.reify_string(command.exe),
        args: command
            .args
            .into_iter()
            .map(|string| file_index.reify_string(string))
            .collect::<Vec<_>>(),
        env: command
            .env
            .into_iter()
            .map(|(key, value)| (key, file_index.reify_string(value)))
            .collect::<HashMap<_, _>>(),
        additional_files: command.additional_files,
    };
    if !file_index.errors.is_empty() {
        return Err(format!(
            "Failed to find the following named files in this executable: {}",
            file_index.errors.iter().join(", ")
        ));
    }
    eprintln!("Prepared command:\n{:#?}", prepared_cmd);

    for file in &file_index.replacements {
        to_extract.insert(file.clone());
    }
    eprintln!("To extract:\n{:#?}", to_extract);

    // TODO(John Sirois): XXX: Extract!
    // 1. rip through files in order -> if to_extract and Size extract and bump location.
    // 2. if still to_extract, open final slice as zip -> rip through files in order -> if to_extract and Entry extract from zip.
    let mut sized = vec![];
    let mut entries = vec![];
    if !to_extract.is_empty() {
        for file in config.files.iter().filter(|f| to_extract.contains(f)) {
            let dst = file_index.get_path(file);
            match file {
                File::Archive(Archive {
                    locator: Locator::Size(size),
                    fingerprint,
                    archive_type,
                    ..
                }) if !dst.is_dir() => sized.push((size, fingerprint, dst, Some(archive_type))),
                File::Blob(Blob {
                    locator: Locator::Size(size),
                    fingerprint,
                    ..
                }) if !dst.is_file() => sized.push((size, fingerprint, dst, None)),
                File::Archive(Archive {
                    locator: Locator::Entry(path),
                    fingerprint,
                    archive_type,
                    ..
                }) if !dst.is_dir() => entries.push((path, fingerprint, dst, Some(archive_type))),
                File::Blob(Blob {
                    locator: Locator::Entry(path),
                    fingerprint,
                    ..
                }) if !dst.is_file() => entries.push((path, fingerprint, dst, None)),
                _ => eprintln!("Already extracted: {}", dst.display()),
            };
        }
    }

    // TODO(John Sirois): XXX: AtomicDirectory
    let mut step = 1;
    let mut location = config.scie.size;
    for (size, fingerprint, dst, archive_type) in sized {
        eprintln!(
            "Step {}: extract {} bytes with fingerprint {:?} starting at {} to {} of archive type {:?}",
            step, size, fingerprint, location, dst.display(), archive_type
        );
        let bytes = &data[location..(location + size)];
        // TODO(John Sirois): XXX: Use fingerprint - insert hasher in stream stack to compare against.
        match archive_type {
            None => {
                let parent_dir = dst.parent().ok_or_else(|| "".to_owned())?;
                std::fs::create_dir_all(parent_dir).map_err(|e| format!("{}", e))?;
                let mut out = std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(dst)
                    .map_err(|e| format!("{}", e))?;
                out.write_all(bytes).map_err(|e| format!("{}", e))?;
            }
            Some(archive) => {
                std::fs::create_dir_all(&dst).map_err(|e| format!("{}", e))?;
                match archive {
                    ArchiveType::Zip => {
                        let seekable_bytes = Cursor::new(bytes);
                        let mut zip =
                            zip::ZipArchive::new(seekable_bytes).map_err(|e| format!("{}", e))?;
                        zip.extract(dst).map_err(|e| format!("{}", e))?;
                    }
                    ArchiveType::Tar => {
                        let mut tar = tar::Archive::new(bytes);
                        tar.unpack(dst).map_err(|e| format!("{}", e))?;
                    }
                    ArchiveType::CompressedTar(Compression::Bzip2) => {
                        let bzip2_decoder = bzip2::read::BzDecoder::new(bytes);
                        let mut tar = tar::Archive::new(bzip2_decoder);
                        tar.unpack(dst).map_err(|e| format!("{}", e))?;
                    }
                    ArchiveType::CompressedTar(Compression::Gzip) => {
                        let gz_decoder = flate2::read::GzDecoder::new(bytes);
                        let mut tar = tar::Archive::new(gz_decoder);
                        tar.unpack(dst).map_err(|e| format!("{}", e))?;
                    }
                    ArchiveType::CompressedTar(Compression::Xz) => {
                        let xz_decoder = xz2::read::XzDecoder::new(bytes);
                        let mut tar = tar::Archive::new(xz_decoder);
                        tar.unpack(dst).map_err(|e| format!("{}", e))?;
                    }
                    ArchiveType::CompressedTar(Compression::Zlib) => {
                        let zlib_decoder = flate2::read::ZlibDecoder::new(bytes);
                        let mut tar = tar::Archive::new(zlib_decoder);
                        tar.unpack(dst).map_err(|e| format!("{}", e))?;
                    }
                    ArchiveType::CompressedTar(Compression::Zstd) => {
                        let zstd_decoder =
                            zstd::stream::Decoder::new(bytes).map_err(|e| format!("{}", e))?;
                        let mut tar = tar::Archive::new(zstd_decoder);
                        tar.unpack(dst).map_err(|e| format!("{}", e))?;
                    }
                }
            }
        }
        step += 1;
        location += size;
    }

    if !entries.is_empty() {
        let seekable_bytes = Cursor::new(&data[location..(data.len() - config.size)]);
        let mut zip = zip::ZipArchive::new(seekable_bytes).map_err(|e| format!("{}", e))?;
        for (path, fingerprint, dst, archive_type) in entries {
            eprintln!(
                "Step {}: extract {} with fingerprint {:?} from trailer zip at {} to {} with archive type {:?}",
                step,
                path.display(),
                fingerprint,
                location,
                dst.display(),
                archive_type
            );
            step += 1;

            std::fs::create_dir_all(&dst).map_err(|e| format!("{}", e))?;
            zip.extract(dst).map_err(|e| format!("{}", e))?;
        }
    }

    Ok(prepared_cmd)
}

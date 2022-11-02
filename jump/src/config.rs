use std::collections::HashMap;
use std::fmt::Formatter;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::de::{Error, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::fingerprint;

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Locator {
    Size(usize),
    Entry(PathBuf),
}

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    Bzip2,
    Gzip,
    Xz,
    Zlib,
    Zstd,
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub enum ArchiveType {
    Zip,
    Tar,
    CompressedTar(Compression),
}

impl Serialize for ArchiveType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            ArchiveType::Zip => serializer.serialize_str("zip"),
            ArchiveType::Tar => serializer.serialize_str("tar"),
            ArchiveType::CompressedTar(Compression::Bzip2) => serializer.serialize_str("tar.bz2"),
            ArchiveType::CompressedTar(Compression::Gzip) => serializer.serialize_str("tar.gz"),
            ArchiveType::CompressedTar(Compression::Xz) => serializer.serialize_str("tar.xz"),
            ArchiveType::CompressedTar(Compression::Zlib) => serializer.serialize_str("tar.Z"),
            ArchiveType::CompressedTar(Compression::Zstd) => serializer.serialize_str("tar.zst"),
        }
    }
}

struct ArchiveTypeVisitor;

impl<'de> Visitor<'de> for ArchiveTypeVisitor {
    type Value = ArchiveType;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        write!(
            formatter,
            "one of: zip, tar, tar.bz2, tbz2, tar.gz, tgz, tar.xz, tar.lzma, tlz, tar.Z, \
            tar.zst or tzst"
        )
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        // These values are derived from the `-a` extensions described by GNU tar here:
        // https://www.gnu.org/software/tar/manual/html_node/gzip.html#gzip
        match value {
            "zip" => Ok(ArchiveType::Zip),
            "tar" => Ok(ArchiveType::Tar),
            "tar.bz2" | "tbz2" => Ok(ArchiveType::CompressedTar(Compression::Bzip2)),
            "tar.gz" | "tgz" => Ok(ArchiveType::CompressedTar(Compression::Gzip)),
            "tar.xz" | "tar.lzma" | "tlz" => Ok(ArchiveType::CompressedTar(Compression::Xz)),
            "tar.Z" => Ok(ArchiveType::CompressedTar(Compression::Zlib)),
            "tar.zst" | "tzst" => Ok(ArchiveType::CompressedTar(Compression::Zstd)),
            _ => Err(E::invalid_value(Unexpected::Str(value), &self)),
        }
    }
}

impl<'de> Deserialize<'de> for ArchiveType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(ArchiveTypeVisitor)
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct Blob {
    #[serde(flatten)]
    pub locator: Locator,
    pub hash: String,
    pub name: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "is_false")]
    pub always_extract: bool,
}

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct Archive {
    #[serde(flatten)]
    pub locator: Locator,
    pub hash: String,
    pub archive_type: ArchiveType,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "is_false")]
    pub always_extract: bool,
}

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[serde(untagged)]
pub enum File {
    Archive(Archive),
    Blob(Blob),
}

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub enum EnvVar {
    Default(String),
    Replace(String),
}

impl Serialize for EnvVar {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            EnvVar::Default(name) => serializer.serialize_str(name),
            EnvVar::Replace(name) => serializer.serialize_str(format!("={name}").as_str()),
        }
    }
}

struct EnvVarVisitor;

impl<'de> Visitor<'de> for EnvVarVisitor {
    type Value = EnvVar;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        write!(
            formatter,
            "a valid environment variable name: \
            https://pubs.opengroup.org/onlinepubs/009696899/basedefs/xbd_chap08.html"
        )
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        // We use an = prefix to indicate the env var should replace any current env var since an =
        // prefix presents an obvious parsing challenge to OSes; so people likely avoid it and this
        // fact is encoded here:
        // https://pubs.opengroup.org/onlinepubs/009696899/basedefs/xbd_chap08.html
        match value.as_bytes() {
            [b'=', name @ ..] => {
                let env_var_name = std::str::from_utf8(name)
                    .map_err(|_| E::invalid_value(Unexpected::Str(value), &self))?;
                Ok(EnvVar::Replace(env_var_name.into()))
            }
            _ => Ok(EnvVar::Default(value.into())),
        }
    }
}

impl<'de> Deserialize<'de> for EnvVar {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(EnvVarVisitor)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Cmd {
    pub exe: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<EnvVar, String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub additional_files: Vec<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct Jump {
    pub size: usize,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "is_false")]
    pub bare: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Boot {
    pub commands: HashMap<String, Cmd>,
    #[serde(default)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub bindings: HashMap<String, Cmd>,
}

fn default_base() -> PathBuf {
    PathBuf::from("~/.nce")
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Lift {
    pub files: Vec<File>,
    pub boot: Boot,
    pub name: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_base")]
    pub base: PathBuf,
    #[serde(default)]
    pub size: usize,
    #[serde(default)]
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Scie {
    pub lift: Lift,
    #[serde(default)]
    pub jump: Option<Jump>,
    #[serde(default)]
    #[serde(skip_serializing)]
    pub path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub scie: Scie,
}

impl Config {
    pub fn parse(data: &[u8], origin: &Path) -> Result<Self, String> {
        let mut config: Self = serde_json::from_slice(data)
            .map_err(|e| format!("Failed to decode scie lift manifest: {e}"))?;
        config.scie.lift.size = data.len();
        config.scie.lift.hash = fingerprint::digest(data);
        config.scie.path = origin.to_path_buf();
        Ok(config)
    }

    pub fn from_file(file: &Path) -> Result<Self, String> {
        let data = std::fs::read(file).map_err(|e| {
            format!(
                "Failed to open lift manifest at {file}: {e}",
                file = file.display()
            )
        })?;
        Self::parse(data.as_slice(), file)
    }
    pub fn serialize<W: Write>(&self, mut stream: W, pretty: bool) -> Result<(), String> {
        stream
            .write_all(if cfg!(windows) { "\r\n" } else { "\n" }.as_bytes())
            .map_err(|e| format!("Failed to write scie lift manifest: {e}"))?;
        if pretty {
            serde_json::to_writer_pretty(stream, self)
                .map_err(|e| format!("Failed to write scie lift manifest: {e}"))
        } else {
            serde_json::to_writer(stream, self)
                .map_err(|e| format!("Failed to write scie lift manifest: {e}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{
        Archive, ArchiveType, Blob, Boot, Cmd, Compression, Config, EnvVar, File, Jump, Lift,
        Locator, Scie,
    };

    #[test]
    fn test_serialized_form() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&Config {
                scie: Scie {
                    path: "/usr/bin/science".into(),
                    jump: Some(Jump {
                        version: "0.1.0".to_string(),
                        size: 37,
                        bare: false
                    }),
                    lift: Lift {
                        base: "~/.nce".into(),
                        files: vec![
                            File::Blob(Blob {
                                locator: Locator::Size(1137),
                                hash: "abc".to_string(),
                                name: "pants-client".to_string(),
                                always_extract: true
                            }),
                            File::Archive(Archive {
                                locator: Locator::Size(123),
                                hash: "345".to_string(),
                                archive_type: ArchiveType::CompressedTar(Compression::Zstd),
                                name: Some("python".to_string()),
                                always_extract: false
                            }),
                            File::Archive(Archive {
                                locator: Locator::Size(42),
                                hash: "def".to_string(),
                                archive_type: ArchiveType::Zip,
                                name: None,
                                always_extract: false
                            })
                        ],
                        boot: Boot {
                            commands: vec![(
                                "".to_string(),
                                Cmd {
                                    exe: "bob/exe".to_string(),
                                    args: Default::default(),
                                    env: [
                                        (
                                            EnvVar::Default("DEFAULT".to_string()),
                                            "default".to_string()
                                        ),
                                        (
                                            EnvVar::Replace("REPLACE".to_string()),
                                            "replace".to_string()
                                        )
                                    ]
                                    .into_iter()
                                    .collect(),
                                    additional_files: Default::default(),
                                    description: None
                                }
                            )]
                            .into_iter()
                            .collect::<HashMap<_, _>>(),
                            bindings: Default::default()
                        },
                        size: 37,
                        hash: "XYZ".to_string(),
                        name: "test".to_string(),
                        description: None
                    }
                },
            })
            .unwrap()
        )
    }

    #[test]
    fn test_deserialize_defaults() {
        eprintln!(
            "{:#?}",
            serde_json::from_str::<Config>(
                r#"
                {
                    "scie": {
                        "lift": {
                            "name": "example",
                            "files": [
                                {
                                    "type": "blob",
                                    "name": "pants-client",
                                    "size": 1,
                                    "hash": "789"
                                },
                                {
                                    "type": "archive",
                                    "size": 1137,
                                    "hash": "abc",
                                    "archive_type": "tar.gz"
                                },
                                {
                                    "type": "archive",
                                    "name": "app",
                                    "size": 42,
                                    "hash": "xyz",
                                    "archive_type": "zip"
                                }
                            ],
                            "boot": {
                                "commands": {
                                    "": {
                                        "env": {
                                            "PEX_VERBOSE": "1",
                                            "=PATH": ".:${scie.env.PATH}"
                                        },
                                        "exe":"{python}/bin/python",
                                        "args": [
                                            "{app}"
                                        ]
                                    }
                                }
                            }
                        }
                    }
                }
                "#
            )
            .unwrap()
        )
    }
}

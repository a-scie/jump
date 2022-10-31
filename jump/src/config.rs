use std::collections::HashMap;
use std::fmt::Formatter;
use std::path::PathBuf;

use serde::de::{Error, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Locator {
    Size(usize),
    Entry(PathBuf),
}

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Compression {
    Bzip2,
    Gzip,
    Xz,
    Zlib,
    Zstd,
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(crate) enum ArchiveType {
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

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Blob {
    #[serde(flatten)]
    pub(crate) locator: Locator,
    pub(crate) hash: String,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) always_extract: bool,
}

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Archive {
    #[serde(flatten)]
    pub(crate) locator: Locator,
    pub(crate) hash: String,
    pub(crate) archive_type: ArchiveType,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) always_extract: bool,
}

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type")]
pub(crate) enum File {
    Archive(Archive),
    Blob(Blob),
}

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub(crate) enum EnvVar {
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
pub(crate) struct Cmd {
    pub(crate) exe: String,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    #[serde(default)]
    pub(crate) env: HashMap<EnvVar, String>,
    #[serde(default)]
    pub(crate) additional_files: Vec<String>,
    #[serde(default)]
    pub(crate) description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Jump {
    pub size: usize,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Boot {
    pub(crate) commands: HashMap<String, Cmd>,
    #[serde(default)]
    pub(crate) bindings: HashMap<String, Cmd>,
}

fn default_base() -> PathBuf {
    PathBuf::from("~/.nce")
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Lift {
    pub(crate) files: Vec<File>,
    pub(crate) boot: Boot,
    #[serde(default = "default_base")]
    pub(crate) base: PathBuf,
    #[serde(default)]
    pub(crate) size: usize,
    #[serde(default)]
    pub(crate) hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Scie {
    pub(crate) lift: Lift,
    #[serde(default)]
    pub(crate) jump: Option<Jump>,
    #[serde(default)]
    pub(crate) path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Config {
    pub(crate) scie: Scie,
}

impl Config {
    pub(crate) fn parse(data: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(data).map_err(|e| format!("Failed to decode scie jmp config: {e}"))
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
                        hash: "XYZ".to_string()
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

use std::collections::HashMap;
use std::fmt::Formatter;
use std::path::PathBuf;

use serde::de::{Error, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HashAlgorithm {
    Sha256,
}

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct Fingerprint {
    pub algorithm: HashAlgorithm,
    pub hash: String,
}

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

#[derive(Debug, Serialize, Deserialize)]
pub struct Scie {
    pub version: String,
    pub root: PathBuf,
    pub size: usize,
}

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct Blob {
    #[serde(flatten)]
    pub locator: Locator,
    pub fingerprint: Fingerprint,
    pub name: String,
    #[serde(default)]
    pub always_extract: bool,
}

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct Archive {
    #[serde(flatten)]
    pub locator: Locator,
    pub fingerprint: Fingerprint,
    pub archive_type: ArchiveType,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub always_extract: bool,
}

#[derive(Hash, Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type")]
pub enum File {
    Archive(Archive),
    Blob(Blob),
}

#[derive(Hash, Eq, PartialEq, Debug)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct Cmd {
    pub exe: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<EnvVar, String>,
    #[serde(default)]
    pub additional_files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub scie: Scie,
    pub files: Vec<File>,
    pub command: Cmd,
    #[serde(default)]
    pub additional_commands: HashMap<String, Cmd>,
    #[serde(default)]
    pub size: usize,
}

#[cfg(test)]
mod tests {
    use super::{
        Archive, ArchiveType, Blob, Cmd, Compression, Config, File, Fingerprint, HashAlgorithm,
        Locator, Scie,
    };
    use crate::config::EnvVar;

    #[test]
    fn test_serialized_form() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&Config {
                scie: Scie {
                    version: "0.1.0".to_string(),
                    size: 37,
                    root: "~/.nce".into(),
                },
                files: vec![
                    File::Blob(Blob {
                        locator: Locator::Size(1137),
                        fingerprint: Fingerprint {
                            algorithm: HashAlgorithm::Sha256,
                            hash: "abc".into()
                        },
                        name: "pants-client".into(),
                        always_extract: true
                    }),
                    File::Archive(Archive {
                        locator: Locator::Size(123),
                        fingerprint: Fingerprint {
                            algorithm: HashAlgorithm::Sha256,
                            hash: "345".into()
                        },
                        archive_type: ArchiveType::CompressedTar(Compression::Zstd),
                        name: Some("python".into()),
                        always_extract: false
                    }),
                    File::Archive(Archive {
                        locator: Locator::Size(42),
                        fingerprint: Fingerprint {
                            algorithm: HashAlgorithm::Sha256,
                            hash: "def".into()
                        },
                        archive_type: ArchiveType::Zip,
                        name: None,
                        always_extract: false
                    })
                ],
                command: Cmd {
                    exe: "bob/exe".into(),
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
                    additional_files: Default::default()
                },
                additional_commands: Default::default(),
                size: 37
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
                "version": "0.1.0",
                "size": 37,
                "root": "~/.nce"
              },
              "files": [
                {
                  "type": "blob",
                  "name": "pants-client",
                  "size": 1,
                  "fingerprint": {
                    "algorithm": "sha256",
                    "hash": "789"
                  }
                },
                {
                  "type": "archive",
                  "size": 1137,
                  "fingerprint": {
                    "algorithm": "sha256",
                    "hash": "abc"
                  },
                  "archive_type": "tar.gz"
                },
                {
                  "type": "archive",
                  "name": "app",
                  "size": 42,
                  "fingerprint": {
                    "algorithm": "sha256",
                    "hash": "xyz"
                  },
                  "archive_type": "zip"
                }
              ],
              "command": {
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
        "#
            )
            .unwrap()
        )
    }
}

// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::fmt::Formatter;
use std::io::Write;

use indexmap::IndexMap;
use serde::de::{Error, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum Compression {
    Bzip2,
    Gzip,
    Xz,
    Zlib,
    Zstd,
}

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum ArchiveType {
    CompressedTar(Compression),
    Tar,
    Zip,
}

impl ArchiveType {
    pub(crate) fn from_ext(value: &str) -> Option<Self> {
        // These values are derived from the `-a` extensions described by GNU tar here:
        // https://www.gnu.org/software/tar/manual/html_node/gzip.html#gzip
        match value {
            "zip" => Some(ArchiveType::Zip),
            "tar" => Some(ArchiveType::Tar),
            "tar.bz2" | "tbz2" => Some(ArchiveType::CompressedTar(Compression::Bzip2)),
            "tar.gz" | "tgz" => Some(ArchiveType::CompressedTar(Compression::Gzip)),
            "tar.xz" | "tar.lzma" | "tlz" => Some(ArchiveType::CompressedTar(Compression::Xz)),
            "tar.Z" => Some(ArchiveType::CompressedTar(Compression::Zlib)),
            "tar.zst" | "tzst" => Some(ArchiveType::CompressedTar(Compression::Zstd)),
            _ => None,
        }
    }

    pub fn as_ext(&self) -> &str {
        match self {
            ArchiveType::Zip => "zip",
            ArchiveType::Tar => "tar",
            ArchiveType::CompressedTar(Compression::Bzip2) => "tar.bz2",
            ArchiveType::CompressedTar(Compression::Gzip) => "tar.gz",
            ArchiveType::CompressedTar(Compression::Xz) => "tar.xz",
            ArchiveType::CompressedTar(Compression::Zlib) => "tar.Z",
            ArchiveType::CompressedTar(Compression::Zstd) => "tar.zst",
        }
    }
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub enum FileType {
    Archive(ArchiveType),
    Blob,
    Directory,
}

impl Serialize for FileType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            FileType::Archive(archive_type) => serializer.serialize_str(archive_type.as_ext()),
            FileType::Blob => serializer.serialize_str("blob"),
            FileType::Directory => serializer.serialize_str("directory"),
        }
    }
}

struct FileTypeVisitor;

impl Visitor<'_> for FileTypeVisitor {
    type Value = FileType;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        write!(
            formatter,
            "one of: blob, directory, zip, tar, tar.bz2, tbz2, tar.gz, tgz, tar.xz, tar.lzma, \
            tlz, tar.Z, tar.zst or tzst"
        )
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        match value {
            "blob" => Ok(FileType::Blob),
            "directory" => Ok(FileType::Directory),
            _ => ArchiveType::from_ext(value)
                .map(FileType::Archive)
                .ok_or_else(|| E::invalid_value(Unexpected::Str(value), &self)),
        }
    }
}

impl<'de> Deserialize<'de> for FileType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(FileTypeVisitor)
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub name: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<usize>,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default, rename = "type")]
    pub file_type: Option<FileType>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable: Option<bool>,
    #[serde(default)]
    #[serde(skip_serializing_if = "is_false")]
    pub eager_extract: bool,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
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

impl Visitor<'_> for EnvVarVisitor {
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cmd {
    pub exe: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub env: IndexMap<EnvVar, Option<String>>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Jump {
    pub size: usize,
    #[serde(default)]
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boot {
    pub commands: IndexMap<String, Cmd>,
    #[serde(default)]
    #[serde(skip_serializing_if = "IndexMap::is_empty")]
    pub bindings: IndexMap<String, Cmd>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lift {
    pub name: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    pub files: Vec<File>,
    pub boot: Boot,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_dotenv: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scie {
    pub lift: Lift,
    #[serde(default)]
    pub jump: Option<Jump>,
}

pub struct Fmt {
    pretty: bool,
    leading_newline: bool,
    trailing_newline: bool,
}

impl Fmt {
    pub fn new() -> Self {
        Fmt {
            pretty: false,
            leading_newline: false,
            trailing_newline: false,
        }
    }

    pub fn pretty(mut self, value: bool) -> Self {
        self.pretty = value;
        self
    }

    pub fn leading_newline(mut self, value: bool) -> Self {
        self.leading_newline = value;
        self
    }

    pub fn trailing_newline(mut self, value: bool) -> Self {
        self.trailing_newline = value;
        self
    }
}

impl Default for Fmt {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Other {
    #[serde(flatten)]
    other: IndexMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub scie: Scie,
    #[serde(flatten)]
    pub(crate) other: Option<Other>,
}

impl Config {
    pub const MAXIMUM_CONFIG_SIZE: usize = 0xFFFF;
    #[cfg(target_family = "windows")]
    const NEWLINE: &'static [u8] = b"\r\n";

    #[cfg(target_family = "unix")]
    const NEWLINE: &'static [u8] = b"\n";

    pub fn new<L: Into<Lift>>(jump: Jump, lift: L, other: Option<Other>) -> Self {
        Self {
            scie: Scie {
                jump: Some(jump),
                lift: lift.into(),
            },
            other,
        }
    }

    pub fn parse(data: &[u8]) -> Result<Self, String> {
        let config: Self = serde_json::from_slice(data)
            .map_err(|e| format!("Failed to decode scie lift manifest: {e}"))?;
        Ok(config)
    }

    pub fn serialize<W: Write>(&self, mut stream: W, fmt: Fmt) -> Result<(), String> {
        let mut write_bytes = |bytes| {
            stream
                .write_all(bytes)
                .map_err(|e| format!("Failed to write scie lift manifest: {e}"))
        };

        if fmt.leading_newline {
            write_bytes(Config::NEWLINE)?;
        }

        let body = if fmt.pretty {
            serde_json::to_vec_pretty(self)
        } else {
            serde_json::to_vec(self)
        }
        .map_err(|e| format!("Failed to serialize scie lift manifest: {e}"))?;
        write_bytes(body.as_slice())?;

        if fmt.trailing_newline {
            write_bytes(Config::NEWLINE)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::{ArchiveType, Boot, Cmd, Compression, Config, EnvVar, File, Jump, Lift};
    use crate::config::FileType;

    #[test]
    fn test_serialized_form() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&Config::new(
                Jump {
                    version: "0.1.0".to_string(),
                    size: 37,
                },
                Lift {
                    base: None,
                    files: vec![
                        File {
                            name: "pants-client".to_string(),
                            key: None,
                            size: Some(1137),
                            hash: Some("abc".to_string()),
                            file_type: Some(FileType::Blob),
                            executable: Some(true),
                            eager_extract: true,
                            source: None,
                        },
                        File {
                            name: "python".to_string(),
                            key: None,
                            size: Some(123),
                            hash: Some("345".to_string()),
                            file_type: Some(FileType::Archive(ArchiveType::CompressedTar(
                                Compression::Zstd
                            ))),
                            executable: None,
                            eager_extract: false,
                            source: None,
                        },
                        File {
                            name: "foo.zip".to_string(),
                            key: None,
                            size: Some(42),
                            hash: Some("def".to_string()),
                            file_type: Some(FileType::Archive(ArchiveType::Zip)),
                            executable: None,
                            eager_extract: false,
                            source: None,
                        }
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
                                        Some("default".to_string())
                                    ),
                                    (
                                        EnvVar::Replace("REPLACE".to_string()),
                                        Some("replace".to_string())
                                    ),
                                    (EnvVar::Default("PEX_.*".to_string()), None,),
                                    (EnvVar::Replace("PEX".to_string()), None,)
                                ]
                                .into_iter()
                                .collect(),
                                description: None
                            }
                        )]
                        .into_iter()
                        .collect::<IndexMap<_, _>>(),
                        bindings: Default::default()
                    },
                    name: "test".to_string(),
                    description: None,
                    load_dotenv: Some(false)
                },
                None,
            ))
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
                                    "name": "pants-client"
                                },
                                {
                                    "name": "foo.tar.gz"
                                },
                                {
                                    "name": "app.zip"
                                }
                            ],
                            "boot": {
                                "commands": {
                                    "": {
                                        "env": {
                                            "PEX_VERBOSE": "1",
                                            "=PATH": ".:${scie.env.PATH}",
                                            "PEX_.*": null,
                                            "=PEX": null
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

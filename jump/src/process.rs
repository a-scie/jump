// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::{OsStr, OsString};
use std::process::{Child, Command, ExitStatus, Stdio};

use logging_timer::time;
use os_str_bytes::OsStrBytes;
use sha2::{Digest, Sha256};

use crate::comparable_regex::ComparableRegex;
use crate::config::EnvVar as ConfigEnvVar;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum EnvVar {
    Default((OsString, OsString)),
    Replace((OsString, OsString)),
    Remove(OsString),
    RemoveMatching(ComparableRegex),
}

impl TryFrom<(&ConfigEnvVar, Option<String>)> for EnvVar {
    type Error = String;

    fn try_from(env_var: (&ConfigEnvVar, Option<String>)) -> Result<Self, Self::Error> {
        match env_var {
            (ConfigEnvVar::Default(name), Some(value)) => {
                Ok(Self::Default((name.to_owned().into(), value.into())))
            }
            (ConfigEnvVar::Replace(name), Some(value)) => {
                Ok(Self::Replace((name.to_owned().into(), value.into())))
            }
            (ConfigEnvVar::Default(name), None) => Ok(Self::RemoveMatching(
                ComparableRegex::try_from(name.as_str())?,
            )),
            (ConfigEnvVar::Replace(name), None) => Ok(Self::Remove(name.to_owned().into())),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct EnvVars {
    pub vars: Vec<EnvVar>,
}

impl EnvVars {
    // Translates this `EnvVars` into a sequence of env var set and env var remove instructions
    // that, when carried out in order, will place the environment in the requested state.
    pub fn to_env_vars(
        &self,
        ambient_env: impl Iterator<Item = (OsString, OsString)>,
    ) -> Vec<(OsString, Option<OsString>)> {
        let mut defaults = vec![];
        let mut replacements = vec![];
        let mut removals: HashSet<OsString> = HashSet::new();
        let mut env = ambient_env.collect::<HashMap<_, _>>();
        for env_var in &self.vars {
            match env_var {
                EnvVar::Default((name, val)) => {
                    defaults.push((name.to_owned(), val.to_owned()));
                }
                EnvVar::Replace((name, val)) => {
                    replacements.push((name.to_owned(), val.to_owned()));
                }
                EnvVar::Remove(name) => {
                    removals.insert(name.to_owned());
                }
                EnvVar::RemoveMatching(regex) => {
                    for name in env.keys() {
                        if regex.is_match(name.as_os_str().to_raw_bytes().as_ref()) {
                            removals.insert(name.to_owned());
                        }
                    }
                }
            }
        }
        let mut env_vars = vec![];
        for name in &removals {
            env_vars.push((name.clone(), None));
        }
        for (name, default) in defaults {
            let value = if removals.contains(&name) {
                default
            } else {
                env.remove(&name).unwrap_or(default)
            };
            env_vars.push((name, Some(value)))
        }
        for (name, value) in replacements {
            env_vars.push((name, Some(value)))
        }
        env_vars
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Process {
    pub env: EnvVars,
    pub exe: OsString,
    pub args: Vec<OsString>,
}

fn as_bytes(os_string: &OsString) -> Result<Vec<u8>, String> {
    let string = os_string
        .clone()
        .into_string()
        .map_err(|e| format!("Failed to encode as UTF-8 string: {e:?}"))?;
    Ok(string.into_bytes())
}

impl Process {
    #[time("debug", "Process::{}")]
    pub(crate) fn fingerprint(&self) -> Result<String, String> {
        let mut hasher = Sha256::new_with_prefix(as_bytes(&self.exe)?);
        for arg in &self.args {
            hasher.update(as_bytes(arg)?);
        }
        for (name, value) in self.env.to_env_vars(env::vars_os()) {
            if let Some(val) = value {
                hasher.update(as_bytes(&name)?);
                hasher.update(as_bytes(&val)?);
            }
        }
        Ok(format!("{digest:x}", digest = hasher.finalize()))
    }

    fn as_command(
        &self,
        extra_args: impl IntoIterator<Item = impl AsRef<OsStr>>,
        extra_env: impl IntoIterator<Item = (OsString, Option<OsString>)>,
    ) -> Command {
        let mut command = Command::new(&self.exe);
        command.args(&self.args).args(extra_args);
        for (name, value) in self
            .env
            .to_env_vars(env::vars_os())
            .into_iter()
            .chain(extra_env)
        {
            match value {
                Some(val) => {
                    command.env(name, val);
                }
                None => {
                    command.env_remove(name);
                }
            }
        }
        command
    }

    pub fn execute(
        &self,
        extra_args: impl IntoIterator<Item = OsString>,
        extra_env: impl IntoIterator<Item = (OsString, Option<OsString>)>,
    ) -> Result<ExitStatus, String> {
        self.as_command(extra_args, extra_env)
            .spawn()
            .map_err(|e| {
                format!(
                    "Failed to spawn {exe:?} {args:?}: {e}",
                    exe = self.exe,
                    args = self.args
                )
            })?
            .wait()
            .map_err(|e| {
                format!(
                    "Spawned process with {exe:?} {args:?} but failed to gather its exit \
                    status: {e}",
                    exe = self.exe,
                    args = self.args
                )
            })
    }

    pub fn spawn_stdout(&self, args: &[&str]) -> Result<Child, String> {
        self.as_command(args, None)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| {
                format!(
                    "Failed to spawn {exe:?} {args:?}: {e}",
                    exe = self.exe,
                    args = self.args
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use os_str_bytes::OsStrBytes;

    use crate::comparable_regex::ComparableRegex;
    use crate::{EnvVar, EnvVars};

    #[ctor::ctor]
    fn init() {
        env_logger::init();
    }

    #[test]
    fn to_env_vars_empty() {
        assert_eq!(
            Vec::<(OsString, Option<OsString>)>::new(),
            EnvVars { vars: vec![] }.to_env_vars([].into_iter())
        )
    }

    fn assert_to_env_vars(ambient_env: &[(OsString, OsString)]) {
        assert_eq!(
            vec![("foo".into(), Some("bar".into()))],
            EnvVars {
                vars: vec![
                    EnvVar::Replace(("foo".into(), "bar".into())),
                    EnvVar::RemoveMatching(ComparableRegex::try_from("__DNE__.*").unwrap())
                ]
            }
            .to_env_vars(ambient_env.iter().map(|tup| tup.to_owned()))
        )
    }

    #[test]
    fn to_env_vars() {
        assert_to_env_vars(&[])
    }

    #[cfg(windows)]
    fn create_non_utf8_string() -> OsString {
        use std::os::windows::ffi::OsStringExt;

        // This value is the 1st high surrogate code point See Chapter 3 (Conformance) section 3.9,
        // "UTF-8": https://www.unicode.org/versions/Unicode15.0.0/ch03.pdf
        //
        // > * Because surrogate code points are not Unicode scalar values, any UTF-8 byte
        // >   sequence that would otherwise map to code points U+D800..U+DFFF is ill-
        // >   formed.
        //
        // As such it is valid utf16, but we expect it to fail to convert to utf8.
        OsString::from_wide(&[0xD800])
    }

    #[cfg(unix)]
    fn create_non_utf8_string() -> OsString {
        use std::os::unix::ffi::OsStringExt;

        // This value is taken directly from the original repro example in
        // https://github.com/pantsbuild/scie-pants/issues/198 that led here.
        OsString::from_vec(vec![b'b', 0xA5, b'r'])
    }

    #[test]
    fn to_env_vars_non_utf8() {
        let non_utf8 = create_non_utf8_string();
        assert!(non_utf8.clone().into_string().is_err());

        assert_to_env_vars(&[(non_utf8.clone(), "baz".into())]);
        assert_to_env_vars(&[("baz".into(), non_utf8.clone())]);
        assert_to_env_vars(&[(non_utf8.clone(), non_utf8.clone())]);

        assert_eq!(
            // N.B.: Our docs guaranty removals are done first, then defaults are set and
            // finally overwrites are processed. This is regardless of the order of the env var
            // entries in the lift manifest JSON document.
            vec![("baz".into(), None), ("foo".into(), Some("bar".into()))],
            EnvVars {
                vars: vec![
                    EnvVar::Replace(("foo".into(), "bar".into())),
                    EnvVar::RemoveMatching(ComparableRegex::try_from("^baz$").unwrap())
                ]
            }
            .to_env_vars([("baz".into(), non_utf8.clone())].into_iter()),
            "Expected removal of an env var with a non-utf8 value to succeed."
        );

        let mut re = String::new();
        re.push('^');
        for byte in non_utf8.as_os_str().to_raw_bytes().as_ref() {
            re.push_str(format!(r"(?-u:\x{:X})", byte).as_str());
        }
        re.push('$');
        assert_eq!(
            vec![(non_utf8.clone(), None)],
            EnvVars {
                vars: vec![EnvVar::RemoveMatching(
                    ComparableRegex::try_from(re.as_str()).unwrap()
                )]
            }
            .to_env_vars([(non_utf8.clone(), "baz".into())].into_iter()),
            "Expected removal of an env var with a non-utf8 name to succeed."
        );
    }
}

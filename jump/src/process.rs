// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::process::{Child, Command, ExitStatus, Stdio};

use logging_timer::time;
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
    fn to_env_vars(&self) -> Vec<(OsString, Option<OsString>)> {
        let mut defaults = vec![];
        let mut replacements = vec![];
        let mut removals: HashSet<OsString> = HashSet::new();
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
                    for (name, _) in env::vars() {
                        if regex.is_match(name.as_str()) {
                            removals.insert(name.into());
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
                env::var_os(&name).unwrap_or(default)
            };
            env_vars.push((name, Some(value)))
        }
        for (name, value) in replacements {
            env_vars.push((name, Some(value)))
        }
        env_vars
    }

    pub fn export(&self) {
        for (name, value) in self.to_env_vars() {
            match value {
                Some(val) => env::set_var(name, val),
                None => env::remove_var(name),
            }
        }
    }
}

pub fn execute(exe: OsString, args: Vec<OsString>, argv_skip: usize) -> Result<ExitStatus, String> {
    Command::new(&exe)
        .args(&args)
        .args(env::args().skip(argv_skip))
        .spawn()
        .map_err(|e| format!("Failed to spawn {exe:?} {args:?}: {e}"))?
        .wait()
        .map_err(|e| format!("Spawned {exe:?} {args:?} but failed to gather its exit status: {e}"))
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
        for (key, value) in self.env.to_env_vars() {
            if let Some(val) = value {
                hasher.update(as_bytes(&key)?);
                hasher.update(as_bytes(&val)?);
            }
        }
        Ok(format!("{digest:x}", digest = hasher.finalize()))
    }

    fn as_command(&self) -> Command {
        let mut command = Command::new(&self.exe);
        command.args(&self.args);
        for (name, value) in self.env.to_env_vars() {
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
        extra_env: impl IntoIterator<Item = (OsString, OsString)>,
    ) -> Result<ExitStatus, String> {
        self.as_command()
            .envs(extra_env)
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
        self.as_command()
            .args(args)
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

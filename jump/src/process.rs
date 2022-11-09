// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::ffi::OsString;
use std::process::{Command, ExitStatus};

use crate::config::EnvVar as ConfigEnvVar;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EnvVar {
    Default(OsString),
    Replace(OsString),
}

impl From<&ConfigEnvVar> for EnvVar {
    fn from(value: &ConfigEnvVar) -> Self {
        match value {
            ConfigEnvVar::Default(name) => Self::Default(name.to_owned().into()),
            ConfigEnvVar::Replace(name) => Self::Replace(name.to_owned().into()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnvVars {
    pub vars: Vec<(EnvVar, OsString)>,
}

impl EnvVars {
    fn into_env_vars(self) -> impl Iterator<Item = (OsString, OsString)> {
        self.vars.into_iter().map(|(env_var, value)| match env_var {
            EnvVar::Default(name) => {
                let value = std::env::var_os(&name).unwrap_or(value);
                (name, value)
            }
            EnvVar::Replace(name) => (name, value),
        })
    }

    pub fn export(self) {
        for (name, value) in self.into_env_vars() {
            std::env::set_var(name, value);
        }
    }
}

pub fn execute(exe: OsString, args: Vec<OsString>, argv_skip: usize) -> Result<ExitStatus, String> {
    execute_with_env(exe, args, argv_skip, [].into_iter())
}

fn execute_with_env<E>(
    exe: OsString,
    args: Vec<OsString>,
    argv_skip: usize,
    env: E,
) -> Result<ExitStatus, String>
where
    E: Iterator<Item = (OsString, OsString)>,
{
    Command::new(&exe)
        .envs(env)
        .args(&args)
        .args(std::env::args().skip(argv_skip))
        .spawn()
        .map_err(|e| format!("Failed to spawn {exe:?} {args:?}: {e}"))?
        .wait()
        .map_err(|e| format!("Spawned {exe:?} {args:?} but failed to gather its exit status: {e}"))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Process {
    pub env: EnvVars,
    pub exe: OsString,
    pub args: Vec<OsString>,
}

impl Process {
    pub fn execute(self) -> Result<ExitStatus, String> {
        execute_with_env(self.exe, self.args, usize::MAX, self.env.into_env_vars())
    }
}

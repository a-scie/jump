// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

#[macro_use]
extern crate log;

#[macro_use]
extern crate structure;

mod archive;
mod atomic;
mod cmd_env;
mod comparable_regex;
pub mod config;
mod context;
pub mod fingerprint;
mod installer;
mod jump;
mod lift;
mod placeholders;
mod process;
mod zip;

use std::env;
use std::env::current_exe;
use std::path::PathBuf;

use itertools::Itertools;
use log::Level;
use logging_timer::{time, timer};

pub use crate::archive::create_options;
use crate::config::Config;
pub use crate::config::Jump;
pub use crate::context::ARCH;
use crate::installer::Installer;
// Exposed for the package crate post-processing of the scie-jump binary.
pub use crate::jump::EOF_MAGIC;
pub use crate::lift::{load_lift, File, Lift, ScieBoot, Source};
pub use crate::process::{execute, EnvVar, EnvVars, Process};
pub use crate::zip::check_is_zip;

pub struct SelectBoot {
    pub scie: CurrentExe,
    pub boots: Vec<ScieBoot>,
    pub description: Option<String>,
    pub error_message: String,
}

pub const BOOT_PACK_HELP: &str = "\
(-sj|--jump|--scie-jump [PATH])
(-1|--single-lift-line|--no-single-lift-line)
[lift manifest]*

Pack the given lift manifests into scie executables. If no manifests
are given, looks for `lift.json` in the current directory. By
default the current scie-jump is used as the scie tip, but an
alternate scie-jump binary can be specified using --scie-jump. By
default the lift manifest is appended to the tail of the scie as a
single line JSON document, but can be made a multi-line
pretty-printed JSON document by passing --no-single-lift-line.";

fn help() -> String {
    format!(
        "\
For SCIE=<boot_command> you can select from the following:

boot-pack
    {boot_pack_help}

help: Display this help message.

inspect: Pretty-print this scie's lift manifest to stdout.

install (-s|--symlink) [dest dir]*

    Install all the commands in this scie to each dest dir given. If no
    dest dirs are given, installs them in the current directory.

list: List the names of the commands contained in this scie.

split (-n|--dry-run) [directory]? [-- [file]*]?

    Split this scie into its component files in the given directory or
    else the current directory if no argument is given. To just split out
    certain files, list their names or ids after `--`.
",
        boot_pack_help = BOOT_PACK_HELP.split('\n').join("\n    ")
    )
}

pub enum BootAction {
    Execute((Process, bool)),
    Help((String, i32)),
    Inspect((Jump, Lift)),
    Install((PathBuf, Vec<ScieBoot>)),
    List(Vec<ScieBoot>),
    Pack((Jump, PathBuf)),
    Select(SelectBoot),
    Split((Jump, Lift, PathBuf)),
}

pub fn config(jump: Jump, mut lift: Lift) -> Config {
    let other = lift.other.take();
    Config::new(jump, lift, other)
}

pub struct CurrentExe {
    exe: PathBuf,
    invoked_as: PathBuf,
}

impl CurrentExe {
    pub fn name(&self) -> Option<&str> {
        #[cfg(windows)]
        let invoked_as = self.invoked_as.file_stem();

        #[cfg(unix)]
        let invoked_as = self.invoked_as.file_name();

        invoked_as.and_then(|basename| basename.to_str())
    }

    pub fn invoked_as(&self) -> String {
        self.invoked_as
            .to_str()
            .map(|path| path.to_string())
            .unwrap_or_else(|| format!("{}", self.invoked_as.display()))
    }
}

fn find_current_exe() -> Result<CurrentExe, String> {
    let exe =
        current_exe().map_err(|e| format!("Failed to find path of the current executable: {e}"))?;
    let invoked_as = if let Some(arg) = env::args_os().next() {
        PathBuf::from(arg)
    } else {
        exe.clone()
    };
    Ok(CurrentExe { exe, invoked_as })
}

#[time("debug", "jump::{}")]
pub fn prepare_boot(scie_jump_version: &str) -> Result<BootAction, String> {
    let current_exe = find_current_exe()?;
    let file = std::fs::File::open(&current_exe.exe).map_err(|e| {
        format!(
            "Failed to open current exe at {exe} for reading: {e}",
            exe = current_exe.exe.display(),
        )
    })?;
    let data = unsafe {
        memmap2::Mmap::map(&file)
            .map_err(|e| format!("Failed to mmap {exe}: {e}", exe = current_exe.exe.display()))?
    };

    if let Some(jump) = jump::load(scie_jump_version, &data, &current_exe.exe)? {
        return Ok(BootAction::Pack((jump, current_exe.exe)));
    }

    let (jump, lift) = lift::load_scie(&current_exe.exe, &data)?;
    trace!(
        "Loaded lift manifest from {current_exe}:\n{lift:#?}",
        current_exe = current_exe.exe.display()
    );

    if let Some(value) = env::var_os("SCIE") {
        if "boot-pack" == value {
            return Ok(BootAction::Pack((jump, current_exe.exe)));
        } else if "help" == value {
            return Ok(BootAction::Help((format!("{help}\n", help = help()), 0)));
        } else if "inspect" == value {
            return Ok(BootAction::Inspect((jump, lift)));
        } else if "install" == value {
            return Ok(BootAction::Install((current_exe.exe, lift.boots())));
        } else if "list" == value {
            return Ok(BootAction::List(lift.boots()));
        } else if "split" == value {
            return Ok(BootAction::Split((jump, lift, current_exe.exe)));
        } else if !PathBuf::from(&value).exists() {
            let help_message = format!(
                "The SCIE environment variable is set to {value:?} which is not a scie path\n\
                or one of the known SCIE boot commands.\n\
                \n\
                {help}\
                ",
                help = help()
            );
            return Ok(BootAction::Help((help_message, 1)));
        }
    }

    if lift.load_dotenv {
        let _timer = timer!(Level::Debug; "jump::load_dotenv");
        match dotenv::from_filename(".env") {
            Ok(env) => {
                let mut iter = env.iter();
                while let Some((key, value)) = iter.try_next().map_err(|err| {
                    format!(
                        "This scie requested .env files be loaded but there was an error doing so: \
                        {err}"
                    )
                })? {
                    if std::env::var(key).is_err() {
                        std::env::set_var(key, value);
                    }
                }
            }
            Err(_) => {
                debug!(
                    "No .env files found for invocation of {current_exe} from cwd of {cwd:?}",
                    current_exe = current_exe.exe.display(),
                    cwd = env::current_dir()
                )
            }
        }
    }
    let payload = &data[jump.size..data.len() - lift.size];
    let installer = Installer::new(payload);
    match context::select_command(&current_exe, &jump, &lift, &installer) {
        Ok(selected_command) => {
            installer.install(&selected_command.files)?;
            let process = selected_command.process;
            trace!("Prepared {process:#?}");
            env::set_var("SCIE", current_exe.exe.as_os_str());
            env::set_var("SCIE_ARGV0", current_exe.invoked_as.as_os_str());
            Ok(BootAction::Execute((
                process,
                selected_command.argv1_consumed,
            )))
        }
        Err(error_message) => Ok(BootAction::Select(SelectBoot {
            scie: current_exe,
            boots: lift.boots(),
            description: lift.description,
            error_message,
        })),
    }
}

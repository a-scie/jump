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
pub mod io;
mod jump;
mod lift;
mod placeholders;
mod process;
mod zip;

use std::env;
use std::env::current_exe;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use itertools::Itertools;
use log::Level;
use logging_timer::{time, timer};
use semver::Version;

pub use crate::archive::create_options;
use crate::config::Config;
pub use crate::config::Jump;
pub use crate::context::ARCH;
use crate::installer::{Directory, FileSource, Scie, install};
// Exposed for the package crate post-processing of the scie-jump binary.
pub use crate::jump::EOF_MAGIC_V2 as EOF_MAGIC;
pub use crate::jump::load as load_jump;
pub use crate::lift::{File, Lift, ScieBoot, Source, load_lift};
pub use crate::process::{EnvVar, EnvVars, Process};
pub use crate::zip::check_is_zip;

pub struct SelectBoot {
    pub scie: CurrentExe,
    pub boots: Vec<ScieBoot>,
    pub description: Option<String>,
    pub error_message: String,
}

pub const BOOT_PACK_HELP: &str = "\
-V|--version Print the scie-jump version and exit.

(-sj|--jump|--scie-jump [PATH])
(-1|--single-lift-line|--no-single-lift-line)
[lift manifest]*

Pack the given lift manifests into scie executables. If no manifests
are given, looks for `lift.json` in the current directory. By
default the current scie-jump is used as the scie tip, but an
alternate scie-jump binary can be specified using --scie-jump. By
default the lift manifest is appended to the tail of the scie as a
single line JSON document, but can be made a multi-line
pretty-printed JSON document by passing --no-single-lift-line.

-x|--launch|--launch=[lift manifest]
[arg]*

Execute the given lift manifest. If no manifest is given, looks for
`lift.json` in the current directory. Any remaining arguments are
passed through to the scie lift command that gets selected.
";

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

pub struct ScieExe {
    exe: PathBuf,
    lift: Option<PathBuf>,
    tmp: Option<tempfile::NamedTempFile>,
}

impl ScieExe {
    fn scie(scie: PathBuf) -> Self {
        Self {
            exe: scie,
            lift: None,
            tmp: None,
        }
    }

    fn jump(jump: PathBuf, lift: PathBuf) -> Self {
        Self {
            exe: jump,
            lift: Some(lift),
            tmp: None,
        }
    }

    pub fn is_scie(&self) -> bool {
        self.lift.is_none()
    }

    #[cfg(windows)]
    fn create_script(
        scie_jump: &Path,
        name: Option<&str>,
        lift: &Path,
    ) -> (String, Option<&'static str>) {
        let script = if let Some(scie_boot) = name {
            format!(
                "\
#!/usr/bin/env pwsh
if (-not $env:SCIE_BOOT) {{
    $env:SCIE_BOOT = '{scie_boot}'
}}

&{scie_jump} --launch={lift} @args
exit $LASTEXITCODE
",
                scie_jump = scie_jump.display(),
                lift = lift.display()
            )
        } else {
            format!(
                "\
#!/usr/bin/env pwsh

&{scie_jump} --launch={lift} @args
exit $LASTEXITCODE
",
                scie_jump = scie_jump.display(),
                lift = lift.display()
            )
        };
        (script, Some(".ps1"))
    }

    #[cfg(windows)]
    fn mark_executable(_file: &mut std::fs::File, _lift: &Path) -> Result<(), String> {
        Ok(())
    }

    #[cfg(unix)]
    fn create_script(
        scie_jump: &Path,
        name: Option<&str>,
        lift: &Path,
    ) -> (String, Option<&'static str>) {
        let script = if let Some(scie_boot) = name {
            format!(
                "\
#!/bin/sh -e
if [ -z \"$SCIE_BOOT\" ]; then
    export SCIE_BOOT=\"{scie_boot}\"
fi

exec {scie_jump} --launch={lift} \"$@\"
",
                scie_jump = scie_jump.display(),
                lift = lift.display()
            )
        } else {
            format!(
                "\
#!/bin/sh -e

exec {scie_jump} --launch={lift} \"$@\"
",
                scie_jump = scie_jump.display(),
                lift = lift.display()
            )
        };
        (script, None)
    }

    #[cfg(unix)]
    fn mark_executable(file: &mut std::fs::File, lift: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = file
            .metadata()
            .map_err(|e| {
                format!(
                    "Failed to obtain temporary shim file metadata for {lift}: {e}",
                    lift = lift.display()
                )
            })?
            .permissions();
        perms.set_mode(0o755);
        file.set_permissions(perms).map_err(|e| {
            format!(
                "Failed to mark temporary shim for {lift} as executable: {e}",
                lift = lift.display()
            )
        })
    }

    pub fn exe(&mut self, command: &ScieBoot, dest: &Path) -> Result<PathBuf, String> {
        if let Some(lift) = self.lift.as_deref() {
            let (contents, extension) = Self::create_script(
                &self.exe,
                if command.default {
                    None
                } else {
                    Some(&command.name)
                },
                lift,
            );
            let mut tmp_builder = tempfile::Builder::new();
            if let Some(ext) = extension {
                tmp_builder.suffix(ext);
            }
            let tmp = self.tmp.insert(tmp_builder.tempfile_in(dest).map_err(|e| {
                format!(
                    "Failed to create temporary file to write scie shim for {lift} to: {e}",
                    lift = lift.display()
                )
            })?);
            let tmp_file = tmp.as_file_mut();
            tmp_file.write_all(contents.as_bytes()).map_err(|e| {
                format!(
                    "Failed to write temporary script shim for {lift}: {e}",
                    lift = lift.display()
                )
            })?;
            Self::mark_executable(tmp_file, lift)?;
            Ok(tmp.path().to_path_buf())
        } else {
            Ok(self.exe.clone())
        }
    }

    pub fn cmdline(&self) -> String {
        if let Some(lift) = self.lift.as_deref() {
            format!(
                "{scie_jump} --launch={lift}",
                scie_jump = self.exe.display(),
                lift = lift.display()
            )
        } else {
            self.exe.display().to_string()
        }
    }
}

pub enum BootAction {
    Execute((Process, usize, Vec<(OsString, Option<OsString>)>)),
    Help((String, i32)),
    Inspect((Jump, Lift)),
    Install((ScieExe, Vec<ScieBoot>, usize)),
    List(Vec<ScieBoot>),
    Pack((Jump, PathBuf, usize)),
    Select(SelectBoot),
    Split((Jump, Lift, PathBuf, usize)),
}

pub fn config(jump: Jump, mut lift: Lift) -> Config {
    let other = lift.other.take();
    Config::new(jump, lift, other)
}

#[derive(Debug)]
pub struct CurrentExe {
    exe: PathBuf,
    invoked_as: PathBuf,
}

impl CurrentExe {
    pub fn from_path(exe: &Path) -> Self {
        CurrentExe {
            exe: exe.to_path_buf(),
            invoked_as: exe.to_path_buf(),
        }
    }

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
pub fn prepare_boot(current_scie_jump_version: &Version) -> Result<BootAction, String> {
    let current_exe = find_current_exe()?;
    let (jump, lift, mut file_source, scie_exe, argv_skip) = if let Some(jump) =
        jump::load(&current_exe.exe, current_scie_jump_version)?
    {
        if let Some(arg) = env::args().nth(1)
            && (arg == "-x" || arg == "--launch" || arg.starts_with("--launch="))
        {
            let lift_path = Path::new(if arg.len() > "--launch=".len() {
                &arg["--launch=".len()..]
            } else {
                "lift.json"
            });
            let (configured_jump, lift) = load_lift(lift_path)?;
            if let Some(expected_jump) = configured_jump
                && jump != expected_jump
            {
                return Err(format!(
                    "The current scie jump {jump:?} is not the configured scie jump {expected_jump:?}."
                ));
            }
            let directory =
                Directory::new(lift_path.parent().map(Path::to_path_buf).unwrap_or(
                    env::current_dir().map_err(|e| format!("Failed to query CWD: {e}"))?,
                ));
            (
                jump,
                lift,
                FileSource::Directory(directory),
                ScieExe::jump(current_exe.exe.clone(), lift_path.to_path_buf()),
                2,
            )
        } else {
            return Ok(BootAction::Pack((jump, current_exe.exe.clone(), 1)));
        }
    } else {
        let (jump, lift) = lift::load_scie(&current_exe.exe)?;
        trace!(
            "Loaded lift manifest from {current_exe}:\n{lift:#?}",
            current_exe = current_exe.exe.display()
        );
        let scie = Scie::new(&current_exe.exe, &jump)?;
        (
            jump,
            lift,
            FileSource::Scie(scie),
            ScieExe::scie(current_exe.exe.clone()),
            1,
        )
    };

    if let Some(value) = env::var_os("SCIE") {
        if "boot-pack" == value {
            return Ok(BootAction::Pack((jump, current_exe.exe, argv_skip)));
        } else if "help" == value {
            return Ok(BootAction::Help((format!("{help}\n", help = help()), 0)));
        } else if "inspect" == value {
            return Ok(BootAction::Inspect((jump, lift)));
        } else if "install" == value {
            return Ok(BootAction::Install((scie_exe, lift.boots(), argv_skip)));
        } else if "list" == value {
            return Ok(BootAction::List(lift.boots()));
        } else if "split" == value {
            return match scie_exe {
                ScieExe {
                    exe: scie_jump,
                    lift: Some(lift),
                    ..
                } => Err(format!(
                    "Cannot split unless running from a scie; currently executing {lift} using \
                    the scie-jump at {scie_jump}.",
                    lift = lift.display(),
                    scie_jump = scie_jump.display(),
                )),
                ScieExe { exe: scie, .. } => Ok(BootAction::Split((jump, lift, scie, argv_skip))),
            };
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

    let mut ambient_env = env::vars_os().collect::<IndexMap<_, _>>();
    if lift.load_dotenv {
        let _timer = timer!(Level::Debug; "jump::load_dotenv");
        match dotenv::from_filename(".env") {
            Ok(env) => {
                let mut iter = env.iter();
                while let Some((name, value)) = iter.try_next().map_err(|err| {
                    format!(
                        "This scie requested .env files be loaded but there was an error doing so: \
                        {err}"
                    )
                })? {
                    if env::var(name).is_err() {
                        ambient_env.insert(name.into(), value.into());
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
    ambient_env.insert("SCIE".into(), current_exe.exe.clone().into());
    ambient_env.insert("SCIE_ARGV0".into(), current_exe.invoked_as.clone().into());

    match context::select_command(&current_exe, &jump, &lift, &mut file_source, ambient_env) {
        Ok(selected_command) => {
            install(&mut file_source, &selected_command.files)?;
            let process = selected_command.process;
            let extra_env: Vec<(OsString, Option<OsString>)> = vec![
                // Avoid subprocesses that re-execute this SCIE unintentionally getting in an
                // infinite loop.
                ("SCIE_BOOT".into(), None),
            ];
            Ok(BootAction::Execute((
                process,
                if selected_command.argv1_consumed {
                    argv_skip + 1
                } else {
                    argv_skip
                },
                extra_env,
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

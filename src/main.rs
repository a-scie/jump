// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::ffi::OsString;

use proc_exit::{Code, ExitResult};

mod boot;

use jump::{BootAction, Process};

#[cfg(windows)]
fn exec(
    process: Process,
    argv_skip: usize,
    extra_env: Vec<(OsString, Option<OsString>)>,
) -> ExitResult {
    let result = process.execute(
        std::env::args_os().skip(argv_skip).collect::<Vec<_>>(),
        extra_env,
    );
    match result {
        Ok(exit_status) => Code::from(exit_status).ok(),
        Err(message) => Err(Code::FAILURE.with_message(message)),
    }
}

#[cfg(unix)]
fn exec(
    mut process: Process,
    argv_skip: usize,
    extra_env: Vec<(OsString, Option<OsString>)>,
) -> ExitResult {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStringExt;

    use jump::EnvVar;
    use nix::unistd::execve;

    for (name, value) in extra_env {
        match value {
            Some(val) => {
                process.env.vars.push(EnvVar::Replace((name, val)));
            }
            None => {
                process.env.vars.push(EnvVar::Remove(name));
            }
        }
    }
    let env = process.to_env_vars(true);

    let c_exe = CString::new(process.exe.into_vec()).map_err(|e| {
        Code::FAILURE.with_message(format!("Failed to convert executable to a C string: {e}",))
    })?;

    let mut c_args = vec![c_exe.clone()];
    c_args.extend(
        process
            .args
            .into_iter()
            .chain(std::env::args().skip(argv_skip).map(OsString::from))
            .map(|arg| {
                CString::new(arg.into_vec()).map_err(|e| {
                    Code::FAILURE
                        .with_message(format!("Failed to convert argument to a C string: {e}",))
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    );

    let c_env = env
        .into_iter()
        .map(|(mut name, value)| {
            name.push("=");
            name.push(value);
            CString::new(name.into_vec()).map_err(|e| {
                Code::FAILURE.with_message(format!("Failed to convert env var to a C string: {e}"))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    execve(&c_exe, &c_args, &c_env)
        .map_err(|e| {
            Code::new(e as i32).with_message(format!(
                "Failed to exec {c_exe:?} with argv {c_args:?}: {e}"
            ))
        })
        .map(|_| ())
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitResult {
    env_logger::init();

    let action = jump::prepare_boot().map_err(|e| {
        Code::FAILURE.with_message(format!("Failed to prepare a scie jump action: {e}"))
    })?;

    match action {
        BootAction::Execute((process, argv1_consumed, extra_env)) => {
            let argv_skip = if argv1_consumed { 2 } else { 1 };
            exec(process, argv_skip, extra_env)
        }
        BootAction::Help((message, exit_code)) => boot::help(message, exit_code),
        BootAction::Inspect((jump, lift)) => boot::inspect(jump, lift),
        BootAction::Install((scie, commands)) => boot::install(scie, commands),
        BootAction::List(commands) => boot::list(commands),
        BootAction::Pack((jump, scie_jump_path)) => boot::pack(jump, scie_jump_path),
        BootAction::Select(select_boot) => boot::select(select_boot),
        BootAction::Split((jump, lift, scie_path)) => boot::split(jump, lift, scie_path),
        BootAction::Version => {
            println!("{VERSION}");
            Code::SUCCESS.ok()
        }
    }
}

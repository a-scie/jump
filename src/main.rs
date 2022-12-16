// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::env;
use std::env::current_exe;
use std::ffi::OsString;
use std::path::PathBuf;

use proc_exit::{Code, Exit, ExitResult};

mod boot;

use jump::BootAction;

#[cfg(windows)]
fn exec(exe: OsString, args: Vec<OsString>, argv_skip: usize) -> ExitResult {
    let result = jump::execute(exe, args, argv_skip);
    match result {
        Ok(exit_status) => Code::from(exit_status).ok(),
        Err(message) => Err(Code::FAILURE.with_message(message)),
    }
}

#[cfg(unix)]
fn exec(exe: OsString, args: Vec<OsString>, argv_skip: usize) -> ExitResult {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStringExt;

    use nix::unistd::execv;

    let c_exe = CString::new(exe.into_vec()).map_err(|e| {
        Code::FAILURE.with_message(format!("Failed to convert executable to a C string: {e}",))
    })?;

    let mut c_args = vec![c_exe.clone()];
    c_args.extend(
        args.into_iter()
            .chain(std::env::args().skip(argv_skip).map(OsString::from))
            .map(|arg| {
                CString::new(arg.into_vec()).map_err(|e| {
                    Code::FAILURE
                        .with_message(format!("Failed to convert argument to a C string: {e}",))
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    );

    execv(&c_exe, &c_args)
        .map_err(|e| {
            Code::new(e as i32).with_message(format!(
                "Failed to exec {c_exe:?} with argv {c_args:?}: {e}"
            ))
        })
        .map(|_| ())
}

fn find_current_exe() -> Result<PathBuf, Exit> {
    if let Some(arg) = env::args().next() {
        let argv0 = PathBuf::from(arg);
        if argv0.is_file() {
            return Ok(argv0);
        }
    }
    current_exe().map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to find path of the current executable: {e}"
        ))
    })
}

fn main() -> ExitResult {
    env_logger::init();

    if let Some(arg) = env::args().next() {
        let argv0 = PathBuf::from(arg);
        if argv0.is_file() {}
    }
    let current_exe = find_current_exe()?;
    let action = jump::prepare_boot(current_exe).map_err(|e| {
        Code::FAILURE.with_message(format!("Failed to prepare a scie jump action: {e}"))
    })?;

    match action {
        BootAction::Execute((process, argv1_consumed)) => {
            process.env.export();
            let argv_skip = if argv1_consumed { 2 } else { 1 };
            exec(process.exe, process.args, argv_skip)
        }
        BootAction::Help((message, exit_code)) => boot::help(message, exit_code),
        BootAction::Inspect((jump, lift)) => boot::inspect(jump, lift),
        BootAction::Install((scie, commands)) => boot::install(scie, commands),
        BootAction::List(commands) => boot::list(commands),
        BootAction::Pack((jump, scie_jump_path)) => boot::pack(jump, scie_jump_path),
        BootAction::Select(select_boot) => boot::select(select_boot),
        BootAction::Split((jump, lift, scie_path)) => boot::split(jump, lift, scie_path),
    }
}

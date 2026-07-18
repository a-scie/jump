// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use std::ffi::OsString;

use jump::Process;
use proc_exit::{Code, ExitResult};
use scie_jump::boot;

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
            .chain(std::env::args_os().skip(argv_skip))
            .map(|arg| {
                CString::new(arg.as_encoded_bytes()).map_err(|e| {
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
            CString::new(name.as_encoded_bytes()).map_err(|e| {
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

fn main() -> ExitResult {
    env_logger::init();
    let action = boot::prepare_boot()?;
    boot::boot(action, exec)
}

use std::collections::HashMap;
use std::env::current_exe;
use std::ffi::OsString;

use proc_exit::{Code, Exit, ExitResult};

use jump::{Action, Cmd, EnvVar};

fn into_env_vars(env: HashMap<EnvVar, String>) -> impl Iterator<Item = (String, OsString)> {
    env.into_iter().map(|(env_var, value)| match env_var {
        EnvVar::Default(name) => {
            let value = std::env::var_os(&name).unwrap_or_else(|| value.to_owned().into());
            (name, value)
        }
        EnvVar::Replace(name) => (name, value.into()),
    })
}

#[cfg(target_family = "windows")]
fn exec(cmd: Cmd) -> ExitResult {
    use std::process::Command;
    let exit_status = Command::new(&cmd.exe)
        .args(&cmd.args)
        .args(std::env::args().skip(1))
        .envs(std::env::vars())
        .envs(into_env_vars(cmd.env))
        .spawn()
        .map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to spawn {exe:?} {args:?}: {e}",
                exe = cmd.exe,
                args = cmd.args
            ))
        })?
        .wait()
        .map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Spawned {exe:?} {args:?} but failed to gather its exit status: {e}",
                exe = cmd.exe,
                args = cmd.args,
            ))
        })?;
    Code::from_status(exit_status).ok()
}

#[cfg(not(target_family = "windows"))]
fn exec(cmd: Cmd) -> ExitResult {
    use nix::unistd::execve;
    use std::ffi::CString;

    let exe = CString::new(cmd.exe.as_bytes()).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to convert executable {exe:?} to a C string: {e}",
            exe = cmd.exe
        ))
    })?;

    let mut args = vec![exe.clone()];
    for arg in cmd.args.into_iter().chain(std::env::args().skip(1)) {
        args.push(CString::new(arg.as_bytes()).map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to convert argument {arg:?} to a C string: {e}",
            ))
        })?);
    }

    for (name, value) in into_env_vars(cmd.env) {
        std::env::set_var(name, value);
    }
    let env = std::env::vars()
        .map(|(k, v)| CString::new(format!("{k}={v}").as_bytes()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to convert env {env_vars:?} into C strings: {e}",
                env_vars = std::env::vars()
            ))
        })?;

    execve(&exe, &args, &env)
        .map_err(|e| Exit::new(Code::FAILURE).with_message(format!("Failed to exec {args:?}: {e}")))
        .map(|_| ())
}

fn main() -> ExitResult {
    let current_exe = current_exe().map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to find path of the current executable: {e}"
        ))
    })?;
    let action = jump::prepare_action(&current_exe).map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to prepare a scie jump action from executable {exe}: {e}",
            exe = current_exe.display(),
        ))
    })?;
    match action {
        Action::BootPack(size) => Err(Exit::new(Code::FAILURE).with_message(format!(
            "TODO(John Sirois): Implement boot-pack (self size should be {size})"
        ))),
        Action::Cmd(cmd) => {
            std::env::set_var("SCIE", current_exe.as_os_str());
            exec(cmd)
        }
    }
}

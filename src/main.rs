use std::env::current_exe;

use proc_exit::{Code, Exit, ExitResult};

use jump::{Action, Cmd};

#[cfg(target_family = "windows")]
fn exec(cmd: Cmd) -> ExitResult {
    use std::process::Command;
    let exit_status = Command::new(&cmd.exe)
        .args(&cmd.args)
        .args(std::env::args().skip(1))
        .envs(&cmd.env)
        .envs(std::env::vars())
        .spawn()
        .map_err(|e| Code::FAILURE.with_message(format!("Failed to spawn command {cmd:?}: {e}")))?
        .wait()
        .map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Spawned command {cmd:?} but failed to gather its exit status: {e}"
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

    for (name, value) in cmd.env {
        if std::env::var_os(&name).is_none() {
            std::env::set_var(name, value);
        }
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
        Action::Cmd(cmd) => exec(cmd),
    }
}

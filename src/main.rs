use std::env::current_exe;

use proc_exit::{Code, Exit, ExitResult};

use scie_jump::Cmd;

#[cfg(target_family = "windows")]
fn exec(cmd: Cmd) -> ExitResult {
    use std::process::Command;
    let exit_status = Command::new(&cmd.exe)
        .args(&cmd.args)
        .args(std::env::args().skip(1))
        .envs(&cmd.env)
        .envs(std::env::vars())
        .spawn()
        .map_err(|e| {
            Exit::new(Code::FAILURE)
                .with_message(format!("Failed to spawn command {:?}: {}", cmd, e))
        })?
        .wait()
        .map_err(|e| {
            Exit::new(Code::FAILURE).with_message(format!(
                "Spawned command {:?} but failed to gather its exit status: {}",
                cmd, e
            ))
        })?;
    Code::from_status(exit_status).ok()
}

#[cfg(not(target_family = "windows"))]
fn exec(cmd: Cmd) -> ExitResult {
    use nix::unistd::execve;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let exe = match CString::new(cmd.exe.as_bytes()) {
        Err(e) => proc_exit::exit(Err(Exit::new(Code::FAILURE).with_message(format!(
            "Failed to convert executable {:?} to a C string: {}",
            cmd.exe, e
        )))),
        Ok(exe) => exe,
    };

    let mut args = vec![exe.clone()];
    for arg in cmd.args {
        args.push(match CString::new(arg.as_bytes()) {
            Err(e) => proc_exit::exit(Err(Exit::new(Code::FAILURE).with_message(format!(
                "Failed to convert argument {:?} to a C string: {}",
                arg, e
            )))),
            Ok(arg) => arg,
        });
    }
    for arg in std::env::args_os().skip(1) {
        args.push(match CString::new(arg.as_os_str().as_bytes()) {
            Err(e) => proc_exit::exit(Err(Exit::new(Code::FAILURE).with_message(format!(
                "Failed to convert argument {:?} to a C string: {}",
                arg, e
            )))),
            Ok(arg) => arg,
        });
    }

    for (k, v) in cmd.env {
        if std::env::var_os(&k).is_none() {
            std::env::set_var(k, v);
        }
    }
    let env = match std::env::vars()
        .map(|(k, v)| CString::new(format!("{}={}", k, v).as_bytes()))
        .collect::<Result<Vec<_>, _>>()
    {
        Err(e) => proc_exit::exit(Err(Exit::new(Code::FAILURE).with_message(format!(
            "Failed to convert env {:?} into C strings: {}",
            std::env::vars(),
            e
        )))),
        Ok(env) => env,
    };
    execve(&exe, &args, &env).map_err(|e| {
        Exit::new(Code::FAILURE).with_message(format!("Failed to exec {:?}: {}", args, e))
    })?;
    Code::FAILURE.ok()
}

fn main() {
    let current_exe = match current_exe() {
        Err(e) => proc_exit::exit(Err(Exit::new(Code::FAILURE).with_message(format!(
            "Failed to find path of the current executable: {}",
            e
        )))),
        Ok(current_exe) => current_exe,
    };
    let cmd = match scie_jump::prepare_command(&current_exe) {
        Err(e) => proc_exit::exit(Err(Exit::new(Code::FAILURE).with_message(format!(
            "Failed to prepare a scie jump command from executable {}: {}",
            current_exe.display(),
            e
        )))),
        Ok(cmd) => cmd,
    };
    proc_exit::exit(exec(cmd));
}

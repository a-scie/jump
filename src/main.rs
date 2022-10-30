use std::env::current_exe;
use std::ffi::OsString;

use proc_exit::{Code, Exit, ExitResult};

use jump::Action;

#[cfg(target_family = "windows")]
fn exec(exe: OsString, args: Vec<OsString>) -> ExitResult {
    use std::process::Command;
    let exit_status = Command::new(&exe)
        .args(&args)
        .args(std::env::args().skip(1))
        .spawn()
        .map_err(|e| Code::FAILURE.with_message(format!("Failed to spawn {exe:?} {args:?}: {e}")))?
        .wait()
        .map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Spawned {exe:?} {args:?} but failed to gather its exit status: {e}",
            ))
        })?;
    Code::from_status(exit_status).ok()
}

#[cfg(not(target_family = "windows"))]
fn exec(exe: OsString, args: Vec<OsString>) -> ExitResult {
    use nix::unistd::execv;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStringExt;

    let c_exe = CString::new(exe.into_vec()).map_err(|e| {
        Code::FAILURE.with_message(format!("Failed to convert executable to a C string: {e}",))
    })?;

    let mut c_args = vec![c_exe.clone()];
    c_args.extend(
        args.into_iter()
            .chain(std::env::args().skip(1).map(OsString::from))
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
            Exit::new(Code::new(e as i32))
                .with_message(format!("Failed to exec {c_exe:?} with argv {c_args:?}: {e}"))
        })
        .map(|_| ())
}

fn main() -> ExitResult {
    env_logger::init();

    let current_exe = current_exe().map_err(|e| {
        Code::FAILURE.with_message(format!(
            "Failed to find path of the current executable: {e}"
        ))
    })?;
    std::env::set_var("SCIE", current_exe.as_os_str());

    let action = jump::prepare_action(current_exe).map_err(|e| {
        Code::FAILURE.with_message(format!("Failed to prepare a scie jump action: {e}"))
    })?;
    match action {
        Action::BootPack(size) => Err(Exit::new(Code::FAILURE).with_message(format!(
            "TODO(John Sirois): Implement boot-pack (self size should be {size})"
        ))),
        Action::Execute(process) => {
            process.env.export();
            exec(process.exe, process.args)
        }
    }
}

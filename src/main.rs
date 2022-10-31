use std::env::current_exe;
use std::ffi::OsString;

use proc_exit::{Code, Exit, ExitResult};

use jump::Action;

#[cfg(target_family = "windows")]
fn exec(exe: OsString, args: Vec<OsString>, argv_skip: usize) -> ExitResult {
    use std::process::Command;
    let exit_status = Command::new(&exe)
        .args(&args)
        .args(std::env::args().skip(argv_skip))
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
fn exec(exe: OsString, args: Vec<OsString>, argv_skip: usize) -> ExitResult {
    use nix::unistd::execv;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStringExt;

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
            Exit::new(Code::new(e as i32)).with_message(format!(
                "Failed to exec {c_exe:?} with argv {c_args:?}: {e}"
            ))
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
        Action::BootPack(jump) => Err(Code::FAILURE
            .with_message(format!("TODO(John Sirois): Implement boot-pack: {jump:#?}"))),
        Action::Execute((process, argv1_consumed)) => {
            process.env.export();
            let argv_skip = if argv1_consumed { 2 } else { 1 };
            exec(process.exe, process.args, argv_skip)
        }
        Action::SelectBoot(select_boot) => Err(Code::FAILURE.with_message(format!(
            "This Scie binary has no default boot command.\n\
            Please select from the following:\n\
            {boot_commands}\n\
            \n\
            You can select a boot command by passing it as the 1st argument or else by \
            setting the SCIE_BOOT environment variable.\n\
            {error_message}",
            boot_commands = select_boot
                .boots
                .into_iter()
                .map(|boot| if let Some(description) = boot.description {
                    format!("{name}: {description}", name = boot.name)
                } else {
                    boot.name
                })
                .collect::<Vec<_>>()
                .join("\n"),
            error_message = select_boot.error_message.unwrap_or_default()
        ))),
    }
}

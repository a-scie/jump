// Copyright 2022 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

use jump::config::Fmt;
use jump::{Jump, Lift, SelectBoot};
use proc_exit::{Code, ExitResult};

mod pack;
mod split;
pub(crate) use pack::set as pack;
pub(crate) use split::split;

pub(crate) fn help(message: String, exit_code: i32) -> ExitResult {
    let code = Code::from(exit_code);
    if code.is_err() {
        Err(code.with_message(message))
    } else {
        print!("{}", message);
        code.ok()
    }
}

pub(crate) fn inspect(jump: Jump, lift: Lift) -> ExitResult {
    let config = jump::config(jump, lift);
    let fmt = Fmt::new().pretty(true).trailing_newline(true);
    config
        .serialize(std::io::stdout(), fmt)
        .map_err(|e| Code::FAILURE.with_message(format!("Failed to serialize lift manifest: {e}")))
}

pub(crate) fn select(select_boot: SelectBoot) -> ExitResult {
    Err(Code::FAILURE.with_message(format!(
        "This Scie binary has no default boot command.\n\
            {description}\n\
            Please select from the following boot commands:\n\
            \n\
            {boot_commands}\n\
            \n\
            You can select a boot command by passing it as the 1st argument or else by \
            setting the SCIE_BOOT environment variable.\n\
            {error_message}",
        description = select_boot
            .description
            .map(|message| format!("\n{message}\n"))
            .unwrap_or_default(),
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
        error_message = select_boot
            .error_message
            .map(|err| format!("\nERROR: {err}"))
            .unwrap_or_default()
    )))
}

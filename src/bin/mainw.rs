// Copyright 2026 Science project contributors.
// Licensed under the Apache License, Version 2.0 (see LICENSE).

#![cfg(windows)]
#![windows_subsystem = "windows"]

use std::ffi::{CString, OsString};
use std::fmt::Display;
use std::process::exit;

use jump::{BootAction, Process};
use proc_exit::{Code, ExitResult};
use scie_jump::{VERSION, boot};
use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExA, DestroyWindow, HWND_MESSAGE, MB_OK, MSG, MessageBoxA, PM_REMOVE, PeekMessageA,
    WINDOW_EX_STYLE, WINDOW_STYLE,
};
use windows::core::{PCSTR, s};

enum Title {
    Error,
    Warning,
}

#[macro_export]
macro_rules! error {
    ($($fmt_args:tt)*) => {{
        $crate::message(Title::Error, format_args!($($fmt_args)*));
        exit(1)
    }}
}

#[macro_export]
macro_rules! warn {
    ($($fmt_args:tt)*) => {{
        $crate::message(Title::Warning, format_args!($($fmt_args)*));
    }}
}

fn message(title: Title, message: impl Display) {
    let message = unsafe { CString::new(message.to_string()).unwrap_unchecked() };
    let message = PCSTR::from_raw(message.as_ptr() as *const _);
    unsafe {
        MessageBoxA(
            None,    // hWnd (No owner window - this is a free-floating error dialog)
            message, // lpText
            match title {
                Title::Error => s!("Error"),
                Title::Warning => s!("Warning"),
            }, // lpCaption
            MB_OK,   // uType (Just an Ok button to dismiss the error dialog)
        )
    };
}

fn clear_app_starting_cursor_state() {
    let mut msg = MSG::default();
    unsafe {
        // Create a message-only (invisible) window.
        // See: https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features#message-only-windows
        if let Ok(hwnd) = CreateWindowExA(
            WINDOW_EX_STYLE(0), // dwExStyle
            // (See https://learn.microsoft.com/en-us/windows/win32/winmsg/about-window-classes#system-classes
            // for the Static system window class)
            s!("STATIC"),       // lpClassName
            s!("scie-jumpw"),   // lpWindowName
            WINDOW_STYLE(0),    // dwStyle
            0,                  // X
            0,                  // Y
            0,                  // nWidth
            0,                  // nHeight
            Some(HWND_MESSAGE), // hWndParent (This is what makes the window message-only)
            None,               // hMenu
            None,               // hInstance
            None,               // lpParam
        ) {
            // Process all pending messages (remove them from the window message queue); this is
            // enough to clear the app starting cursor state.
            // See: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-peekmessagea
            let _ = PeekMessageA(&mut msg, Some(hwnd), 0, 0, PM_REMOVE);
            let _ = DestroyWindow(hwnd);
        }
    }
}

fn attach_parent_process_console() -> bool {
    return matches!(unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }, Ok(()));
}

fn exec(
    process: Process,
    argv_skip: usize,
    extra_env: Vec<(OsString, Option<OsString>)>,
) -> ExitResult {
    let mut child = process
        .as_command(
            std::env::args_os().skip(argv_skip).collect::<Vec<_>>(),
            extra_env,
        )
        .spawn()
        .map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to spawn {exe:?} {args:?}: {e}",
                exe = process.exe,
                args = process.args
            ))
        })?;

    clear_app_starting_cursor_state();

    let result = child.wait().map_err(|e| {
        format!(
            "Spawned process with {exe:?} {args:?} but failed to gather its exit \
            status: {e}",
            exe = process.exe,
            args = process.args
        )
    });
    match result {
        Ok(exit_status) => Code::from(exit_status).ok(),
        Err(message) => Err(Code::FAILURE.with_message(message)),
    }
}

fn main() -> ExitResult {
    let action = if attach_parent_process_console() {
        env_logger::init();
        let action = boot::prepare_boot()?;
        if matches!(action, BootAction::Pack((_, _, _, bare)) if bare) {
            warn!(
                "Console output may be garbled when using scie-jumpw.exe to package scies.\n\
                \n\
                You should prefer using scie-jump.exe instead, which can be found at:\n\
                https://github.com/a-scie/jump/releases/tag/v{VERSION}"
            )
        }
        action
    } else {
        boot::prepare_boot()?
    };
    match boot::boot(action, exec) {
        Ok(()) => Ok(()),
        Err(err) => error!("Problem booting scie: {err}"),
    }
}

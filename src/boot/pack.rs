use std::path::PathBuf;

use jump::Jump;
use proc_exit::{Code, ExitResult};

pub(crate) fn set(jump: Jump, scie_jump_path: PathBuf) -> ExitResult {
    // 1. Optional path: Default name is lift.json in CWD if no path or path is a directory.
    // 2. Open the lift manifest and Config::parse
    // 3. Set CWD to lift manifest parent dir.
    // 3. Must have >=1 file and for each file check hash (and size) or fail.
    // 4. Output name is the lift manifest name, but in original CWD.
    // Optional --single-lift-line / --no-single-lift-line for lift manifest trailer packing.

    Err(Code::FAILURE.with_message(format!(
        "TODO(John Sirois): Implement boot-pack for {path}: {jump:#?}",
        path = scie_jump_path.display()
    )))
}

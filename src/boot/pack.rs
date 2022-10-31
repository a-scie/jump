use jump::Jump;
use proc_exit::{Code, ExitResult};

pub(crate) fn make(jump: Jump) -> ExitResult {
    Err(Code::FAILURE.with_message(format!("TODO(John Sirois): Implement boot-pack: {jump:#?}")))
}

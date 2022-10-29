use proc_exit::{Code, Exit, ExitResult};
use std::path::PathBuf;

fn main() -> ExitResult {
    if std::env::args().len() != 2 {
        return Err(Code::FAILURE.with_message(
            "Usage: cargo run -p package <dest dir>\n\
            \n\
            A destination directory for the packaged scie-jump executable is \
            required.",
        ));
    }

    let src: PathBuf = env!("SCIE_STRAP").into();
    let dest_dir: PathBuf = std::env::args().skip(1).next().unwrap().into();
    let dst = dest_dir.join(
        src.file_name()
            .expect(format!("Expected {} to end in a file name.", src.display()).as_str()),
    );
    if dest_dir.is_file() {
        return Err(Code::FAILURE.with_message(format!(
            "The specified dest_dir of {} is a file. Not overwriting",
            dest_dir.display()
        )));
    } else {
        std::fs::create_dir_all(&dest_dir).map_err(|e| {
            Code::FAILURE.with_message(format!(
                "Failed to create dest_dir {dest_dir}: {e}",
                dest_dir = dest_dir.display()
            ))
        })?;
    }

    std::fs::copy(&src, &dst).map_err(|e| {
        Exit::new(Code::FAILURE).with_message(format!(
            "Failed to copy {src} to {dst}: {e}",
            src = src.display(),
            dst = dst.display()
        ))
    })?;
    eprintln!("Wrote the scie-jump to {}", dst.display());
    Ok(())
}

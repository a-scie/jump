use std::env::current_exe;

fn main() -> Result<(), String> {
    println!(
        "{}",
        scie_jump::message(
            &mut std::fs::OpenOptions::new()
                .read(true)
                .open(current_exe().map_err(|e| format!("{}", e))?)
                .map_err(|e| format!("{}", e))?
        )
    );
    Ok(())
}

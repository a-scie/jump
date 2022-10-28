fn main() -> Result<(), String> {
    Err(format!(
        "This packaging binary does nothing. Use {packaged_binary} instead",
        packaged_binary = env!("SCIE_STRAP"),
    ))
}

use std::path::PathBuf;

use logging_timer::time;

use crate::config::{Config, Scie};
use crate::jump;

const MAXIMUM_CONFIG_SIZE: usize = 0xFFFF;

#[time("debug")]
pub(crate) fn load(scie_path: PathBuf, scie_data: &[u8]) -> Result<Scie, String> {
    let end_of_zip = crate::zip::end_of_zip(scie_data, MAXIMUM_CONFIG_SIZE)?;
    let config_bytes = &scie_data[end_of_zip..];
    let config = Config::parse(config_bytes, &scie_path)?;

    let scie = config.scie;
    let jump = scie.jump.as_ref().ok_or_else(|| {
        format!("Loaded a lift manifest without the required jump manifest. Given {scie:?}")
    })?;

    if jump.version != jump::VERSION {
        return Err(format!(
            "The scie at {path} has a jump in its tip with version {version} but the lift \
                manifest declares {jump:?}",
            path = scie_path.display(),
            version = jump::VERSION,
        ));
    }
    Ok(scie)
}

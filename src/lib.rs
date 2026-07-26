use log::LevelFilter;
use semver::Version;

pub mod boot;

pub const VERSION: Version = Version::new(1, 13, 0);

pub fn init_logging() {
    env_logger::builder()
        .filter_level(LevelFilter::Warn)
        .parse_default_env()
        .init()
}

#[cfg(test)]
mod test {
    use semver::Version;

    use crate::VERSION;

    #[test]
    fn test_versions_consistent() {
        let cargo_manifest = env!("CARGO_MANIFEST_PATH");
        let manifest_version = Version::parse(env!("CARGO_PKG_VERSION"))
            .map_err(|e| format!("The version in manifest {cargo_manifest} is invalid: {e}"))
            .unwrap();
        assert_eq!(
            VERSION,
            manifest_version,
            "The version in the manifest at {cargo_manifest} is {manifest_version} which does not \
            match the version in {this_file} which is {VERSION}",
            this_file = file!()
        )
    }
}

use super::MbtxConfig;

use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;

#[test]
fn enabled_tool_rejects_missing_or_invalid_executables() {
    let mut config = MbtxConfig {
        enabled: true,
        ..Default::default()
    };
    assert!(
        config
            .validate()
            .expect_err("missing moon")
            .to_string()
            .contains("mbtx.moon is required")
    );
    let directory = tempfile::tempdir().expect("directory");
    config.moon = Some(AbsolutePathBuf::from_absolute_path(directory.path()).expect("absolute"));
    assert_eq!(
        config
            .validate()
            .expect_err("directory cannot execute")
            .kind(),
        std::io::ErrorKind::InvalidInput
    );
    config.moon = Some(config.moon.expect("path").join("missing"));
    assert_eq!(
        config.validate().expect_err("missing executable").kind(),
        std::io::ErrorKind::NotFound
    );
}

#[test]
fn unknown_configuration_keys_fail_explicitly() {
    assert!(toml::from_str::<MbtxConfig>("enabled = true\ntarget = 'native'").is_err());
}

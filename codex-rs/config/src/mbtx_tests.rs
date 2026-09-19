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

#[test]
fn shared_deadline_expires_and_is_never_extended_beyond_two_hours() {
    let mut config = MbtxConfig::default();
    assert_eq!(config.remaining_attempt_ms().expect("no deadline"), None);
    config.attempt_deadline_unix_ms = Some(1);
    assert_eq!(
        config.remaining_attempt_ms().expect_err("expired").kind(),
        std::io::ErrorKind::TimedOut
    );
    config.attempt_deadline_unix_ms = Some(u64::MAX);
    assert_eq!(
        config.remaining_attempt_ms().expect("ceiling"),
        Some(7_200_000)
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("epoch")
        .as_millis() as u64;
    config.attempt_deadline_unix_ms = Some(now + 60_000);
    assert!(matches!(
        config.remaining_attempt_ms(),
        Ok(Some(1..=60_000))
    ));
}

#[cfg(unix)]
#[test]
fn policy_requires_a_read_only_file_and_absolute_read_only_execution_path() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().expect("directory");
    let binary = directory.path().join("executable");
    std::fs::write(&binary, "fixture").expect("binary");
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let policy = directory.path().join("policy.json");
    std::fs::write(&policy, "{}").expect("policy");
    let utilities = directory.path().join("utilities");
    std::fs::create_dir(&utilities).expect("utilities");
    std::fs::set_permissions(&utilities, std::fs::Permissions::from_mode(0o555))
        .expect("read only");
    let executable = AbsolutePathBuf::from_absolute_path(binary).expect("absolute");
    let mut config = MbtxConfig {
        enabled: true,
        moon: Some(executable.clone()),
        moonrun: Some(executable),
        dependency_cache: Some(
            AbsolutePathBuf::from_absolute_path(directory.path()).expect("cache"),
        ),
        runtime_policy: Some(AbsolutePathBuf::from_absolute_path(&policy).expect("policy")),
        execution_path: Some(utilities.to_string_lossy().into_owned()),
        ..Default::default()
    };
    assert_eq!(
        config.validate().expect_err("writable policy").kind(),
        std::io::ErrorKind::InvalidInput
    );
    std::fs::set_permissions(&policy, std::fs::Permissions::from_mode(0o444)).expect("read only");
    config.validate().expect("valid policy configuration");
    for path in [
        None,
        Some(String::new()),
        Some(".".into()),
        Some(directory.path().to_string_lossy().into_owned()),
    ] {
        config.execution_path = path;
        assert_eq!(
            config.validate().expect_err("invalid PATH").kind(),
            std::io::ErrorKind::InvalidInput
        );
    }
}

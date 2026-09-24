use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

fn configuration(path: &Path, requests: u64, streams: u64) -> Result<RelayConfig> {
    fs::write(
        path,
        format!(
            r#"
model_provider = "local"
model = "test-model"
model_reasoning_effort = "xhigh"
[model_providers.local]
name = "Local fixture"
base_url = "http://127.0.0.1:1/v1"
wire_api = "responses"
env_key = "UNUSED_TEST_KEY"
requires_openai_auth = false
request_max_retries = {requests}
stream_max_retries = {streams}
"#
        ),
    )?;
    RelayConfig::load(path)
}

#[test]
fn independent_retry_limits_reach_both_arms_without_unbounded_fallback() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    fs::write(root.join("codex"), b"sandbox path fixture")?;
    fs::create_dir(root.join("worker-ipc"))?;
    fs::write(
        root.join("worker-socket.json"),
        serde_json::to_vec(&root.join("worker-ipc/s"))?,
    )?;
    for folder in ["workspace", "home", "tmp"] {
        fs::create_dir(root.join(folder))?;
    }
    let info =
        json!({"moon_home":root,"moon_path":root.join("moon"),"moonrun_path":root.join("moonrun")});
    for (requests, streams) in [(0, 2), (2, 0), (2, 2)] {
        let config = configuration(&root.join("relay.toml"), requests, streams)?;
        for arm in ["shell_tool", "mbtx_program"] {
            let text = child_config(
                &config,
                root,
                &info,
                AttemptContext {
                    arm,
                    endpoint: "http://127.0.0.1:1/a/test/v1",
                    attempt_id: "test",
                    instructions: "Fixture",
                    work: root,
                    evidence: root,
                },
            )?;
            let child: toml::Value = toml::from_str(&text)?;
            assert_eq!(
                child["features"]["unified_exec"].as_bool(),
                Some(arm == "shell_tool")
            );
            assert_eq!(
                json!({"requests":child["model_providers"]["local"]["request_max_retries"],
                    "streams":child["model_providers"]["local"]["stream_max_retries"],
                    "unbounded":child["features"]["unbounded_connection_retries"],
                    "websockets":child["model_providers"]["local"]["supports_websockets"]}),
                json!({"requests":requests,"streams":streams,"unbounded":false,"websockets":false}),
            );
        }
    }
    for (requests, streams) in [(3, 0), (0, 3), (u64::MAX, 0)] {
        assert!(configuration(&root.join("relay.toml"), requests, streams).is_err());
    }
    Ok(())
}

#[test]
fn condition_config_selects_catalog_and_records_tool_mode_metadata() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    fs::write(root.join("codex"), b"sandbox path fixture")?;
    fs::write(root.join("models-direct.json"), b"{}")?;
    fs::create_dir(root.join("worker-ipc"))?;
    fs::write(
        root.join("worker-socket.json"),
        serde_json::to_vec(&root.join("worker-ipc/s"))?,
    )?;
    for folder in ["workspace", "home", "tmp"] {
        fs::create_dir(root.join(folder))?;
    }
    let config = configuration(&root.join("relay.toml"), 1, 1)?;
    let info = json!({
        "moon_home":root,
        "moon_path":root.join("moon"),
        "moonrun_path":root.join("moonrun"),
        "conditions":{"conditions":[{
            "condition_id":"direct-mbtx-production",
            "backend_arm":"mbtx",
            "catalog_file":"models-direct.json",
            "requested_tool_mode":"direct",
            "effective_tool_mode":"direct",
            "shell_type":"unified_exec",
            "catalog_variant":"harness_direct",
            "catalog_sha256":"catalog",
            "policy_sha256":"policy",
            "capability_profile":"production"
        }]}
    });
    let rendered = child_config_for_condition(
        &config,
        root,
        &info,
        AttemptContext {
            arm: "mbtx_program",
            endpoint: "http://127.0.0.1:1/a/test/v1",
            attempt_id: "test",
            instructions: "Fixture",
            work: root,
            evidence: root,
        },
        "direct-mbtx-production",
    )?;
    let child: toml::Value = toml::from_str(&rendered)?;
    let expected_catalog = root
        .join("models-direct.json")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        child["model_catalog_json"].as_str(),
        Some(expected_catalog.as_str())
    );
    assert_eq!(child["features"]["shell_tool"].as_bool(), Some(false));
    assert!(rendered.contains("requested_tool_mode = direct"));
    assert!(rendered.contains("effective_tool_mode = direct"));
    Ok(())
}

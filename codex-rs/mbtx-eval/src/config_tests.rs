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
    for (requests, streams) in [(0, 2), (2, 0), (5, 5)] {
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
                json!({"requests":child["model_providers"]["local"]["request_max_retries"],
                    "streams":child["model_providers"]["local"]["stream_max_retries"],
                    "unbounded":child["features"]["unbounded_connection_retries"],
                    "websockets":child["model_providers"]["local"]["supports_websockets"]}),
                json!({"requests":requests,"streams":streams,"unbounded":false,"websockets":false}),
            );
        }
    }
    for (requests, streams) in [(6, 0), (0, 6), (u64::MAX, 0)] {
        assert!(configuration(&root.join("relay.toml"), requests, streams).is_err());
    }
    Ok(())
}

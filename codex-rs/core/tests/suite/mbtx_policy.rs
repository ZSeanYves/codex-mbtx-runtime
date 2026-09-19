//! Test the Codex host boundary with fixture executables. MoonRun's actual
//! allow/deny semantics are exercised separately by the evaluator preflight.

use super::*;
use codex_features::Feature;
use pretty_assertions::assert_eq;
use std::os::unix::fs::PermissionsExt;

enum Budget {
    SharedDeadline,
    ExpiredDeadline,
    PerCall,
}

async fn fixture_host(
    server: &MockServer,
    budget: Budget,
) -> Result<(TestCodex, tempfile::TempDir)> {
    let fixture = tempfile::tempdir()?;
    let compiler = fixture.path().join("moon");
    let runtime = fixture.path().join("moonrun");
    let policy = fixture.path().join("policy.json");
    let utilities = fixture.path().join("utilities");
    let dependencies = fixture.path().join("dependencies");
    std::fs::create_dir_all(dependencies.join("v1/sources"))?;
    std::fs::create_dir(dependencies.join(".moon-cache"))?;
    std::fs::create_dir(&utilities)?;
    std::fs::set_permissions(&utilities, std::fs::Permissions::from_mode(0o555))?;
    std::fs::write(&policy, "{\"process\":{\"allow\":[]}}\n")?;
    std::fs::set_permissions(&policy, std::fs::Permissions::from_mode(0o444))?;
    std::fs::write(
        &compiler,
        r#"#!/bin/sh
set -eu
/bin/sleep 0.05
/bin/mkdir -p "$7/program.mbtx/wasm/release/build/single"
/bin/cp "$8" "$7/program.mbtx/wasm/release/build/single/single.wasm"
"#,
    )?;
    let utility_path = utilities.to_string_lossy().into_owned();
    let quoted_path = shlex::try_quote(&utility_path)?;
    std::fs::write(
        &runtime,
        format!(
            r#"#!/bin/sh
set -eu
test "$1" = --policy
test "$3" = --
test "$PATH" = {quoted_path}
test -r "$2"
/bin/sleep 0.05
case "$(/bin/cat "$4")" in
  *deny*) printf 'Sandbox policy blocked process spawn\n' >&2; exit 1 ;;
  *) printf 'allowed\n' ;;
esac
"#
        ),
    )?;
    for binary in [&compiler, &runtime] {
        std::fs::set_permissions(binary, std::fs::Permissions::from_mode(0o755))?;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;
    let settings = MbtxConfig {
        enabled: true,
        moon: Some(AbsolutePathBuf::from_absolute_path(compiler)?),
        moonrun: Some(AbsolutePathBuf::from_absolute_path(runtime)?),
        dependency_cache: Some(AbsolutePathBuf::from_absolute_path(dependencies)?),
        runtime_policy: Some(AbsolutePathBuf::from_absolute_path(policy)?),
        execution_path: Some(utility_path),
        attempt_deadline_unix_ms: match budget {
            Budget::SharedDeadline => Some(now + 300_000),
            Budget::ExpiredDeadline => Some(1),
            Budget::PerCall => None,
        },
        ..Default::default()
    };
    settings.validate()?;
    let mut extensions = ExtensionRegistryBuilder::<Config>::new();
    codex_mbtx_extension::install(&mut extensions, |config: &Config| config.mbtx.clone());
    let test = test_codex()
        .with_extensions(Arc::new(extensions.build()))
        .with_config(move |config| {
            config.mbtx = settings;
            for feature in [Feature::ShellTool, Feature::UnifiedExec, Feature::CodeMode] {
                config
                    .features
                    .disable(feature)
                    .expect("disable shell tools");
            }
        })
        .build_with_auto_env(server)
        .await?;
    Ok((test, fixture))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_policy_and_path_are_applied_on_cache_hits_and_denials_remain_diagnostics()
-> Result<()> {
    core_test_support::skip_if_remote!(
        Ok(()),
        "MBTX host execution only supports local Linux/macOS"
    );
    let server = responses::start_mock_server().await;
    let (test, _fixture) = fixture_host(&server, Budget::SharedDeadline).await?;
    let mut replies = Vec::new();
    for (id, source) in [("first", "allow"), ("cached", "allow"), ("denied", "deny")] {
        replies.push(responses::sse(vec![
            responses::ev_response_created(id),
            responses::ev_function_call(
                id,
                "mbtx",
                &json!({
                    "source":source, "build_timeout_ms":1, "run_timeout_ms":1,
                })
                .to_string(),
            ),
            responses::ev_completed(id),
        ]));
    }
    replies.push(responses::sse(vec![
        responses::ev_response_created("override"),
        responses::ev_function_call(
            "override",
            "mbtx",
            &json!({
                "source":"allow", "runtime_policy":"/tmp/model-policy.json",
            })
            .to_string(),
        ),
        responses::ev_completed("override"),
    ]));
    replies.push(responses::sse(vec![
        responses::ev_response_created("done"),
        responses::ev_assistant_message("answer", "done"),
        responses::ev_completed("done"),
    ]));
    let mock = responses::mount_sse_sequence(&server, replies).await;
    test.submit_turn("Execute the fixture programs").await?;
    let request = mock.last_request().context("final request")?;
    let mut observed = Vec::new();
    let digest = "1744b96f140c52f1008aeac4c1a104456d5768b0b8f2d08c1a7e9caabdc848ca";
    for id in ["first", "cached", "denied"] {
        let (text, _) = request
            .function_call_output_content_and_success(id)
            .context("tool output")?;
        let result: Value = serde_json::from_str(&text.context("JSON output")?)?;
        assert_eq!(result["policy_sha256"], digest);
        observed.push(json!({
            "status":result["status"], "cache":result["cache"],
            "stdout":result["run"]["stdout"], "stderr":result["run"]["stderr"],
            "diagnostic":result["process_denial_diagnostic"],
        }));
    }
    assert_eq!(
        observed,
        vec![
            json!({"status":"success", "cache":"cold", "stdout":"allowed\n", "stderr":"", "diagnostic":null}),
            json!({"status":"success", "cache":"hit", "stdout":"allowed\n", "stderr":"", "diagnostic":null}),
            json!({"status":"error", "cache":"dependencies_reused", "stdout":"", "stderr":"Sandbox policy blocked process spawn\n", "diagnostic":"Sandbox policy blocked process spawn"}),
        ]
    );
    let (rejection, _) = request
        .function_call_output_content_and_success("override")
        .context("rejected override")?;
    assert!(
        rejection
            .context("rejection")?
            .contains("unknown field `runtime_policy`")
    );
    let first = mock.requests()[0].body_json();
    let names: Vec<_> = first["tools"]
        .as_array()
        .context("tools")?
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.contains(&"mbtx"));
    for forbidden in ["shell", "shell_command", "exec_command", "write_stdin"] {
        assert!(!names.contains(&forbidden), "unexpected tool {forbidden}");
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expired_attempt_never_compiles_and_normal_calls_keep_their_timeout() -> Result<()> {
    core_test_support::skip_if_remote!(
        Ok(()),
        "MBTX host execution only supports local Linux/macOS"
    );
    for budget in [Budget::ExpiredDeadline, Budget::PerCall] {
        let server = responses::start_mock_server().await;
        let expired = matches!(budget, Budget::ExpiredDeadline);
        let (test, _fixture) = fixture_host(&server, budget).await?;
        let mock = replay(&server, json!({"source":"allow", "run_timeout_ms":20})).await;
        test.submit_turn("Execute the fixture program").await?;
        let result = outcome(&mock)?;
        assert_eq!(result["status"], "error", "{result:#}");
        if expired {
            assert_eq!(
                (result["build"].clone(), result["run"].clone()),
                (Value::Null, Value::Null)
            );
            assert!(
                result["error"]
                    .as_str()
                    .context("error")?
                    .contains("deadline expired")
            );
        } else {
            assert_eq!(result["build"]["exit_code"], 0, "{result:#}");
            assert_eq!(result["run"]["status"], "timed_out", "{result:#}");
        }
    }
    Ok(())
}

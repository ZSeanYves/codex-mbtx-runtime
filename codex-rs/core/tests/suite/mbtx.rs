//! Real Codex + MoonBit tests. Opt in after installing the documented toolchain;
//! all model responses are local fixed SSE, with no provider credentials.

use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use codex_config::mbtx::MbtxConfig;
use codex_core::TurnInputRequest;
use codex_core::config::Config;
use codex_extension_api::ExtensionRegistryBuilder;
use codex_protocol::models::PermissionProfile;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::ReviewDecision;
use codex_protocol::protocol::SandboxPolicy;
use codex_protocol::protocol::ThreadSettingsOverrides;
use codex_protocol::user_input::UserInput;
use codex_utils_absolute_path::AbsolutePathBuf;
use core_test_support::responses;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use wiremock::MockServer;

const CAPABILITIES: &str = include_str!("../../../../mbtx/fixtures/capabilities.mbtx");

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires MoonBit and cached async 0.21.3"]
async fn filename_reuses_compilation_but_executes_fresh_and_source_changes_invalidate() -> Result<()>
{
    let server = responses::start_mock_server().await;
    let test = configured(&server).await?;
    let source = "fn main { println(\"original 雪\") }";
    std::fs::write(test.workspace_path("saved.mbtx"), source)?;
    let mut replies = Vec::new();
    for (id, args) in [
        ("first", json!({"source":source})),
        ("again", json!({"filename":"saved.mbtx"})),
        (
            "changed",
            json!({"source":"fn main { println(\"changed λ\") }"}),
        ),
    ] {
        replies.push(responses::sse(vec![
            responses::ev_response_created(id),
            responses::ev_function_call(id, "mbtx", &args.to_string()),
            responses::ev_completed(id),
        ]));
    }
    replies.push(responses::sse(vec![
        responses::ev_response_created("final"),
        responses::ev_assistant_message("answer", "done"),
        responses::ev_completed("final"),
    ]));
    let mock = responses::mount_sse_sequence(&server, replies).await;
    test.submit_turn("Execute each supplied program, then finish")
        .await?;
    let last = mock.last_request().context("final request")?;
    let mut results = Vec::new();
    for id in ["first", "again", "changed"] {
        let (text, _) = last
            .function_call_output_content_and_success(id)
            .context("tool result")?;
        results.push(serde_json::from_str::<Value>(
            &text.context("JSON result")?,
        )?);
    }
    assert_eq!(
        results.iter().map(|r| &r["status"]).collect::<Vec<_>>(),
        vec![&json!("success"); 3],
        "{results:#?}"
    );
    assert_eq!(results[1]["cache"], "hit", "{results:#?}");
    assert_eq!(results[1]["build_reused_from"], "first");
    assert_eq!(results[1]["run"]["stdout"], "original 雪\n");
    assert_ne!(results[0]["artifact_path"], results[1]["artifact_path"]);
    assert_eq!(results[2]["cache"], "dependencies_reused");
    assert_eq!(results[2]["run"]["stdout"], "changed λ\n");
    for result in &results {
        assert_eq!(result["source_resource"]["complete"], true);
        assert_eq!(result["artifact_resource"]["complete"], true);
        assert!(
            result["artifact_resource"]["bytes"]
                .as_u64()
                .is_some_and(|bytes| bytes > 0)
        );
    }
    assert_eq!(
        results[0]["artifact_resource"]["sha256"],
        results[1]["artifact_resource"]["sha256"]
    );
    assert_ne!(
        results[0]["artifact_resource"]["resource_id"],
        results[1]["artifact_resource"]["resource_id"]
    );
    Ok(())
}

fn toolchain() -> Result<MbtxConfig> {
    let settings = MbtxConfig {
        enabled: true,
        moon: Some(AbsolutePathBuf::from_absolute_path(which::which("moon")?)?),
        moonrun: Some(AbsolutePathBuf::from_absolute_path(which::which(
            "moonrun",
        )?)?),
        dependency_cache: Some(AbsolutePathBuf::from_absolute_path(
            std::env::var_os("MOON_DEP_CACHE")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    std::path::PathBuf::from(std::env::var_os("HOME").expect("HOME"))
                        .join(".moon/cache/deps")
                }),
        )?),
        ..Default::default()
    };
    settings.validate()?;
    Ok(settings)
}

async fn configured(server: &MockServer) -> Result<TestCodex> {
    let settings = toolchain()?;
    let mut extensions = ExtensionRegistryBuilder::<Config>::new();
    codex_mbtx_extension::install(&mut extensions, |config: &Config| config.mbtx.clone());
    test_codex()
        .with_extensions(Arc::new(extensions.build()))
        .with_config(move |config| {
            config.mbtx = settings;
            config.mbtx.output_directory = Some(
                AbsolutePathBuf::from_absolute_path(
                    config.codex_home.join("test-output-resources"),
                )
                .expect("absolute capture path"),
            );
            config
                .permissions
                .shell_environment_policy
                .r#set
                .insert("MBTX_CAPABILITY_ENV".into(), "observed".into());
        })
        .build_with_auto_env(server)
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disabled_extension_is_absent_from_model_tools() -> Result<()> {
    let server = responses::start_mock_server().await;
    let mut extensions = ExtensionRegistryBuilder::<Config>::new();
    codex_mbtx_extension::install(&mut extensions, |config: &Config| config.mbtx.clone());
    let test = test_codex()
        .with_extensions(Arc::new(extensions.build()))
        .build_with_auto_env(&server)
        .await?;
    let mock = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("r1"),
            responses::ev_assistant_message("final", "done"),
            responses::ev_completed("r1"),
        ]),
    )
    .await;
    test.submit_turn("Say done").await?;
    assert!(
        !mock.single_request().body_json()["tools"]
            .as_array()
            .context("tools")?
            .iter()
            .any(|tool| tool["name"] == "mbtx")
    );
    Ok(())
}

async fn replay(server: &MockServer, arguments: Value) -> responses::ResponseMock {
    responses::mount_sse_sequence(
        server,
        vec![
            responses::sse(vec![
                responses::ev_response_created("r1"),
                responses::ev_function_call("mbtx-call", "mbtx", &arguments.to_string()),
                responses::ev_completed("r1"),
            ]),
            responses::sse(vec![
                responses::ev_response_created("r2"),
                responses::ev_assistant_message("final", "done"),
                responses::ev_completed("r2"),
            ]),
        ],
    )
    .await
}

fn outcome(mock: &responses::ResponseMock) -> Result<Value> {
    let request = mock.last_request().context("missing follow-up request")?;
    let (text, _) = request
        .function_call_output_content_and_success("mbtx-call")
        .context("missing MBTX result")?;
    Ok(serde_json::from_str(&text.context("missing result text")?)?)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires MoonBit and cached async 0.21.3; run the stage-one validation command"]
async fn inline_program_preserves_inputs_inside_codex_sandbox() -> Result<()> {
    let server = responses::start_mock_server().await;
    let test = configured(&server).await?;
    let cwd = test.workspace_path("working 雪 space");
    std::fs::create_dir(&cwd)?;
    let output_path = cwd.join("artifact.txt");
    let mock = replay(
        &server,
        json!({"source":CAPABILITIES, "cwd":cwd,
        "argv":[output_path, "", "λ $() ; | *"], "max_output_bytes":4096}),
    )
    .await;
    test.submit_turn_with_policy(
        "Run the capability program",
        SandboxPolicy::new_workspace_write_policy(),
    )
    .await?;
    let result = outcome(&mock)?;
    assert!(
        mock.requests()[0].body_json()["tools"]
            .as_array()
            .context("tools")?
            .iter()
            .any(|tool| tool["name"] == "mbtx")
    );
    assert_eq!(result["status"], "success", "{result:#}");
    let data: Value = serde_json::from_str(result["run"]["stdout"].as_str().context("stdout")?)?;
    assert_eq!(
        (
            data["argv"].clone(),
            data["env"].clone(),
            data["child"].clone()
        ),
        (
            json!(["", "λ $() ; | *"]),
            json!(["observed"]),
            json!("|λ $() ; | *")
        )
    );
    assert_eq!(result["run"]["stderr"], "separate stderr: λ雪\n");
    assert_eq!(result["run"]["signal"], Value::Null);
    assert_eq!(
        std::fs::read_to_string(output_path)?,
        "MoonBit UTF-8: λ雪\n"
    );
    assert_eq!(
        std::fs::read_to_string(result["source_path"].as_str().context("source reference")?)?,
        CAPABILITIES
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires MoonBit; run the stage-one validation command"]
async fn compile_failure_and_runtime_failure_are_distinct() -> Result<()> {
    for (source, failed_stage) in [
        ("fn main { this_is_not_defined() }", "compilation"),
        ("fn main { abort(\"runtime failure 雪\") }", "completed"),
    ] {
        let server = responses::start_mock_server().await;
        let test = configured(&server).await?;
        let mock = replay(&server, json!({"source":source})).await;
        test.submit_turn("Execute the exact supplied program")
            .await?;
        let result = outcome(&mock)?;
        assert_eq!(
            (result["status"].clone(), result["stage"].clone()),
            (json!("error"), json!(failed_stage)),
            "{result:#}"
        );
        assert_eq!(result["run"].is_null(), failed_stage == "compilation");
        assert_eq!(
            std::fs::read_to_string(result["source_path"].as_str().context("source")?)?,
            source
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires MoonBit; run the stage-one validation command"]
async fn approval_denial_prevents_compilation() -> Result<()> {
    let server = responses::start_mock_server().await;
    let test = configured(&server).await?;
    let mock = replay(
        &server,
        json!({"source":"fn main { println(\"must not run\") }"}),
    )
    .await;
    test.codex
        .start_or_steer_turn(
            TurnInputRequest::user_input(vec![UserInput::Text {
                text: "Run the submitted program".into(),
                text_elements: Vec::new(),
            }])
            .with_thread_settings(ThreadSettingsOverrides {
                approval_policy: Some(AskForApproval::UnlessTrusted),
                permission_profile: Some(PermissionProfile::Disabled),
                ..Default::default()
            }),
        )
        .await?;
    let event = wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::ExecApprovalRequest(_))
    })
    .await;
    let EventMsg::ExecApprovalRequest(approval) = event else {
        unreachable!()
    };
    test.codex
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::Denied {
                rejection: "denied by test".into(),
            },
        })
        .await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
    let result = outcome(&mock)?;
    assert_eq!(
        (
            result["stage"].clone(),
            result["build"].clone(),
            result["run"].clone()
        ),
        (json!("compilation"), Value::Null, Value::Null),
        "{result:#}"
    );
    assert!(
        result["error"]
            .as_str()
            .context("rejection")?
            .contains("Rejected")
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires MoonBit and cached async 0.21.3; run the stage-one validation command"]
async fn runtime_timeout_returns_observed_status() -> Result<()> {
    let server = responses::start_mock_server().await;
    let test = configured(&server).await?;
    let mock = replay(
        &server,
        json!({"source":r#"import { "moonbitlang/async@0.21.3" }
async fn main { println("ready"); @async.sleep(30000) }"#, "run_timeout_ms":500}),
    )
    .await;
    test.submit_turn("Execute the timeout program").await?;
    let result = outcome(&mock)?;
    assert_eq!(result["build"]["exit_code"], 0, "{result:#}");
    assert_eq!(
        (
            result["run"]["status"].clone(),
            result["run"]["exit_code"].clone(),
            result["run"]["signal"].clone()
        ),
        (json!("timed_out"), Value::Null, json!(15)),
        "{result:#}"
    );
    assert_eq!(result["run"]["stdout"], "ready\n");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires MoonBit and cached async 0.21.3; run the stage-one validation command"]
async fn real_interrupt_waits_for_reap_before_turn_aborted() -> Result<()> {
    let server = responses::start_mock_server().await;
    let test = configured(&server).await?;
    // STOP the runtime to force the TERM grace to expire. This detects the
    // former 100ms outer-turn abort cutting off the required 500ms cleanup.
    let source = r#"import { "moonbitlang/async@0.21.3", "moonbitlang/async@0.21.3/shell" }
async fn main {
  @shell.Cmd("/bin/sh", ["-c", "printf '%s %s' $$ $PPID > pids; kill -STOP $PPID; touch ready; sleep 30"]).run()
}"#;
    let _mock = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("r1"),
            responses::ev_function_call("mbtx-call", "mbtx", &json!({"source":source}).to_string()),
            responses::ev_completed("r1"),
        ]),
    )
    .await;
    test.codex
        .start_or_steer_turn(
            TurnInputRequest::user_input(vec![UserInput::Text {
                text: "Run the supplied program".into(),
                text_elements: Vec::new(),
            }])
            .with_thread_settings(ThreadSettingsOverrides {
                approval_policy: Some(AskForApproval::Never),
                permission_profile: Some(PermissionProfile::Disabled),
                ..Default::default()
            }),
        )
        .await?;
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        while !test.workspace_path("ready").exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .context("runtime readiness")?;
    let pids: Vec<i32> = std::fs::read_to_string(test.workspace_path("pids"))?
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    let [child, leader] = pids.as_slice() else {
        anyhow::bail!("missing process identities")
    };
    assert_eq!(unsafe { libc::getpgid(*child) }, *leader);
    let started = std::time::Instant::now();
    test.codex.submit(Op::Interrupt).await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnAborted(_))
    })
    .await;
    assert!(
        started.elapsed() >= std::time::Duration::from_millis(450),
        "cleanup was cut off before the TERM grace expired"
    );
    let waited = unsafe { libc::waitpid(*leader, std::ptr::null_mut(), libc::WNOHANG) };
    assert_eq!(
        (waited, std::io::Error::last_os_error().raw_os_error()),
        (-1, Some(libc::ECHILD))
    );
    let state = tokio::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &child.to_string()])
        .output()
        .await?;
    let state = String::from_utf8_lossy(&state.stdout);
    assert!(
        state.trim().is_empty() || state.trim().starts_with('Z'),
        "descendant still runs: {state}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires MoonBit and cached async 0.21.3; run the stage-one validation command"]
async fn program_cannot_write_a_file_denied_by_codex() -> Result<()> {
    for source in [
        r#"import { "moonbitlang/async@0.21.3", "moonbitlang/async@0.21.3/fs", "moonbitlang/core/env" }
async fn main { @fs.write_file(@env.args()[1], "must be denied") }"#,
        r#"import { "moonbitlang/async@0.21.3", "moonbitlang/async@0.21.3/shell", "moonbitlang/core/env" }
async fn main {
  @shell.Cmd("/bin/sh", ["-c", "printf forbidden > \"$1\"", "mbtx-child", @env.args()[1]]).run()
}"#,
    ] {
        let server = responses::start_mock_server().await;
        let test = configured(&server).await?;
        let denied = tempfile::tempdir()?;
        let target = denied.path().join("forbidden.txt");
        let mock = replay(&server, json!({"source":source,"argv":[target]})).await;
        let permission = PermissionProfile::from_legacy_sandbox_policy_for_cwd(
            &SandboxPolicy::new_workspace_write_policy(),
            test.config.cwd.as_path(),
        );
        let mut filesystem = permission.file_system_sandbox_policy();
        filesystem
            .entries
            .push(codex_protocol::permissions::FileSystemSandboxEntry {
                path: codex_protocol::permissions::FileSystemPath::Path {
                    path: AbsolutePathBuf::from_absolute_path(&target)?.into(),
                },
                access: codex_protocol::permissions::FileSystemAccessMode::Deny,
                missing_path_behavior: None,
            });
        let permission = PermissionProfile::from_runtime_permissions(
            &filesystem,
            codex_protocol::permissions::NetworkSandboxPolicy::Restricted,
        );
        test.submit_turn_with_permission_profile("Execute the submitted program", permission)
            .await?;
        let result = outcome(&mock)?;
        assert_eq!(result["build"]["exit_code"], 0, "{result:#}");
        assert_eq!(result["status"], "error", "{result:#}");
        assert!(
            !target.exists(),
            "sandbox denied write must have no side effect"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires rebuilt Codex CLI and MoonBit; run the stage-one validation command"]
async fn cli_registers_and_executes_the_program_tool() -> Result<()> {
    let server = responses::start_mock_server().await;
    let home = tempfile::tempdir()?;
    let cwd = tempfile::tempdir()?;
    let settings = toolchain()?;
    let config = format!(
        r#"model = "gpt-5.5"
model_provider = "fixture"
[features]
code_mode = false
[model_providers.fixture]
name = "Offline fixture"
base_url = "{}/v1"
wire_api = "responses"
requires_openai_auth = false
request_max_retries = 0
stream_max_retries = 0
[mbtx]
{}"#,
        server.uri(),
        toml::to_string(&settings)?
    );
    std::fs::write(home.path().join("config.toml"), config)?;
    let mock = replay(
        &server,
        json!({"source":"fn main { println(\"CLI-MBTX-OK\") }"}),
    )
    .await;
    let mut command = tokio::process::Command::new(codex_utils_cargo_bin::cargo_bin("codex")?);
    command
        .args([
            "exec",
            "--json",
            "--strict-config",
            "--skip-git-repo-check",
            "--ephemeral",
            "--ignore-rules",
            "-s",
            "workspace-write",
            "-C",
        ])
        .arg(cwd.path())
        .arg("Execute the supplied MBTX program")
        .env("CODEX_HOME", home.path())
        .env_remove("OPENAI_API_KEY")
        .env_remove("CODEX_API_KEY")
        .env_remove("CODEX_ACCESS_TOKEN")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let output =
        tokio::time::timeout(std::time::Duration::from_secs(60), command.output()).await??;
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        mock.requests()[0].body_json()["tools"]
            .as_array()
            .context("tools")?
            .iter()
            .any(|tool| tool["name"] == "mbtx")
    );
    let result = outcome(&mock)?;
    assert_eq!(
        (result["status"].clone(), result["run"]["stdout"].clone()),
        (json!("success"), json!("CLI-MBTX-OK\n")),
        "{result:#}"
    );
    Ok(())
}

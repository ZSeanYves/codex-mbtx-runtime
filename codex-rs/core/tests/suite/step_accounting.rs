//! Fixed local SSE validation of loop identities; never an agent-benefit benchmark.
//! The validation entry sets a unique trace root in the test process environment.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use codex_config::mbtx::MbtxConfig;
use codex_core::TurnInputRequest;
use codex_core::config::Config;
use codex_extension_api::ExtensionRegistryBuilder;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::SandboxPolicy;
use codex_protocol::user_input::UserInput;
use codex_rollout_trace::RolloutTrace;
use codex_rollout_trace::StepObservation;
use codex_rollout_trace::StepOutcome;
use codex_rollout_trace::replay_bundle;
use codex_utils_absolute_path::AbsolutePathBuf;
use core_test_support::responses;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use wiremock::ResponseTemplate;

fn final_response(id: &str) -> ResponseTemplate {
    responses::sse_response(responses::sse(vec![
        responses::ev_response_created(id),
        responses::ev_assistant_message(&format!("message-{id}"), "done"),
        responses::ev_completed(id),
    ]))
}

// Expected values are handwritten per trajectory, independently of analysis code.
fn record_expectation(
    test: &TestCodex,
    name: &str,
    expected: Value,
) -> Result<(PathBuf, RolloutTrace)> {
    let root = std::env::var_os("CODEX_ROLLOUT_TRACE_ROOT")
        .context("run moon run mbtx/scripts/validate-evaluation.mbtx")?;
    let suffix = format!("-{}", test.session_configured.thread_id);
    let mut candidates = std::fs::read_dir(root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(&suffix))
                && !path.join("expected-steps.json").exists()
        })
        .collect::<Vec<_>>();
    assert_eq!(candidates.len(), 1, "one fresh trace per capture");
    let bundle = candidates.pop().expect("trace bundle");
    let raw = std::fs::read(bundle.join("trace.jsonl"))?;
    let trace = replay_bundle(&bundle)?;
    let observed = [
        trace
            .step_events
            .iter()
            .filter(|event| matches!(event.observation, StepObservation::Started { .. }))
            .count(),
        trace
            .step_events
            .iter()
            .filter(|event| {
                matches!(
                    event.observation,
                    StepObservation::Finished {
                        outcome: StepOutcome::Accepted,
                        ..
                    }
                )
            })
            .count(),
        trace
            .step_events
            .iter()
            .filter(|event| matches!(event.observation, StepObservation::HttpStarted { .. }))
            .count(),
        trace
            .step_events
            .iter()
            .filter(|event| matches!(event.observation, StepObservation::ToolEmitted { .. }))
            .count(),
        trace
            .step_events
            .iter()
            .filter(|event| matches!(event.observation, StepObservation::ToolDispatched { .. }))
            .count(),
    ];
    assert_eq!(
        observed,
        [
            "agent_steps_started",
            "agent_steps",
            "model_requests",
            "tool_calls",
            "tool_executions"
        ]
        .map(|key| expected[key].as_u64().expect("expected integer") as usize),
        "{name}"
    );
    assert_eq!(trace, replay_bundle(&bundle)?, "reduction is deterministic");
    assert_eq!(std::fs::read(bundle.join("trace.jsonl"))?, raw);
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(bundle.join("expected-steps.json"))?;
    serde_json::to_writer_pretty(file, &json!({"case":name,"observed":expected}))?;
    Ok((bundle, trace))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a fresh trace root; run mbtx/scripts/validate-evaluation.mbtx"]
async fn multiple_shell_tools_and_final_answer_are_two_steps() -> Result<()> {
    let server = responses::start_mock_server().await;
    let mut events = vec![responses::ev_response_created("tools")];
    for id in ["a", "b", "c"] {
        events.push(responses::ev_function_call(
            id,
            "exec_command",
            &json!({"cmd":format!("printf '{id}\\n'"),"login":false}).to_string(),
        ));
    }
    events.push(responses::ev_completed("tools"));
    let mock = responses::mount_response_sequence(
        &server,
        vec![
            responses::sse_response(responses::sse(events)),
            final_response("final"),
        ],
    )
    .await;
    let test = test_codex().build_with_auto_env(&server).await?;
    test.submit_turn("Run three commands and finish").await?;
    test.codex.shutdown_and_wait().await?;
    assert_eq!(mock.requests().len(), 2);
    for id in ["a", "b", "c"] {
        let (output, _) = mock.requests()[1]
            .function_call_output_content_and_success(id)
            .context("successful command output returned to model")?;
        let output = output.context("command output")?;
        assert!(output.contains("Process exited with code 0"), "{output}");
        assert!(output.ends_with(&format!("{id}\n")), "{output}");
    }
    record_expectation(
        &test,
        "shell-multiple-tools",
        json!({"agent_steps_started":2,"agent_steps":2,"model_requests":2,"tool_calls":3,"tool_executions":3,"tool_errors":0}),
    )?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a fresh trace root; run mbtx/scripts/validate-evaluation.mbtx"]
async fn http_retry_is_one_step_and_two_sends() -> Result<()> {
    let server = responses::start_mock_server().await;
    let mock = responses::mount_response_sequence(
        &server,
        vec![
            ResponseTemplate::new(500).set_body_string("temporary failure"),
            final_response("after-retry"),
        ],
    )
    .await;
    let test = test_codex()
        .with_config(|config| {
            config.model_provider.request_max_retries = Some(1);
        })
        .build_with_auto_env(&server)
        .await?;
    test.submit_turn("Finish").await?;
    test.codex.shutdown_and_wait().await?;
    assert_eq!(mock.requests().len(), 2);
    let (_, trace) = record_expectation(
        &test,
        "http-retry",
        json!({"agent_steps_started":1,"agent_steps":1,"model_requests":2,"tool_calls":0,"tool_executions":0,"http_transport_retries":1}),
    )?;
    assert!(trace.step_events.iter().any(|event| matches!(
        event.observation,
        StepObservation::HttpFinished {
            status_code: Some(500),
            ..
        }
    )));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a fresh trace root; run mbtx/scripts/validate-evaluation.mbtx"]
async fn exhausted_429_retains_started_round_without_acceptance() -> Result<()> {
    let server = responses::start_mock_server().await;
    let mock = responses::mount_response_once(
        &server,
        ResponseTemplate::new(429).set_body_string("rate limited"),
    )
    .await;
    let test = test_codex()
        .with_config(|config| {
            config.model_provider.request_max_retries = Some(0);
            config.model_provider.stream_max_retries = Some(0);
        })
        .build_with_auto_env(&server)
        .await?;
    test.submit_turn("Finish").await?;
    test.codex.shutdown_and_wait().await?;
    assert_eq!(mock.requests().len(), 1);
    let (_, trace) = record_expectation(
        &test,
        "http-retry-exhausted",
        json!({"agent_steps_started":1,"agent_steps":0,"model_requests":1,"tool_calls":0,"tool_executions":0}),
    )?;
    assert!(trace.step_events.iter().any(|event| matches!(
        event.observation,
        StepObservation::Finished {
            outcome: StepOutcome::Failed,
            ..
        }
    )));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a fresh trace root; run mbtx/scripts/validate-evaluation.mbtx"]
async fn partial_stream_side_effect_and_retry_keep_one_round() -> Result<()> {
    let server = responses::start_mock_server().await;
    let partial = responses::sse_response(responses::sse(vec![
        responses::ev_response_created("partial"),
        responses::ev_function_call(
            "write-marker",
            "exec_command",
            &json!({"cmd":"printf observed > marker.txt","login":false}).to_string(),
        ),
    ]));
    let mock =
        responses::mount_response_sequence(&server, vec![partial, final_response("recovered")])
            .await;
    let test = test_codex()
        .with_config(|config| {
            config.model_provider.stream_max_retries = Some(1);
        })
        .build_with_auto_env(&server)
        .await?;
    test.submit_turn_with_policy(
        "Write the marker and finish",
        SandboxPolicy::new_workspace_write_policy(),
    )
    .await?;
    test.codex.shutdown_and_wait().await?;
    assert_eq!(mock.requests().len(), 2);
    assert_eq!(
        std::fs::read_to_string(test.config.cwd.join("marker.txt"))?,
        "observed"
    );
    let (_, trace) = record_expectation(
        &test,
        "partial-stream-with-side-effect",
        json!({"agent_steps_started":1,"agent_steps":1,"model_requests":2,"tool_calls":1,"tool_executions":1,"tool_errors":0}),
    )?;
    assert_eq!(trace.inference_calls.len(), 2);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a fresh trace root; run mbtx/scripts/validate-evaluation.mbtx"]
async fn interrupt_then_resume_does_not_recount_history() -> Result<()> {
    let server = responses::start_mock_server().await;
    let delayed = responses::mount_response_once(
        &server,
        final_response("cancelled").set_delay(Duration::from_secs(30)),
    )
    .await;
    let test = test_codex().build_with_auto_env(&server).await?;
    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "Wait".into(),
            text_elements: vec![],
        }]))
        .await?;
    tokio::time::timeout(Duration::from_secs(10), async {
        while delayed.requests().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    test.codex.submit(Op::Interrupt).await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnAborted(_))
    })
    .await;
    test.codex.shutdown_and_wait().await?;
    let (original, first) = record_expectation(
        &test,
        "interrupted-before-headers",
        json!({"agent_steps_started":1,"agent_steps":0,"model_requests":1,"tool_calls":0,"tool_executions":0}),
    )?;
    let original_bytes = std::fs::read(original.join("trace.jsonl"))?;
    server.reset().await;
    responses::mount_response_once(&server, final_response("resumed")).await;
    let resumed = test_codex()
        .resume(
            &server,
            Arc::clone(&test.home),
            test.session_configured
                .rollout_path
                .clone()
                .context("rollout")?,
        )
        .await?;
    resumed.submit_turn("Finish now").await?;
    resumed.codex.shutdown_and_wait().await?;
    let (_, second) = record_expectation(
        &resumed,
        "resumed-new-capture",
        json!({"agent_steps_started":1,"agent_steps":1,"model_requests":1,"tool_calls":0,"tool_executions":0}),
    )?;
    assert_ne!(first.trace_id, second.trace_id);
    assert_eq!(std::fs::read(original.join("trace.jsonl"))?, original_bytes);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a fresh trace root; run mbtx/scripts/validate-evaluation.mbtx"]
async fn manual_compaction_is_request_work_outside_task_steps() -> Result<()> {
    let server = responses::start_mock_server().await;
    let mock = responses::mount_response_sequence(
        &server,
        vec![
            final_response("before"),
            responses::sse_response(responses::sse(vec![
                responses::ev_response_created("summary"),
                json!({"type":"response.output_item.done","item":{"type":"compaction","encrypted_content":"stage-two-summary"}}),
                responses::ev_completed("summary"),
            ])),
            final_response("after"),
        ],
    )
    .await;
    let test = test_codex().build_with_auto_env(&server).await?;
    test.submit_turn("Begin").await?;
    test.codex.submit(Op::Compact).await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
    test.submit_turn("Finish").await?;
    test.codex.shutdown_and_wait().await?;
    assert_eq!(mock.requests().len(), 3);
    assert_eq!(
        mock.requests()[2].inputs_of_type("compaction")[0]["encrypted_content"],
        "stage-two-summary"
    );
    record_expectation(
        &test,
        "manual-compaction",
        json!({"agent_steps_started":2,"agent_steps":2,"model_requests":3,"tool_calls":0,"tool_executions":0,"compaction_requests":1}),
    )?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires MoonBit and a fresh trace root; run mbtx/scripts/validate-evaluation.mbtx"]
async fn mbtx_compiler_repair_is_a_new_model_decision() -> Result<()> {
    let server = responses::start_mock_server().await;
    let mut replies = Vec::new();
    for (id, source) in [
        ("bad", "fn main { missing_function() }"),
        ("fixed", "fn main { println(\"verified\") }"),
    ] {
        replies.push(responses::sse_response(responses::sse(vec![
            responses::ev_response_created(id),
            responses::ev_function_call(id, "mbtx", &json!({"source":source}).to_string()),
            responses::ev_completed(id),
        ])));
    }
    replies.push(final_response("done"));
    let mock = responses::mount_response_sequence(&server, replies).await;
    let settings = MbtxConfig {
        enabled: true,
        moon: Some(AbsolutePathBuf::from_absolute_path(which::which("moon")?)?),
        moonrun: Some(AbsolutePathBuf::from_absolute_path(which::which(
            "moonrun",
        )?)?),
        dependency_cache: Some(AbsolutePathBuf::from_absolute_path(
            PathBuf::from(std::env::var_os("HOME").context("HOME")?).join(".moon/cache/deps"),
        )?),
        ..Default::default()
    };
    let mut extensions = ExtensionRegistryBuilder::<Config>::new();
    codex_mbtx_extension::install(&mut extensions, |config: &Config| config.mbtx.clone());
    let test = test_codex()
        .with_extensions(Arc::new(extensions.build()))
        .with_config(move |config| {
            config.mbtx = settings;
        })
        .build_with_auto_env(&server)
        .await?;
    test.submit_turn("Run a program, repair the compilation error, and finish")
        .await?;
    test.codex.shutdown_and_wait().await?;
    assert_eq!(mock.requests().len(), 3);
    let (bad, _) = mock.requests()[1]
        .function_call_output_content_and_success("bad")
        .context("compile error returned to model")?;
    let bad: Value = serde_json::from_str(&bad.context("compiler output")?)?;
    assert_eq!(bad["status"], "error");
    let (fixed, _) = mock.requests()[2]
        .function_call_output_content_and_success("fixed")
        .context("fixed output")?;
    let fixed: Value = serde_json::from_str(&fixed.context("execution output")?)?;
    assert_eq!(fixed["run"]["stdout"], "verified\n");
    record_expectation(
        &test,
        "mbtx-compiler-repair",
        json!({"agent_steps_started":3,"agent_steps":3,"model_requests":3,"tool_calls":2,"tool_executions":2,"tool_errors":1}),
    )?;
    Ok(())
}

use std::io::Write;
use std::sync::Arc;

use anyhow::Result;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

use crate::AgentStepGuard;
use crate::InferenceTraceContext;
use crate::RawTraceEventContext;
use crate::RequestPurpose;
use crate::StepObservation;
use crate::StepOutcome;
use crate::TraceWriter;
use crate::replay_bundle;

#[test]
fn acceptance_is_terminal_and_replay_does_not_add_steps() -> Result<()> {
    let temp = TempDir::new()?;
    let writer = Arc::new(TraceWriter::create(
        temp.path(),
        "trace".into(),
        "rollout".into(),
        "thread".into(),
    )?);
    let guard = AgentStepGuard::start(Arc::clone(&writer), RawTraceEventContext::default());
    let context = guard.context();
    context.finish(
        StepOutcome::Accepted,
        Some("response"),
        /*reason*/ None,
    );
    context.finish(
        StepOutcome::Failed,
        /*response_id*/ None,
        Some("late tool failure"),
    );
    drop(guard);
    let trace = replay_bundle(temp.path())?;
    assert_eq!(trace.step_events.len(), 2);
    assert!(matches!(
        &trace.step_events[1].observation,
        StepObservation::Finished {
            outcome: StepOutcome::Accepted,
            ..
        }
    ));
    assert_eq!(trace, replay_bundle(temp.path())?);
    assert!(
        TraceWriter::create(
            temp.path(),
            "replacement".into(),
            "other".into(),
            "thread".into()
        )
        .is_err()
    );
    assert_eq!(trace, replay_bundle(temp.path())?);
    Ok(())
}

#[test]
fn dropped_scope_and_pending_http_remain_observed_incomplete() -> Result<()> {
    let temp = TempDir::new()?;
    let writer = Arc::new(TraceWriter::create(
        temp.path(),
        "trace".into(),
        "rollout".into(),
        "thread".into(),
    )?);
    let guard = AgentStepGuard::start(Arc::clone(&writer), RawTraceEventContext::default());
    let inference = InferenceTraceContext::enabled(
        writer,
        "thread".into(),
        "turn".into(),
        "model".into(),
        "provider".into(),
    )
    .with_agent_step(guard.context())
    .start_attempt();
    let http = inference.http_trace_context();
    http.started(0);
    http.finished(0, Some(429), Some("rate limited".into()));
    http.started(1);
    drop(guard);
    let trace = replay_bundle(temp.path())?;
    assert_eq!(trace.step_events.len(), 6);
    assert!(matches!(
        trace.step_events[1].observation,
        StepObservation::HttpScope {
            purpose: RequestPurpose::Task,
            ..
        }
    ));
    assert!(matches!(
        trace.step_events[5].observation,
        StepObservation::Finished {
            outcome: StepOutcome::Abandoned,
            ..
        }
    ));
    let mut log = std::fs::OpenOptions::new()
        .append(true)
        .open(temp.path().join("trace.jsonl"))?;
    // A crash may split a multibyte UTF-8 character as well as a JSON token.
    log.write_all(b"{\"reason\":\"\xe9")?;
    let partial = replay_bundle(temp.path())?;
    assert_eq!(partial.step_events, trace.step_events);
    assert_eq!(
        partial.replay_warnings,
        vec!["incomplete final event; replayed the intact prefix"]
    );
    // A complete malformed line is corruption, not a recoverable interrupted write.
    log.write_all(b"\n")?;
    assert!(replay_bundle(temp.path()).is_err());
    Ok(())
}

#[test]
fn duplicate_or_missing_raw_sequence_is_rejected() -> Result<()> {
    let temp = TempDir::new()?;
    let writer = Arc::new(TraceWriter::create(
        temp.path(),
        "trace".into(),
        "rollout".into(),
        "thread".into(),
    )?);
    drop(AgentStepGuard::start(
        writer,
        RawTraceEventContext::default(),
    ));
    let path = temp.path().join("trace.jsonl");
    let original = std::fs::read_to_string(&path)?;
    let first = original.lines().next().expect("start event");
    std::fs::write(&path, format!("{first}\n{original}"))?;
    assert!(replay_bundle(temp.path()).is_err());
    Ok(())
}

///|
test "layer evidence does not turn task failure or compiler diagnostics into proven cause" {
  let ambiguous = v3_attribution(
    { "outcome": { "exit_code": 1 }, "trace": Json::null() },
    {},
    {},
    "harness_error",
    0,
  )
  guard ambiguous
    is {
      "sandbox/launcher": { "confidence": String("unknown"), "cause": Null, .. },
      "Codex scheduling": { "confidence": String("unknown"), "cause": Null, .. },
      ..
    } else {
    fail(ambiguous.stringify())
  }
  let tools : Json = {
    "mbtx_result_coverage": true,
    "mbtx_compile_failures": 1,
    "mbtx_execution_failures": 0,
  }
  let recovered = v3_attribution(
    {},
    tools,
    { "success": true, "confidence": "observed" },
    "success",
    1,
  )
  guard recovered
    is {
      "relay/provider": {
        "external_failures": Number(1, ..),
        "cause": Null,
        ..
      },
      "MBTX compile/runtime": {
        "confidence": String("observed"),
        "compile_failures": Number(1, ..),
        "cause": Null,
        ..
      },
      ..
    } else {
    fail(recovered.stringify())
  }
  let mismatch = v3_attribution(
    {},
    tools,
    {
      "success": false,
      "confidence": "observed",
      "reason": "manifest mismatch",
    },
    "task_failure",
    0,
  )
  guard mismatch
    is {
      "fixture/oracle": { "confidence": String("observed"), "cause": Null, .. },
      ..
    } else {
    fail(mismatch.stringify())
  }
}

///|
test "validation timeout preserves completed model session without claiming success" {
  let task : Json = {
    "id": "test-v3",
    "cohort": "workflow",
    "complexity": "quick",
    "process_allow": [],
    "files": {},
    "output": "result.json",
    "expected": 42,
  }
  let result = analyze_attempt(task, {
    "trace": {
      "status": "completed",
      "inference_calls": {},
      "tool_calls": {},
      "step_events": [],
      "replay_warnings": [],
    },
    "outcome": {
      "termination": "wall_limit",
      "model_termination": "exited",
      "exit_code": 0,
    },
    "snapshot": { "files": { "result.json": "42" }, "errors": [] },
    "exchanges": [],
    "submission": { "status": "attempt_timeout", "timeout_phase": "case_2" },
  })
  guard result
    is {
      "status": String("censored"),
      "session_completed": True,
      "steps_to_success": Null,
      "layer_evidence": {
        "timeout": {
          "confidence": String("observed"),
          "deadline_reached": True,
          ..
        },
        ..
      },
      ..
    } else {
    fail(result.stringify())
  }
}

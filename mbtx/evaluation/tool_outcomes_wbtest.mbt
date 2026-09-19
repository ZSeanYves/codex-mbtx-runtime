///|
test "successful tool envelopes preserve child failures and missing MBTX evidence" {
  let trace : Json = {
    "tool_calls": {
      "shell": {
        "kind": { "type": "terminal" },
        "execution": { "status": "completed" },
      },
      "program": {
        "kind": { "type": "other", "name": "mbtx" },
        "execution": { "status": "failed" },
      },
      "edit": {
        "kind": { "type": "apply_patch" },
        "execution": { "status": "completed" },
      },
    },
    "terminal_operations": {
      "command": {
        "execution": { "status": "failed" },
        "result": { "exit_code": 127 },
      },
    },
  }
  let results : Json = {
    "program": {
      "output": {
        "status": "error",
        "stage": "compilation",
        "build": { "exit_code": 255 },
      },
    },
  }
  let known = tool_outcomes(trace, results, complete=true)
  guard known
    is {
      "mbtx_compile_failures": Number(1, ..),
      "mbtx_execution_failures": Number(0, ..),
      "shell_command_failures": Number(1, ..),
      "shared_edit_calls": Number(1, ..),
      "mbtx_result_coverage": True,
      ..
    } else {
    fail(known.stringify())
  }
  let missing = tool_outcomes(trace, {}, complete=true)
  guard missing
    is {
      "mbtx_compile_failures": Null,
      "mbtx_execution_failures": Null,
      "shell_command_failures": Number(1, ..),
      "mbtx_result_coverage": False,
      ..
    } else {
    fail(missing.stringify())
  }
  let partial = tool_outcomes(
    {
      "tool_calls": {},
      "terminal_operations": {
        "command": { "execution": { "status": "running" } },
      },
    },
    {},
    complete=false,
  )
  guard partial is { "shell_command_failures": Null, .. } else {
    fail(partial.stringify())
  }
}

///|
test "policy diagnostics do not become invented subprocess counts or compilations" {
  let trace : Json = {
    "tool_calls": {
      "call-a": { "kind": { "type": "other", "name": "mbtx" } },
      "call-b": { "kind": { "type": "other", "name": "mbtx" } },
    },
  }
  let results : Json = {
    "call-a": {
      "output": {
        "status": "success",
        "stage": "completed",
        "cache": "miss",
        "build": { "exit_code": 0 },
        "run": { "exit_code": 0 },
      },
    },
    "call-b": {
      "output": {
        "status": "error",
        "stage": "execution",
        "cache": "hit",
        "build": { "exit_code": 0 },
        "run": { "exit_code": 1 },
        "policy_sha256": "digest",
        "process_denial_diagnostic": "Sandbox policy blocked process spawn",
      },
    },
  }
  let actual = tool_outcomes(trace, results, complete=true)
  guard actual
    is {
      "mbtx_compilations": Number(1, ..),
      "mbtx_cache_hits": Number(1, ..),
      "mbtx_program_launches_observed": Number(2, ..),
      "child_process_count": Null,
      "process_denial_diagnostics_observed": [
        {
          "tool_call_id": String("call-b"),
          "confidence": String("observed"),
          "request_identity": Null,
          ..
        },
      ],
      ..
    } else {
    fail(actual.stringify())
  }
}

///|
test "worker exit receipts remain separate from total process counts" {
  let summary = worker_statistics([
    {
      "confidence": "observed",
      "event": { "action": "job", "phase": "started" },
    },
    {
      "confidence": "observed",
      "event": { "action": "job", "phase": "completed", "exit_code": 7 },
    },
    {
      "confidence": "inferred",
      "event": { "action": "job", "phase": "completed", "exit_code": 0 },
    },
    { "confidence": "unknown", "evidence_error": "missing receipt" },
  ])
  guard summary
    is {
      "by_action": {
        "job": {
          "starts_observed": Number(1, ..),
          "completions_observed": Number(1, ..),
          "exit_codes_observed": { "7": Number(1, ..), .. },
          ..
        },
        ..
      },
      "observation_errors": Number(1, ..),
      "all_child_process_count": Null,
      ..
    } else {
    fail(summary.stringify())
  }
}

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

///|
test "oracle rejects a plausible wrong answer and modified inputs" {
  let protocol = @evaluation.pilot_protocol()
  guard protocol is { "tasks": Array(tasks), .. } else { fail("tasks") }
  let task = tasks[1]
  guard task is { "files": Object(inputs), "expected": expected, .. } else {
    fail("task")
  }
  let files = inputs.copy()
  files["result.json"] = expected.stringify().to_json()
  let good = @evaluation.task_oracle(task, {
    "files": files.to_json(),
    "errors": [],
  })
  guard good is { "success": True, .. } else { fail("valid output rejected") }
  files["result.json"] = "{\"customers\":{\"alpha\":14,\"beta\":15},\"total\":30}".to_json()
  guard @evaluation.task_oracle(task, { "files": files.to_json(), "errors": [] })
    is { "success": False, .. } else {
    fail("wrong total accepted")
  }
  files["result.json"] = expected.stringify().to_json()
  files["orders.json"] = "[]".to_json()
  guard @evaluation.task_oracle(task, { "files": files.to_json(), "errors": [] })
    is { "success": False, .. } else {
    fail("input mutation accepted")
  }
  guard @evaluation.task_oracle(task, { "files": {}, "errors": ["unreadable"] })
    is { "success": Null, .. } else {
    fail("missing evidence must be unknown")
  }
}

///|
test "relay error preserves successful oracle without becoming an MBTX error" {
  guard @evaluation.pilot_protocol() is { "tasks": Array(tasks), .. } else {
    fail("tasks")
  }
  let task = tasks[5]
  let facts : Json = {
    "attempt_id": "a",
    "pair_id": "p",
    "arm": "mbtx_program",
    "integrity": true,
    "trace": Json::null(),
    "snapshot": {
      "files": { "settings.json": "{\"port\":8080,\"retries\":2}" },
      "errors": [],
    },
    "outcome": { "termination": "exited", "exit_code": 1 },
    "exchanges": [{ "status_code": 429, "transport_error": false }],
    "evidence": {},
  }
  let report = @evaluation.analyze_attempt(task, facts)
  guard report
    is {
      "status": String("relay_error"),
      "oracle": { "success": True, .. },
      "accounting": { "metrics": { "agent_steps": Null, .. }, .. },
      ..
    } else {
    fail(report.stringify())
  }
}

///|
test "infrastructure gate counts final results and preserves isolated failures" {
  let failed : Json = { "status": "relay_error" }
  let ok : Json = { "status": "success" }
  guard @evaluation.collection_gate([failed, failed, failed, failed])
    is { "pause": False, .. } else {
    fail("premature pause")
  }
  guard @evaluation.collection_gate([failed, failed, failed, failed, failed])
    is { "pause": True, .. } else {
    fail("consecutive failure gate")
  }
  let mixed : Array[Json] = []
  for _ in 0..<8 {
    mixed.push(failed)
    mixed.push(ok)
  }
  guard @evaluation.collection_gate(mixed) is { "pause": True, .. } else {
    fail("rolling failure gate")
  }
}

///|
test "scheduled but unfinished arms remain in ITT" {
  let manifest : Json = {
    "run_id": "r",
    "mode": "replay",
    "platform": "linux",
    "protocol": { "protocol_id": "test" },
    "schedule": [{ "pair_id": "p", "task_id": "t" }],
  }
  let result = @evaluation.analyze_experiment(manifest, [])
  guard result
    is {
      "partial": True,
      "itt": {
        "shell_tool": {
          "assigned": Number(1, ..),
          "captured": Number(0, ..),
          ..
        },
        ..
      },
      "conditional": { "mean_step_difference": Null, .. },
      ..
    } else {
    fail(result.stringify())
  }
}

///|
test "incomplete Responses and local evidence errors have distinct attribution" {
  let task : Json = {
    "id": "t",
    "files": {},
    "output": "result.json",
    "expected": 42,
  }
  let cases : Array[(Json, String)] = [
    (
      {
        "status_code": 200,
        "complete": true,
        "sse_expected": true,
        "sse_terminal": Json::null(),
      },
      "relay_error",
    ),
    (
      { "status_code": 200, "transport_error": true, "evidence_error": true },
      "harness_error",
    ),
  ]
  for (exchange, expected) in cases {
    let report = @evaluation.analyze_attempt(task, {
      "attempt_id": "a",
      "pair_id": "p",
      "arm": "mbtx_program",
      "integrity": true,
      "trace": Json::null(),
      "snapshot": { "files": {}, "errors": [] },
      "outcome": { "termination": "exited", "exit_code": 1 },
      "exchanges": [exchange],
      "evidence": {},
    })
    guard report is { "status": String(actual), .. } else { fail("status") }
    assert_eq(actual, expected)
  }
}

///|
test "native starts and upstream sends remain distinct when no headers arrive" {
  let report = @evaluation.analyze_attempt(Json::null(), {
    "trace": Json::null(),
    "exchanges": [
      { "send_ns": 17, "transport_error": true },
      { "headers": { "send_ns": 23 } },
      { "complete": false },
    ],
    "outcome": { "termination": "exited", "exit_code": 1 },
    "integrity": true,
  })
  guard report
    is {
      "status": String("relay_error"),
      "upstream_requests_observed": Number(2, ..),
      "accounting": { "metrics": { "model_requests": Null, .. }, .. },
      ..
    } else {
    fail(report.stringify())
  }
}

///|
test "local cancellation before response headers preserves unknown external status" {
  for termination in ["timeout", "cancelled"] {
    let exchange : Json = {
      "status_code": Json::null(),
      "transport_error": Json::null(),
      "complete": false,
      "cancelled": true,
      "phase": "awaiting_headers",
      "send_ns": 17,
      "end_ns": 101,
      "first_byte_ns": Json::null(),
    }
    let report = @evaluation.analyze_attempt(Json::null(), {
      "trace": Json::null(),
      "exchanges": [exchange],
      "outcome": { "termination": termination.to_json(), "exit_code": 1 },
      "integrity": true,
    })
    guard report
      is {
        "status": String(actual),
        "upstream_requests_observed": Number(1, ..),
        "exchanges": [retained],
        ..
      } else {
      fail(report.stringify())
    }
    assert_eq(actual, termination)
    assert_eq(retained, exchange)
  }
}

///|
test "a successful arm with missing step coverage is retained outside paired analysis" {
  let manifest : Json = {
    "run_id": "r",
    "mode": "relay",
    "platform": "linux",
    "protocol": { "protocol_id": "test" },
    "schedule": [{ "pair_id": "p", "task_id": "t" }],
  }
  let arm = fn(name : String, covered : Bool, steps : Json) -> Json {
    {
      "attempt_id": name.to_json(),
      "pair_id": "p",
      "task_id": "t",
      "arm": name.to_json(),
      "status": "success",
      "integrity": true,
      "accounting": {
        "coverage": { "steps": covered.to_json() },
        "metrics": { "agent_steps": steps },
      },
    }
  }
  let report = @evaluation.analyze_experiment(manifest, [
    arm("shell_tool", true, (3).to_json()),
    arm("mbtx_program", false, Json::null()),
  ])
  guard report
    is {
      "itt": { "mbtx_program": { "successes": Number(1, ..), .. }, .. },
      "conditional": {
        "pairs": Number(0, ..),
        "mean_step_difference": Null,
        ..
      },
      ..
    } else {
    fail(report.stringify())
  }
}

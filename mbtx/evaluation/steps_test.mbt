///|
fn event(seq : Int, observation : Json) -> Json {
  {
    "seq": seq.to_json(),
    "thread_id": "thread",
    "codex_turn_id": "turn",
    "observation": observation,
  }
}

///|
fn capture(
  events : Array[Json],
  inferences? : Json = {},
  tools? : Json = {},
) -> Json {
  {
    "trace_id": "trace",
    "rollout_id": "rollout",
    "step_events": events.to_json(),
    "inference_calls": inferences,
    "tool_calls": tools,
    "replay_warnings": [],
    "status": "completed",
  }
}

///|
test "accepted final answer and multiple tools remain one response round" {
  let trace = capture([
    event(1, { "type": "started", "step_id": "s" }),
    event(2, {
      "type": "tool_emitted",
      "step_id": "s",
      "call_id": "a",
      "tool_name": "shell",
    }),
    event(3, {
      "type": "tool_emitted",
      "step_id": "s",
      "call_id": "b",
      "tool_name": "mbtx",
    }),
    event(4, { "type": "tool_dispatched", "step_id": "s", "tool_call_id": "a" }),
    event(5, { "type": "tool_dispatched", "step_id": "s", "tool_call_id": "b" }),
    event(6, {
      "type": "finished",
      "step_id": "s",
      "outcome": "accepted",
      "response_id": "r",
      "reason": null,
    }),
    event(7, { "type": "started", "step_id": "final" }),
    event(8, {
      "type": "finished",
      "step_id": "final",
      "outcome": "accepted",
      "response_id": "r2",
      "reason": null,
    }),
  ])
  let report = @evaluation.analyze_steps(trace)
  guard report
    is {
      "observed": {
        "agent_steps": Number(2, ..),
        "tool_calls": Number(2, ..),
        "tool_executions": Number(2, ..),
        ..
      },
      ..
    } else {
    fail(report.stringify())
  }
  assert_eq(report, @evaluation.analyze_steps(trace))
}

///|
test "HTTP retries and compaction requests do not add task steps" {
  let trace = capture(
    [
      event(1, { "type": "started", "step_id": "s" }),
      event(2, {
        "type": "inference_linked",
        "step_id": "s",
        "inference_call_id": "i",
        "purpose": "task",
      }),
      event(3, {
        "type": "http_scope",
        "scope_id": "h",
        "inference_call_id": "i",
        "compaction_request_id": null,
        "step_id": "s",
        "purpose": "task",
      }),
      event(4, {
        "type": "http_started",
        "scope_id": "h",
        "request_id": "h:0",
        "attempt": 0,
      }),
      event(5, {
        "type": "http_finished",
        "request_id": "h:0",
        "status_code": 429,
        "error": "rate limited",
      }),
      event(6, {
        "type": "http_started",
        "scope_id": "h",
        "request_id": "h:1",
        "attempt": 1,
      }),
      event(7, {
        "type": "http_finished",
        "request_id": "h:1",
        "status_code": 200,
        "error": null,
      }),
      event(8, {
        "type": "finished",
        "step_id": "s",
        "outcome": "accepted",
        "response_id": "r",
        "reason": null,
      }),
      event(9, {
        "type": "inference_linked",
        "step_id": null,
        "inference_call_id": "compact",
        "purpose": "compaction",
      }),
      event(10, {
        "type": "http_scope",
        "scope_id": "c",
        "inference_call_id": "compact",
        "compaction_request_id": null,
        "step_id": null,
        "purpose": "compaction",
      }),
      event(11, {
        "type": "http_started",
        "scope_id": "c",
        "request_id": "c:0",
        "attempt": 0,
      }),
    ],
    inferences={ "i": {}, "compact": {} },
  )
  let report = @evaluation.analyze_steps(trace)
  guard report
    is {
      "metrics": {
        "agent_steps": Number(1, ..),
        "model_requests": Number(3, ..),
        "http_transport_retries": Number(1, ..),
        ..
      },
      "observed": { "compaction_requests": Number(1, ..), .. },
      "requests": [_, _, { "status_code": Null, "ended_seq": Null, .. }],
      ..
    } else {
    fail(report.stringify())
  }
}

///|
test "missing request observation restricts request totals only" {
  let report = @evaluation.analyze_steps(
    capture(
      [
        event(1, { "type": "started", "step_id": "s" }),
        event(2, {
          "type": "inference_linked",
          "step_id": "s",
          "inference_call_id": "ws",
          "purpose": "task",
        }),
        event(3, {
          "type": "finished",
          "step_id": "s",
          "outcome": "accepted",
          "response_id": "r",
          "reason": null,
        }),
      ],
      inferences={ "ws": { "usage": null } },
    ),
  )
  guard report
    is {
      "metrics": {
        "agent_steps": Number(1, ..),
        "model_requests": Null,
        "repair_steps": Null,
        "process_spawns": Null,
        ..
      },
      ..
    } else {
    fail(report.stringify())
  }
}

///|
test "old and interrupted evidence never becomes zero-step success" {
  let old = @evaluation.analyze_steps(capture([]))
  guard old
    is { "metrics": { "agent_steps": Null, "task_success": Null, .. }, .. } else {
    fail(old.stringify())
  }
  let partial = @evaluation.analyze_steps(
    capture([event(1, { "type": "started", "step_id": "s" })]),
  )
  guard partial
    is {
      "metrics": { "agent_steps": Null, .. },
      "observed": {
        "agent_steps_started": Number(1, ..),
        "agent_steps": Number(0, ..),
        ..
      },
      ..
    } else {
    fail(partial.stringify())
  }
  let running = capture([
    event(1, { "type": "started", "step_id": "s" }),
    event(2, {
      "type": "finished",
      "step_id": "s",
      "outcome": "accepted",
      "response_id": "r",
      "reason": null,
    }),
  ])
  guard running is Object(fields) else { fail("expected object") }
  fields["status"] = Json::string("running")
  let report = @evaluation.analyze_steps(running)
  guard report
    is {
      "complete": False,
      "metrics": { "agent_steps": Null, .. },
      "observed": { "agent_steps": Number(1, ..), .. },
      ..
    } else {
    fail(report.stringify())
  }
}

///|
test "pending tool result is not zero errors and does not hide completed steps" {
  let report = @evaluation.analyze_steps(
    capture(
      [
        event(1, { "type": "started", "step_id": "s" }),
        event(2, {
          "type": "tool_dispatched",
          "step_id": "s",
          "tool_call_id": "pending",
        }),
        event(3, {
          "type": "finished",
          "step_id": "s",
          "outcome": "accepted",
          "response_id": "r",
          "reason": null,
        }),
      ],
      tools={ "pending": { "execution": { "status": "running" } } },
    ),
  )
  guard report
    is {
      "complete": False,
      "metrics": { "agent_steps": Number(1, ..), "tool_errors": Null, .. },
      "observed": { "tool_errors": Number(0, ..), .. },
      ..
    } else {
    fail(report.stringify())
  }
}

///|
test "failure followed by another round is only a possible repair" {
  let report = @evaluation.analyze_steps(
    capture(
      [
        event(1, { "type": "started", "step_id": "s" }),
        event(2, {
          "type": "finished",
          "step_id": "s",
          "outcome": "accepted",
          "response_id": "r",
          "reason": null,
        }),
        event(4, { "type": "started", "step_id": "next" }),
        event(5, {
          "type": "finished",
          "step_id": "next",
          "outcome": "accepted",
          "response_id": "r2",
          "reason": null,
        }),
      ],
      tools={
        "failed": {
          "thread_id": "thread",
          "execution": { "status": "failed", "ended_seq": 3 },
        },
      },
    ),
  )
  guard report
    is {
      "metrics": { "repair_steps": Null, .. },
      "possible_repairs": [
        {
          "step_id": "next",
          "confidence": "inferred",
          "source_seq": Number(3, ..),
          ..
        },
      ],
      ..
    } else {
    fail(report.stringify())
  }
}

///|
test "duplicate observation or terminal event is rejected" {
  for
    events in [
      [
        event(1, { "type": "started", "step_id": "s" }),
        event(1, { "type": "started", "step_id": "t" }),
      ],
      [
        event(1, { "type": "started", "step_id": "s" }),
        event(2, {
          "type": "finished",
          "step_id": "s",
          "outcome": "failed",
          "response_id": null,
          "reason": "error",
        }),
        event(3, {
          "type": "finished",
          "step_id": "s",
          "outcome": "accepted",
          "response_id": "r",
          "reason": null,
        }),
      ],
    ] {
    try @evaluation.analyze_steps(capture(events)) catch {
      _ => ()
    } noraise {
      _ => fail("expected malformed evidence rejection")
    }
  }
}

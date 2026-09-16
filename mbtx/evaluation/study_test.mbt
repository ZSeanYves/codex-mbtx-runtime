///|
test "saved literal answer cannot satisfy unseen input validation" {
  let task : Json = {
    "id": "t",
    "acceptance": "programs",
    "files": { "input.json": "[1]" },
    "output": "result.json",
    "expected": 1,
    "withheld": [
      {
        "files": { "input.json": "[2]" },
        "output": "result.json",
        "expected": 2,
      },
    ],
  }
  let process : Json = {
    "exit_code": 0,
    "timed_out": false,
    "wait_observed": true,
    "drain_complete": true,
    "residual_group_before_cleanup": false,
  }
  let snapshot : Json = {
    "files": { "input.json": "[1]", "result.json": "1" },
    "errors": [],
  }
  let base : Map[String, Json] = {
    "arm": "mbtx_program",
    "trace": Json::null(),
    "exchanges": [],
    "integrity": true,
    "snapshot": snapshot,
    "outcome": { "exit_code": 0, "termination": "exited" },
  }
  for (hidden_result, expected) in [("1", false), ("2", true)] {
    base["submission"] = {
      "status": "captured",
      "cases": [
        { "process": process, "snapshot": snapshot },
        {
          "process": process,
          "snapshot": {
            "files": {
              "input.json": "[2]",
              "result.json": hidden_result.to_json(),
            },
            "errors": [],
          },
        },
      ],
    }
    let actual = @evaluation.analyze_attempt(task, base.to_json())
    guard actual is { "oracle": { "success": success, .. }, .. } else {
      fail("oracle")
    }
    assert_eq(success, expected.to_json())
  }
  base["snapshot"] = {
    "files": { "input.json": "[3]", "result.json": "1" },
    "errors": [],
  }
  guard @evaluation.analyze_attempt(task, base.to_json())
    is { "status": String("task_failure"), .. } else {
    fail("visible input mutation was ignored")
  }
  base["submission"] = { "status": "harness_error", "cases": Json::null() }
  guard @evaluation.analyze_attempt(task, base.to_json())
    is { "status": String("harness_error"), .. } else {
    fail("validation infrastructure attributed to task")
  }
}

///|
test "missing program evidence and undrained streams remain unknown" {
  let task : Json = {
    "id": "t",
    "acceptance": "programs",
    "files": {},
    "output": "result.json",
    "expected": 1,
    "withheld": [],
  }
  let facts : Map[String, Json] = {
    "trace": Json::null(),
    "exchanges": [],
    "integrity": true,
    "snapshot": { "files": { "result.json": "1" }, "errors": [] },
    "outcome": { "exit_code": 0, "termination": "exited" },
  }
  guard @evaluation.analyze_attempt(task, facts.to_json())
    is { "status": String("unknown"), "oracle": { "success": Null, .. }, .. } else {
    fail("absent evidence became success")
  }
  facts["submission"] = {
    "status": "captured",
    "cases": [
      {
        "process": {
          "exit_code": 0,
          "wait_observed": true,
          "drain_complete": false,
        },
        "snapshot": { "files": { "result.json": "1" }, "errors": [] },
      },
    ],
  }
  guard @evaluation.analyze_attempt(task, facts.to_json())
    is { "oracle": { "success": Null, .. }, .. } else {
    fail("missing EOF became known")
  }
  facts["submission"] = {
    "status": "captured",
    "cases": [
      {
        "process": {
          "exit_code": 0,
          "wait_observed": true,
          "drain_complete": true,
          "residual_group_before_cleanup": Json::null(),
        },
        "snapshot": { "files": { "result.json": "1" }, "errors": [] },
      },
    ],
  }
  guard @evaluation.analyze_attempt(task, facts.to_json())
    is { "status": String("unknown"), "oracle": { "success": Null, .. }, .. } else {
    fail("unobserved process group became absent")
  }
  facts["submission"] = { "status": "incomplete_build", "cases": [] }
  guard @evaluation.analyze_attempt(task, facts.to_json())
    is { "status": String("unknown"), .. } else {
    fail("incomplete build became task failure")
  }
}

///|
test "nested paired uncertainty is deterministic and preserves missing coverage" {
  let pairs : Array[Json] = []
  let attempts : Array[Json] = []
  for variant in 0..<2 {
    for repeat in 0..<2 {
      let pair = "p\{variant}-\{repeat}"
      let task = "t\{variant}"
      pairs.push({
        "pair_id": pair.to_json(),
        "task_id": task.to_json(),
        "family": "data",
        "scenario": "aggregation",
        "variant": variant.to_json(),
        "repeat": repeat.to_json(),
      })
      for arm in ["shell_tool", "mbtx_program"] {
        let steps = if arm == "shell_tool" { 4 } else { 2 + 4 * variant }
        attempts.push({
          "attempt_id": (pair + arm).to_json(),
          "pair_id": pair.to_json(),
          "task_id": task.to_json(),
          "arm": arm.to_json(),
          "status": "success",
          "integrity": true,
          "workspace": { "baseline_commit": "identical" },
          "accounting": {
            "coverage": { "steps": true },
            "metrics": { "agent_steps": steps.to_json() },
          },
        })
      }
    }
  }
  let manifest : Json = {
    "protocol": { "suite": "programs" },
    "schedule": pairs.to_json(),
  }
  let report = @evaluation.analyze_experiment(manifest, attempts)
  assert_eq(report, @evaluation.analyze_experiment(manifest, attempts))
  guard report
    is {
      "conditional": {
        "pairs": Number(4, ..),
        "mean_step_difference": Number(0, ..),
        "confidence_interval": [Number(low, ..), Number(high, ..)],
        ..
      },
      ..
    } else {
    fail(report.stringify())
  }
  assert_true(low <= 0 && high >= 0)
  let _ = attempts.pop()
  let partial = @evaluation.analyze_experiment(manifest, attempts)
  guard partial
    is {
      "partial": True,
      "itt": {
        "mbtx_program": {
          "assigned": Number(4, ..),
          "captured": Number(3, ..),
          "not_started": Number(1, ..),
          ..
        },
        ..
      },
      "conditional": { "pairs": Number(3, ..), .. },
      ..
    } else {
    fail(partial.stringify())
  }
}

///|
test "unequal or unknown starting workspace excludes only strict pair analysis" {
  let schedule : Json = [
    {
      "pair_id": "p",
      "task_id": "t",
      "family": "data",
      "scenario": "aggregation",
      "variant": 0,
      "repeat": 0,
    },
  ]
  let shell : Json = {
    "pair_id": "p",
    "arm": "shell_tool",
    "status": "success",
    "integrity": true,
    "workspace": { "baseline_commit": "a" },
    "accounting": {
      "coverage": { "steps": true },
      "metrics": { "agent_steps": 4 },
    },
  }
  for
    (commit, equal, count) in [
      (Json::string("a"), true.to_json(), 1),
      (Json::string("b"), false.to_json(), 0),
      (Json::null(), Json::null(), 0),
    ] {
    let mbtx : Json = {
      "pair_id": "p",
      "arm": "mbtx_program",
      "status": "success",
      "integrity": true,
      "workspace": { "baseline_commit": commit },
      "accounting": {
        "coverage": { "steps": true },
        "metrics": { "agent_steps": 2 },
      },
    }
    let report = @evaluation.analyze_experiment(
      { "protocol": { "suite": "programs" }, "schedule": schedule },
      [shell, mbtx],
    )
    guard report
      is {
        "conditional": { "pairs": pairs, .. },
        "pairs": [{ "workspace_baseline_equal": observed, .. }],
        "itt": {
          "shell_tool": { "successes": Number(1, ..), .. },
          "mbtx_program": { "successes": Number(1, ..), .. },
          ..
        },
        ..
      } else {
      fail(report.stringify())
    }
    assert_eq(pairs, count.to_json())
    assert_eq(observed, equal)
  }
}

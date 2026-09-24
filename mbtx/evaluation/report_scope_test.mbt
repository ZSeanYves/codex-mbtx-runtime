///|
test "slot corrections count only assigned arms in totals strata and repetitions" {
  let manifest : Json = {
    "protocol": { "protocol_id": "programmable-shell-replacement-v4" },
    "schedule_selection": { "slots": ["p0/mbtx_program", "p1/mbtx_program"] },
    "schedule": [
      {
        "pair_id": "p0",
        "task_id": "t",
        "repeat": 0,
        "track": "basic",
        "family": "data",
        "arms": ["shell_tool", "mbtx_program"],
      },
      {
        "pair_id": "p1",
        "task_id": "t",
        "repeat": 1,
        "track": "basic",
        "family": "data",
        "arms": ["mbtx_program", "shell_tool"],
      },
      {
        "pair_id": "p2",
        "task_id": "excluded",
        "track": "complexity",
        "family": "repository",
        "arms": ["shell_tool", "mbtx_program"],
      },
    ],
  }
  let attempts : Array[Json] = []
  for pair_id in ["p0", "p1"] {
    attempts.push({
      "pair_id": pair_id.to_json(),
      "arm": "mbtx_program",
      "status": "success",
      "integrity": true,
      "accounting": {
        "coverage": { "steps": true },
        "metrics": { "agent_steps": 3 },
      },
    })
    let report = @evaluation.analyze_experiment(manifest, attempts)
    guard report
      is {
        "partial": partial,
        "itt": {
          "shell_tool": {
            "assigned": Number(0, ..),
            "captured": Number(0, ..),
            "not_started": Number(0, ..),
            "assigned_success_rate": Null,
            ..
          },
          "mbtx_program": {
            "assigned": Number(2, ..),
            "not_started": missing,
            "cumulative_success_by_observed_steps": [
              { "denominator": Number(2, ..), .. },
            ],
            ..
          },
          ..
        },
        "by_track": [
          {
            "name": String("basic"),
            "itt": {
              "shell_tool": { "assigned": Number(0, ..), .. },
              "mbtx_program": { "assigned": Number(2, ..), .. },
              ..
            },
            ..
          },
        ],
        "by_family": [{ "name": String("data"), .. }],
        "conditional": { "pairs": Number(0, ..), .. },
        "repeat_comparisons": [
          { "arm": String("mbtx_program"), "repetitions": [_, _], .. },
        ],
        "pairs": [
          {
            "pair_id": String("p0"),
            "assigned_arms": [String("mbtx_program")],
            "shell_status": String("not_assigned"),
            ..
          },
          {
            "pair_id": String("p1"),
            "assigned_arms": [String("mbtx_program")],
            ..
          },
          { "pair_id": String("p2"), "assigned_arms": [], .. },
        ],
        ..
      } else {
      fail(report.stringify())
    }
    assert_eq(partial, (attempts.length() < 2).to_json())
    assert_eq(missing, (2 - attempts.length()).to_json())
  }
}

///|
test "start pair and task filters are already reflected in the frozen schedule" {
  let report = @evaluation.analyze_experiment(
    {
      "protocol": {},
      "schedule_selection": { "start_pair": 5, "task_ids": ["selected-task"] },
      "schedule": [
        {
          "pair_id": "pair-0005",
          "task_id": "selected-task",
          "arms": ["shell_tool", "mbtx_program"],
        },
      ],
    },
    [],
  )
  guard report
    is {
      "partial": True,
      "itt": {
        "shell_tool": {
          "assigned": Number(1, ..),
          "not_started": Number(1, ..),
          ..
        },
        "mbtx_program": {
          "assigned": Number(1, ..),
          "not_started": Number(1, ..),
          ..
        },
        ..
      },
      "pairs": [
        {
          "pair_id": String("pair-0005"),
          "task_id": String("selected-task"),
          ..
        },
      ],
      ..
    } else {
    fail(report.stringify())
  }
}

///|
test "report rejects attempts outside the selected scope and duplicate slots" {
  let manifest : Json = {
    "protocol": {},
    "schedule_selection": { "slots": ["p/mbtx_program"] },
    "schedule": [{ "pair_id": "p", "arms": ["shell_tool", "mbtx_program"] }],
  }
  let mbtx : Json = { "pair_id": "p", "arm": "mbtx_program" }
  let cases : Array[Array[Json]] = [
    [{ "pair_id": "p", "arm": "shell_tool" }],
    [mbtx, mbtx],
  ]
  for attempts in cases {
    try @evaluation.analyze_experiment(manifest, attempts) catch {
      _ => ()
    } noraise {
      _ => fail("invalid attempt scope accepted")
    }
  }
}

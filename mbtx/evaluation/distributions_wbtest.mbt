///|
test "step distributions retain failures and leave missing totals unknown" {
  let attempts : Array[Json] = [
    {
      "attempt_id": "a",
      "task_id": "t",
      "pair_id": "p1",
      "complexity": "deep",
      "arm": "shell_tool",
      "status": "success",
      "steps_to_success": 51,
      "accounting": {
        "metrics": { "agent_steps": 51 },
        "observed": { "agent_steps": 51, "agent_steps_started": 51 },
      },
    },
    {
      "attempt_id": "b",
      "task_id": "t",
      "pair_id": "p2",
      "complexity": "deep",
      "arm": "shell_tool",
      "status": "task_failure",
      "steps_to_success": Json::null(),
      "accounting": {
        "metrics": { "agent_steps": 9 },
        "observed": { "agent_steps": 9, "agent_steps_started": 9 },
      },
    },
    {
      "attempt_id": "c",
      "task_id": "u",
      "pair_id": "p3",
      "complexity": "deep",
      "arm": "shell_tool",
      "status": "censored",
      "steps_to_success": Json::null(),
      "accounting": {
        "metrics": { "agent_steps": Json::null() },
        "observed": { "agent_steps": 3, "agent_steps_started": 4 },
      },
    },
  ]
  guard step_distributions(attempts, { "deep": [50, 100] })
    is [
      {
        "arm": String("shell_tool"),
        "captured": Number(3, ..),
        "known_totals": Number(2, ..),
        "unknown_totals": Number(1, ..),
        "median": Number(30, ..),
        "range": [Number(9, ..), Number(51, ..)],
        "values": [Number(9, ..), Number(51, ..)],
        "within_target": Number(1, ..),
        "observations": [
          _,
          _,
          {
            "steps": Null,
            "observed_steps": Number(3, ..),
            "observed_steps_started": Number(4, ..),
            "steps_to_success": Null,
            ..
          },
        ],
        ..
      },
      { "arm": String("mbtx_program"), "median": Null, "range": Null, .. },
    ] else {
    fail(
      "Known failures belong in the distribution; partial observations must remain partial",
    )
  }
  let pairs : Array[Json] = [
    { "task_id": "t", "pair_id": "p2", "repeat": 1 },
    { "task_id": "t", "pair_id": "p1", "repeat": 0 },
  ]
  guard repeat_comparisons(pairs, attempts)
    is [
      {
        "arm": String("shell_tool"),
        "first_two_step_difference": Number(-42, ..),
        "first_two_status_equal": False,
        ..
      },
      {
        "arm": String("mbtx_program"),
        "first_two_step_difference": Null,
        "first_two_status_equal": Null,
        ..
      },
    ] else {
    fail(
      "Repeated tasks must be aligned by task/arm/repeat and preserve missing evidence",
    )
  }
}

///|
test "contract preflight detects prompt omission and hidden grader fields" {
  let tasks : Array[Json] = @json.from_json(
    field(suite_natural_tool_choice(), "tasks"),
  )
  for task in tasks {
    assert_eq(output_contract_audit(task), {
      "status": "verified",
      "errors": [],
    })
    let fields = match task {
      Object(items) => items.copy()
      _ => abort("task")
    }
    let compact = "Output contract (identical for both execution backends):\n" +
      field(task, "output_contract").stringify()
    fields["output_contract_text"] = compact.to_json()
    fields["goal"] = ("Complete the workflow.\n" + compact).to_json()
    assert_eq(
      field(output_contract_audit(fields.to_json()), "status"),
      "verified".to_json(),
    )
    fields["output_contract_text"] = field(task, "output_contract_text")
    fields["goal"] = "Complete the workflow.".to_json()
    assert_eq(
      field(output_contract_audit(fields.to_json()), "status"),
      "invalid".to_json(),
    )
    fields["goal"] = field(task, "goal")
    let expected = match field(task, "expected") {
      Object(items) => items.copy()
      _ => abort("expected")
    }
    expected["hidden_requirement"] = true.to_json()
    fields["expected"] = expected.to_json()
    assert_eq(
      field(output_contract_audit(fields.to_json()), "status"),
      "invalid".to_json(),
    )
    fields["expected"] = field(task, "expected")
    fields["output_contract"] = Json::null()
    assert_eq(
      field(output_contract_audit(fields.to_json()), "status"),
      "invalid".to_json(),
    )
  }
}

///|
test "output mismatch distinguishes absent files missing fields and incorrect values" {
  assert_eq(artifact_mismatch("result.json", { "total": 3 }, Json::null()), {
    "success": false,
    "reason": "Required artifact is missing",
    "confidence": "observed",
    "artifact": "result.json",
    "differences": [{ "kind": "missing_file", "pointer": "" }],
    "difference_limit": 32,
  })
  let mismatch = artifact_mismatch(
    "result.json",
    { "total": 3 },
    "{\"overall\":3}".to_json(),
  )
  assert_eq(
    field(mismatch, "differences"),
    [
      { "kind": "missing_field", "pointer": "/total" },
      { "kind": "unexpected_field", "pointer": "/overall" },
    ].to_json(),
  )
  let result : Array[Json] = []
  output_differences({ "a/b": [1, 2] }, { "a/b": [1, 9] }, "", result)
  assert_eq(
    result.to_json(),
    [{ "kind": "value_mismatch", "pointer": "/a~1b/1" }].to_json(),
  )
}

///|
test "contract defects permit correction while model errors and unknowns do not" {
  let tasks : Array[Json] = @json.from_json(
    field(suite_natural_tool_choice(), "tasks"),
  )
  let task = tasks[0]
  let snapshot = object_field(task, "files").copy()
  snapshot["result.json"] = "{}".to_json()
  let facts : Map[String, Json] = {
    "trace": {
      "status": "completed",
      "inference_calls": {},
      "tool_calls": {},
      "step_events": [],
      "replay_warnings": [],
    },
    "integrity": true,
    "exchanges": [],
    "outcome": { "termination": "exited", "exit_code": 0 },
    "snapshot": { "files": snapshot.to_json(), "errors": [] },
  }
  let bad_task = match task {
    Object(items) => items.copy()
    _ => abort("task")
  }
  bad_task["goal"] = "Complete the workflow.".to_json()
  let invalid = analyze_attempt(bad_task.to_json(), facts.to_json())
  assert_eq(field(invalid, "failure_class"), "contract_configuration".to_json())
  assert_eq(field(invalid, "rerun_eligible"), true.to_json())
  let model = analyze_attempt(task, facts.to_json())
  assert_eq(field(model, "failure_class"), "model_execution".to_json())
  assert_eq(field(model, "rerun_eligible"), false.to_json())
  facts["snapshot"] = { "files": {}, "errors": ["unreadable"] }
  let unknown = analyze_attempt(task, facts.to_json())
  assert_eq(field(unknown, "failure_class"), "unknown".to_json())
  assert_eq(field(unknown, "rerun_eligible"), false.to_json())
}

///|
fn visible_test_snapshot(task : Json, executed : Json) -> Json raise {
  let snapshot = complex_test_snapshot(task)
  let files = object_field(snapshot, "files").copy()
  let result = object_field(task, "expected").copy()
  result["executed"] = executed
  files[string_field(task, "output")] = result.to_json().stringify().to_json()
  {
    "files": files.to_json(),
    "errors": [],
    "worker_events": field(snapshot, "worker_events"),
  }
}

///|
test "visible repeated delivery accepts initial empty and partial invocation lists" {
  let tasks : Array[Json] = @json.from_json(field(suite_v4("all"), "tasks"))
  for task in tasks {
    if field(task, "visible_result_policy") == Json::null() {
      continue
    }
    let required : Array[Json] = @json.from_json(
      field(field(task, "expected"), "executed"),
    )
    for
      executed in [required.to_json(), Json::array([]), [required[0]].to_json()] {
      let snapshot = visible_test_snapshot(task, executed)
      let actual = visible_task_oracle(task, snapshot)
      assert_eq(
        field(actual, "success"),
        true.to_json(),
        msg=string_field(task, "id"),
      )
      assert_eq(
        field(field(actual, "executed_claim"), "confidence"),
        Json::string("unknown"),
      )
      // The general task oracle remains exact even on a marked task.
      assert_eq(
        field(task_oracle(task, snapshot), "success"),
        (executed == required.to_json()).to_json(),
      )
    }
  }
}

///|
test "visible invocation subsets reject unknown duplicate unsorted and nonstring IDs" {
  let tasks : Array[Json] = @json.from_json(field(suite_v4("basic"), "tasks"))
  let task = tasks[9]
  let required : Array[Json] = @json.from_json(
    field(field(task, "expected"), "executed"),
  )
  for
    executed in [
      Json::array(["unknown-record"]),
      [required[0], required[0]].to_json(),
      [required[1], required[0]].to_json(),
      Json::array([(0).to_json()]),
      Json::string("not-an-array"),
    ] {
    assert_eq(
      field(
        visible_task_oracle(task, visible_test_snapshot(task, executed)),
        "success",
      ),
      false.to_json(),
    )
  }
}

///|
test "visible empty execution retains cumulative receipt and artifact requirements" {
  let tasks : Array[Json] = @json.from_json(field(suite_v4("basic"), "tasks"))
  let task = tasks[9]
  let snapshot = visible_test_snapshot(task, Json::array([]))
  let records : Array[Json] = @json.from_json(field(snapshot, "worker_events"))
  let requirements : Array[Json] = @json.from_json(field(task, "worker_events"))
  let extra = records.copy()
  extra.append(complex_test_receipts([requirements[0]], 90000)[:])
  for changed in [[], records[3:].to_owned(), extra] {
    assert_eq(
      field(
        visible_task_oracle(task, {
          "files": field(snapshot, "files"),
          "errors": [],
          "worker_events": changed.to_json(),
        }),
        "success",
      ),
      false.to_json(),
    )
  }
  for path in ["checkpoint.json", "records.json"] {
    let files = object_field(snapshot, "files").copy()
    files.remove(path)
    assert_eq(
      field(
        visible_task_oracle(task, {
          "files": files.to_json(),
          "errors": [],
          "worker_events": field(snapshot, "worker_events"),
        }),
        "success",
      ),
      false.to_json(),
    )
  }
  let files = object_field(snapshot, "files").copy()
  let result = @json.parse(string_field(files.to_json(), "result.json"))
  guard result is Object(result) else { fail("result object") }
  result["total"] = (-1).to_json()
  files["result.json"] = result.to_json().stringify().to_json()
  assert_eq(
    field(
      visible_task_oracle(task, {
        "files": files.to_json(),
        "errors": [],
        "worker_events": field(snapshot, "worker_events"),
      }),
      "success",
    ),
    false.to_json(),
  )
  assert_eq(
    visible_task_oracle(task, {
      "files": field(snapshot, "files"),
      "errors": ["unreadable state"],
      "worker_events": field(snapshot, "worker_events"),
    }),
    {
      "success": Json::null(),
      "reason": "Output snapshot incomplete",
      "confidence": "unknown",
    },
  )
}

///|
test "unmarked historical visible results retain exact comparison" {
  let tasks : Array[Json] = @json.from_json(field(suite_v4("basic"), "tasks"))
  let task = tasks[9]
  let historical = match task {
    Object(fields) => fields.copy()
    _ => abort("task object")
  }
  historical.remove("visible_result_policy")
  let snapshot = visible_test_snapshot(task, Json::array([]))
  assert_eq(
    visible_task_oracle(historical.to_json(), snapshot),
    task_oracle(historical.to_json(), snapshot),
  )
  assert_eq(
    field(visible_task_oracle(historical.to_json(), snapshot), "success"),
    false.to_json(),
  )
}

///|
test "attempt analysis permits cumulative visible results but rejects empty fresh first pass" {
  let tasks : Array[Json] = @json.from_json(field(suite_v4("basic"), "tasks"))
  let task = tasks[9]
  let inputs = [task]
  let withheld : Array[Json] = @json.from_json(field(task, "withheld"))
  inputs.append(withheld[:])
  let cases : Array[Json] = []
  for input in inputs {
    let phases : Array[Json] = @json.from_json(
      field(input, "validation_phases"),
    )
    let actual = phases.map(phase => complex_test_facts(phase))
    cases.push({ "phases": actual.to_json() })
  }
  let validation : Json = { "status": "captured", "cases": cases.to_json() }
  let facts : Map[String, Json] = {
    "snapshot": visible_test_snapshot(task, Json::array([])),
    "submission": validation,
    "exchanges": [],
    "outcome": { "termination": "exited", "exit_code": 0 },
  }
  let report = analyze_attempt(task, facts.to_json())
  assert_eq(field(field(report, "visible_oracle"), "success"), true.to_json())
  assert_eq(field(field(report, "oracle"), "success"), true.to_json())
  let phases : Array[Json] = @json.from_json(field(cases[0], "phases"))
  let first = match phases[0] {
    Object(fields) => fields.copy()
    _ => abort("phase facts")
  }
  first["snapshot"] = visible_test_snapshot(task, Json::array([]))
  phases[0] = first.to_json()
  cases[0] = { "phases": phases.to_json() }
  facts["submission"] = { "status": "captured", "cases": cases.to_json() }
  let report = analyze_attempt(task, facts.to_json())
  assert_eq(field(field(report, "visible_oracle"), "success"), true.to_json())
  assert_eq(field(field(report, "oracle"), "success"), false.to_json())
  assert_eq(field(report, "status"), Json::string("task_failure"))
}

///|
/// Oracle unit snapshots only: these synthetic receipts do not establish real
/// process execution. The separate offline reference replay verifies that layer.
fn complex_test_receipts(
  required : Array[Json],
  first_pid : Int,
) -> Array[Json] {
  let records : Array[Json] = []
  for index, expected in required {
    let args = if field(expected, "args") != Json::null() {
      field(expected, "args")
    } else {
      [field(expected, "action"), "selected.json", field(expected, "id")].to_json()
    }
    for phase in ["started", "operation", "completed"] {
      records.push({
        "pid": (first_pid + index).to_json(),
        "confidence": "observed",
        "event": {
          "phase": phase.to_json(),
          "args": args,
          "action": field(expected, "action"),
          "id": field(expected, "id"),
          "exit_code": if phase == "completed" {
            field(expected, "exit_code")
          } else {
            Json::null()
          },
        },
      })
    }
  }
  records
}

///|
/// Recompute job-plan values from public workflow inputs and selected records;
/// do not borrow the generated fixture's jobs or reference program output.
fn complex_test_plan(phase : Json, files : Map[String, Json]) -> Json raise {
  let workflow = @json.parse(
    string_field(field(phase, "files"), "workflow.json"),
  )
  let selected = @json.parse(
    string_field(files.to_json(), "records-selected.json"),
  )
  let records : Array[Json] = @json.from_json(field(selected, "records"))
  let nodes : Array[Json] = @json.from_json(field(workflow, "jobs"))
  let states : Map[String, Json] = Map([])
  let jobs : Array[Json] = []
  for _ in 0..<nodes.length() {
    for node in nodes {
      let id = string_field(node, "id")
      let deps : Array[String] = @json.from_json(field(node, "deps"))
      if states.contains(id) || !deps.all(dep => states.contains(dep)) {
        continue
      }
      let record = records
        .iter()
        .find_first(r => field(r, "id") == field(node, "record"))
      guard record is Some(record) else { continue }
      let input : Int = @json.from_json(field(record, "value"))
      let bias : Int = @json.from_json(field(node, "bias"))
      let revision : Int = @json.from_json(field(node, "revision"))
      let record_generation : Int = @json.from_json(field(record, "generation"))
      let value = input * input + bias
      let mut result = value
      let mut generation = record_generation + revision
      for dep in deps {
        result += @json.from_json(field(states[dep], "result"))
        generation += @json.from_json(field(states[dep], "generation"))
      }
      states[id] = {
        "id": id.to_json(),
        "generation": generation.to_json(),
        "result": result.to_json(),
      }
      jobs.push({
        "id": id.to_json(),
        "generation": generation.to_json(),
        "value": value.to_json(),
        "deps": deps.to_json(),
        "enabled": field(node, "enabled"),
        "fail_once": field(node, "fail_once"),
      })
    }
  }
  assert_eq(jobs.length(), nodes.length())
  { "jobs": jobs.to_json() }
}

///|
fn complex_test_snapshot(phase : Json) -> Json raise {
  let files = object_field(phase, "files").copy()
  for path, value in object_field(phase, "expected_outputs") {
    files[path] = match value {
      String(_) => value
      _ => value.stringify().to_json()
    }
  }
  let absent : Array[String] = @json.from_json(field(phase, "absent_outputs"))
  for path in absent {
    files.remove(path)
  }
  files["result.json"] = field(phase, "expected").stringify().to_json()
  if field(phase, "worker_plan_path") != Json::null() {
    files["jobs-selected.json"] = complex_test_plan(phase, files)
      .stringify()
      .to_json()
  }
  let required : Array[Json] = @json.from_json(field(phase, "worker_events"))
  {
    "files": files.to_json(),
    "errors": [],
    "worker_events": complex_test_receipts(required, 100).to_json(),
  }
}

///|
fn complex_test_facts(phase : Json) -> Json raise {
  {
    "snapshot": complex_test_snapshot(phase),
    "process": {
      "wait_observed": true,
      "drain_complete": true,
      "residual_group_before_cleanup": false,
      "exit_code": 0,
      "timed_out": false,
    },
    "input_preservation": { "success": true, "before": {}, "after": {} },
  }
}

///|
test "complexity oracle unit snapshots cover every tier visible withheld and retained phase" {
  let tasks : Array[Json] = @json.from_json(
    field(suite_v4("complexity"), "tasks"),
  )
  for task in tasks {
    let inputs = [task]
    let withheld : Array[Json] = @json.from_json(field(task, "withheld"))
    inputs.append(withheld[:])
    for input in inputs {
      let phases : Array[Json] = @json.from_json(
        field(input, "validation_phases"),
      )
      for index, phase in phases {
        let snapshot = complex_test_snapshot(phase)
        assert_eq(
          field(task_oracle(phase, snapshot), "success"),
          true.to_json(),
          msg=string_field(task, "id") + " phase \{index}",
        )
        // Correct summaries do not excuse corrupt checkpoints, states, plans,
        // manifests, dependency refs, or preserved cache artifacts.
        for path, _ in object_field(phase, "expected_outputs") {
          let files = object_field(snapshot, "files").copy()
          files[path] = "{}"
          let result = task_oracle(phase, {
            "files": files.to_json(),
            "errors": [],
            "worker_events": field(snapshot, "worker_events"),
          })
          assert_eq(
            field(result, "success"),
            false.to_json(),
            msg=string_field(task, "id") + ":" + path,
          )
        }
        let absent : Array[String] = @json.from_json(
          field(phase, "absent_outputs"),
        )
        for path in absent {
          let files = object_field(snapshot, "files").copy()
          files[path] = "retired state resurrected"
          assert_eq(
            field(
              task_oracle(phase, {
                "files": files.to_json(),
                "errors": [],
                "worker_events": field(snapshot, "worker_events"),
              }),
              "success",
            ),
            false.to_json(),
          )
        }
        let required : Array[Json] = @json.from_json(
          field(phase, "worker_events"),
        )
        if !required.is_empty() {
          assert_eq(
            field(
              task_oracle(phase, {
                "files": field(snapshot, "files"),
                "errors": [],
                "worker_events": [],
              }),
              "success",
            ),
            false.to_json(),
          )
        }
      }
    }
  }
}

///|
test "recovery rejects forged job plans even when final states and receipts look correct" {
  for tier in [0, 1, 2] {
    let phases : Array[Json] = @json.from_json(
      field(v4_recovery(tier, 47), "validation_phases"),
    )
    for phase in phases {
      let snapshot = complex_test_snapshot(phase)
      let plan = @json.parse(
        string_field(field(snapshot, "files"), "jobs-selected.json"),
      )
      let jobs : Array[Json] = @json.from_json(field(plan, "jobs"))
      for
        property in (if jobs.is_empty() {
          []
        } else {
          ["value", "generation", "deps", "enabled", "fail_once"]
        }) {
        let changed = jobs.copy()
        let first = match jobs[0] {
          Object(o) => o.copy()
          _ => abort("job")
        }
        first[property] = match property {
          "deps" => ["invented-dependency"]
          "enabled" => false.to_json()
          "fail_once" =>
            (!(@json.from_json(field(jobs[0], "fail_once")) : Bool)).to_json()
          _ => (-999).to_json()
        }
        changed[0] = first.to_json()
        let files = object_field(snapshot, "files").copy()
        files["jobs-selected.json"] = ({ "jobs": changed.to_json() } : Json)
          .stringify()
          .to_json()
        assert_eq(
          field(
            task_oracle(phase, {
              "files": files.to_json(),
              "errors": [],
              "worker_events": field(snapshot, "worker_events"),
            }),
            "success",
          ),
          false.to_json(),
          msg=property,
        )
      }
      let invalid_plans = [
        "{broken", "{}", "{\"jobs\":[{},{}]}", "{\"jobs\":[null,null]}",
      ]
      if !jobs.is_empty() {
        invalid_plans.push("{\"jobs\":[]}")
      }
      for invalid in invalid_plans {
        let files = object_field(snapshot, "files").copy()
        files["jobs-selected.json"] = invalid.to_json()
        assert_eq(
          field(
            task_oracle(phase, {
              "files": files.to_json(),
              "errors": [],
              "worker_events": field(snapshot, "worker_events"),
            }),
            "success",
          ),
          false.to_json(),
        )
      }
    }
  }
}

///|
test "operation receipts distinguish help parsing diagnostics from actual extra recovery and retries" {
  let task = v4_recovery(1, 47)
  let snapshot = complex_test_snapshot(task)
  let events : Array[Json] = @json.from_json(field(snapshot, "worker_events"))
  let diagnostic = complex_test_receipts(
    [
      {
        "action": "recover",
        "id": Json::null(),
        "args": ["recover", "--help"],
        "exit_code": 2,
      },
    ],
    9000,
  ).filter(r => field(field(r, "event"), "phase") != Json::string("operation"))
  let combined = events.copy()
  combined.append(diagnostic[:])
  assert_eq(
    field(
      task_oracle(task, {
        "files": field(snapshot, "files"),
        "errors": [],
        "worker_events": combined.to_json(),
      }),
      "success",
    ),
    true.to_json(),
  )
  let required : Array[Json] = @json.from_json(field(task, "worker_events"))
  for
    extra in [
      required[0],
      required
      .iter()
      .find_first(e => field(e, "exit_code") == (7).to_json())
      .unwrap(),
    ] {
    let combined = events.copy()
    combined.append(complex_test_receipts([extra], 9900)[:])
    assert_eq(
      field(
        task_oracle(task, {
          "files": field(snapshot, "files"),
          "errors": [],
          "worker_events": combined.to_json(),
        }),
        "success",
      ),
      false.to_json(),
    )
  }
  let no_boundary = events.filter(r => {
    field(field(r, "event"), "phase") != Json::string("operation")
  })
  assert_eq(
    field(
      task_oracle(task, {
        "files": field(snapshot, "files"),
        "errors": [],
        "worker_events": no_boundary.to_json(),
      }),
      "success",
    ),
    false.to_json(),
  )
  let incomplete = events[1:].to_owned()
  assert_eq(
    field(
      task_oracle(task, {
        "files": field(snapshot, "files"),
        "errors": [],
        "worker_events": incomplete.to_json(),
      }),
      "confidence",
    ),
    Json::string("unknown"),
  )
}

///|
test "out-of-scope worker receipts remain untrusted" {
  let task = v4_basic(5, 316)
  let snapshot = v4_snapshot(task)
  let records : Array[Json] = @json.from_json(field(snapshot, "worker_events"))
  let external = match records[0] {
    Object(record) => {
      let copy = record.copy()
      copy["scope"] = { "within_root": false, "cwd": "/tmp/model-copy" }
      copy.to_json()
    }
    _ => abort("record")
  }
  assert_eq(
    field(
      task_oracle(task, {
        "files": field(snapshot, "files"),
        "errors": [],
        "worker_events": [external].to_json(),
      }),
      "success",
    ),
    false.to_json(),
  )
  let combined = records.copy()
  combined.push(external)
  assert_eq(
    field(
      task_oracle(task, {
        "files": field(snapshot, "files"),
        "errors": [],
        "worker_events": combined.to_json(),
      }),
      "success",
    ),
    true.to_json(),
  )
}

///|
test "valid initial dependency state permits downstream execution without a new predecessor receipt" {
  let task : Json = {
    "files": {
      "jobs.json": "{\"jobs\":[{\"id\":\"cached\",\"deps\":[],\"enabled\":true,\"fail_once\":false},{\"id\":\"child\",\"deps\":[\"cached\"],\"enabled\":true,\"fail_once\":false}]}",
    },
    "expected_outputs": {},
    "absent_outputs": [],
    "strict_worker_receipts": true,
    "operation_receipts": true,
    "exact_worker_actions": ["job"],
    "initial_completed": ["cached"],
    "worker_events": [{ "action": "job", "id": "child", "exit_code": 0 }],
  }
  let records = complex_test_receipts(
    [{ "action": "job", "id": "child", "exit_code": 0 }],
    500,
  )
  assert_eq(
    field(
      long_oracle(task, { "files": {}, "worker_events": records.to_json() }),
      "success",
    ),
    true.to_json(),
  )
  let without_cache = match task {
    Object(o) => o.copy()
    _ => abort("task")
  }
  without_cache["initial_completed"] = []
  assert_eq(
    field(
      long_oracle(without_cache.to_json(), {
        "files": {},
        "worker_events": records.to_json(),
      }),
      "success",
    ),
    false.to_json(),
  )
}

///|
test "recovery no-change passes reject actual operations and dependency jobs cannot run early" {
  for tier in [0, 1, 2] {
    let phases : Array[Json] = @json.from_json(
      field(v4_recovery(tier, 89), "validation_phases"),
    )
    let repeated = phases[1]
    let snapshot = complex_test_snapshot(repeated)
    let initial_required : Array[Json] = @json.from_json(
      field(phases[0], "worker_events"),
    )
    assert_eq(
      field(
        task_oracle(repeated, {
          "files": field(snapshot, "files"),
          "errors": [],
          "worker_events": complex_test_receipts([initial_required[0]], 8000).to_json(),
        }),
        "success",
      ),
      false.to_json(),
    )
    if tier > 0 {
      let initial = complex_test_snapshot(phases[0])
      let records : Array[Json] = @json.from_json(
        field(initial, "worker_events"),
      )
      let first_job = records
        .iter()
        .find_first(r => {
          field(field(r, "event"), "action") == Json::string("job")
        })
        .unwrap()
      let pid = field(first_job, "pid")
      let early = records.filter(r => field(r, "pid") == pid)
      early.append(records.filter(r => field(r, "pid") != pid)[:])
      assert_eq(
        field(
          task_oracle(phases[0], {
            "files": field(initial, "files"),
            "errors": [],
            "worker_events": early.to_json(),
          }),
          "success",
        ),
        false.to_json(),
      )
    }
  }
}

///|
test "release hashing a different path cannot prove the chosen candidate and final target" {
  for tier in [0, 1, 2] {
    let phases : Array[Json] = @json.from_json(
      field(v4_release(tier, 47), "validation_phases"),
    )
    for phase in phases {
      let snapshot = complex_test_snapshot(phase)
      let required : Array[Json] = @json.from_json(
        field(phase, "worker_events"),
      )
      let altered = required.copy()
      let wrong = match altered[0] {
        Object(o) => o.copy()
        _ => abort("receipt")
      }
      wrong["args"] = ["hash", "candidates/wrong-path.dat"]
      altered[0] = wrong.to_json()
      assert_eq(
        field(
          task_oracle(phase, {
            "files": field(snapshot, "files"),
            "errors": [],
            "worker_events": complex_test_receipts(altered, 4000).to_json(),
          }),
          "success",
        ),
        false.to_json(),
      )
    }
  }
}

///|
test "submission missing phase or preservation evidence stays unknown and changed bytes fail" {
  for tier in [0, 1, 2] {
    for task in [v4_recovery(tier, 47), v4_release(tier, 47)] {
      let task_fields = match task {
        Object(o) => o.copy()
        _ => abort("task")
      }
      task_fields["withheld"] = []
      let task = task_fields.to_json()
      let phases : Array[Json] = @json.from_json(
        field(task, "validation_phases"),
      )
      let facts : Array[Json] = []
      for phase in phases {
        facts.push(complex_test_facts(phase))
      }
      let validation : Json = {
        "status": "captured",
        "cases": [{ "phases": facts.to_json() }],
      }
      assert_eq(
        field(submission_oracle(task, validation), "success"),
        true.to_json(),
      )
      let missing : Json = {
        "status": "captured",
        "cases": [
          { "phases": facts[0:facts.length() - 1].to_owned().to_json() },
        ],
      }
      assert_eq(
        field(submission_oracle(task, missing), "success"),
        Json::null(),
      )
      for
        preservation in [
          Json::null(),
          {
            "success": false,
            "before": { "cache": "abc" },
            "after": { "cache": "changed" },
          },
        ] {
        let altered = facts.copy()
        let mut changed_index = 0
        let mut requires_preservation = false
        for index, phase in phases {
          if field(phase, "preserve_previous") is Array(paths) &&
            !paths.is_empty() {
            changed_index = index
            requires_preservation = true
            break
          }
        }
        let first = match facts[changed_index] {
          Object(o) => o.copy()
          _ => abort("facts")
        }
        first["input_preservation"] = preservation
        altered[changed_index] = first.to_json()
        let result = submission_oracle(task, {
          "status": "captured",
          "cases": [{ "phases": altered.to_json() }],
        })
        assert_eq(
          field(result, "success"),
          if preservation == Json::null() {
            if requires_preservation {
              Json::null()
            } else {
              true.to_json()
            }
          } else {
            false.to_json()
          },
        )
      }
    }
  }
}

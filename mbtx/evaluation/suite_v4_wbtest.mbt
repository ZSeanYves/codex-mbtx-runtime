///|
fn v4_snapshot(task : Json) -> Json raise {
  let files = object_field(task, "files").copy()
  for path, expected in object_field(task, "expected_outputs") {
    files[path] = match expected {
      String(_) => expected
      _ => expected.stringify().to_json()
    }
  }
  let absent : Array[String] = @json.from_json(field(task, "absent_outputs"))
  for path in absent {
    files.remove(path)
  }
  files["result.json"] = field(task, "expected").stringify().to_json()
  let required : Array[Json] = @json.from_json(field(task, "worker_events"))
  let records : Array[Json] = []
  for index, requirement in required {
    let action = field(requirement, "action")
    let args = if field(requirement, "args") != Json::null() {
      field(requirement, "args")
    } else {
      [action, "input.json", field(requirement, "id")].to_json()
    }
    let phases = if action == Json::string("job") ||
      action == Json::string("recover") {
      ["started", "operation", "completed"]
    } else {
      ["started", "completed"]
    }
    for phase in phases {
      records.push({
        "pid": (100 + index).to_json(),
        "confidence": "observed",
        "event": {
          "phase": phase.to_json(),
          "action": action,
          "id": field(requirement, "id"),
          "args": args,
          "exit_code": if phase == "completed" {
            field(requirement, "exit_code")
          } else {
            Json::null()
          },
        },
      })
    }
  }
  { "files": files.to_json(), "errors": [], "worker_events": records.to_json() }
}

///|
test "v4 final summaries cannot replace required artifacts or preserved inputs" {
  let tasks : Array[Json] = @json.from_json(field(suite_v4("basic"), "tasks"))
  for task in tasks {
    let snapshot = v4_snapshot(task)
    assert_eq(
      field(task_oracle(task, snapshot), "success"),
      true.to_json(),
      msg=v4_text(task, "id"),
    )
    let files = object_field(snapshot, "files").copy()
    for path, _ in object_field(task, "expected_outputs") {
      let original = files[path]
      files.remove(path)
      assert_eq(
        field(
          task_oracle(task, {
            "files": files.to_json(),
            "errors": [],
            "worker_events": field(snapshot, "worker_events"),
          }),
          "success",
        ),
        false.to_json(),
        msg=v4_text(task, "id") + ":" + path,
      )
      files[path] = original
    }
    let mutable : Array[String] = @json.from_json(field(task, "mutable_inputs"))
    for path, _ in object_field(task, "files") {
      if mutable.contains(path) {
        continue
      }
      let original = files[path]
      files[path] = "damaged input"
      assert_eq(
        field(
          task_oracle(task, {
            "files": files.to_json(),
            "errors": [],
            "worker_events": field(snapshot, "worker_events"),
          }),
          "success",
        ),
        false.to_json(),
        msg=path,
      )
      files[path] = original
      break
    }
    files["result.json"] = "{}"
    assert_eq(
      field(
        task_oracle(task, {
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

///|
test "v4 required worker actions and hash paths need independent receipts" {
  let tasks : Array[Json] = @json.from_json(field(suite_v4("basic"), "tasks"))
  for task in tasks {
    let required : Array[Json] = @json.from_json(field(task, "worker_events"))
    if required.is_empty() {
      continue
    }
    let snapshot = v4_snapshot(task)
    assert_eq(
      field(
        task_oracle(task, {
          "files": field(snapshot, "files"),
          "errors": [],
          "worker_events": [],
        }),
        "success",
      ),
      false.to_json(),
      msg=v4_text(task, "id"),
    )
    let records : Array[Json] = @json.from_json(
      field(snapshot, "worker_events"),
    )
    let incomplete = records[1:].to_owned()
    assert_eq(
      field(
        task_oracle(task, {
          "files": field(snapshot, "files"),
          "errors": [],
          "worker_events": incomplete.to_json(),
        }),
        "success",
      ),
      Json::null(),
    )
    if v4_text(task, "family") == "release-verification" {
      let missing = records.filter(record => {
        field(record, "pid") != (100).to_json()
      })
      assert_eq(
        field(
          task_oracle(task, {
            "files": field(snapshot, "files"),
            "errors": [],
            "worker_events": missing.to_json(),
          }),
          "success",
        ),
        false.to_json(),
      )
    }
  }
}

///|
test "diagnostic requests do not count as state operations but actual extra work does" {
  let task = v4_basic(5, 316)
  let snapshot = v4_snapshot(task)
  let records : Array[Json] = @json.from_json(field(snapshot, "worker_events"))
  let diagnostic = records.copy()
  for phase in ["started", "completed"] {
    diagnostic.push({
      "pid": 9000,
      "confidence": "observed",
      "event": {
        "phase": phase.to_json(),
        "action": "recover",
        "id": Json::null(),
        "args": ["recover", "--help"],
        "exit_code": if phase == "completed" {
          (2).to_json()
        } else {
          Json::null()
        },
      },
    })
  }
  assert_eq(
    field(
      task_oracle(task, {
        "files": field(snapshot, "files"),
        "errors": [],
        "worker_events": diagnostic.to_json(),
      }),
      "success",
    ),
    true.to_json(),
  )
  let repeated = records.copy()
  for original in records[0:3] {
    let record = match original {
      Object(record) => record.copy()
      _ => abort("record")
    }
    record["pid"] = (9001).to_json()
    repeated.push(Json::object(record))
  }
  assert_eq(
    field(
      task_oracle(task, {
        "files": field(snapshot, "files"),
        "errors": [],
        "worker_events": repeated.to_json(),
      }),
      "success",
    ),
    false.to_json(),
  )
  let without_operations = records.filter(record => {
    field(field(record, "event"), "phase") != Json::string("operation")
  })
  assert_eq(
    field(
      task_oracle(task, {
        "files": field(snapshot, "files"),
        "errors": [],
        "worker_events": without_operations.to_json(),
      }),
      "success",
    ),
    false.to_json(),
  )
}

///|
test "jobs cannot use forged state to bypass real dependency execution or retry" {
  let task = v4_basic(4, 315)
  let snapshot = v4_snapshot(task)
  let records : Array[Json] = @json.from_json(field(snapshot, "worker_events"))
  let reordered = records[3:].to_owned()
  reordered.append(records[0:3])
  assert_eq(
    field(
      task_oracle(task, {
        "files": field(snapshot, "files"),
        "errors": [],
        "worker_events": reordered.to_json(),
      }),
      "success",
    ),
    false.to_json(),
  )
  let required : Array[Json] = @json.from_json(field(task, "worker_events"))
  let failures : Array[Json] = []
  for i, event in required {
    if field(event, "exit_code") == (7).to_json() {
      failures.push((100 + i).to_json())
    }
  }
  let without_retry = records.filter(record => {
    !failures.contains(field(record, "pid"))
  })
  assert_eq(
    field(
      task_oracle(task, {
        "files": field(snapshot, "files"),
        "errors": [],
        "worker_events": without_retry.to_json(),
      }),
      "success",
    ),
    false.to_json(),
  )
}

///|
test "fresh withheld delivery rejects copying the visible answer" {
  let tasks : Array[Json] = @json.from_json(field(suite_v4("basic"), "tasks"))
  for task in tasks {
    let withheld : Array[Json] = @json.from_json(field(task, "withheld"))
    for input in withheld {
      let snapshot = v4_snapshot(input)
      assert_eq(
        field(task_oracle(input, snapshot), "success"),
        true.to_json(),
        msg=v4_text(task, "id"),
      )
      let files = object_field(snapshot, "files").copy()
      files["result.json"] = field(task, "expected").stringify().to_json()
      assert_eq(
        field(
          task_oracle(input, {
            "files": files.to_json(),
            "errors": [],
            "worker_events": field(snapshot, "worker_events"),
          }),
          "success",
        ),
        false.to_json(),
        msg=v4_text(task, "id"),
      )
    }
  }
}

///|
test "no-change validation phase rejects successful but unnecessary recovery" {
  let task = v4_basic(9, 320)
  let phases : Array[Json] = @json.from_json(field(task, "validation_phases"))
  let second = phases[1]
  let snapshot = v4_snapshot(second)
  assert_eq(field(task_oracle(second, snapshot), "success"), true.to_json())
  let first = v4_snapshot(phases[0])
  let records : Array[Json] = @json.from_json(field(first, "worker_events"))
  assert_eq(
    field(
      task_oracle(second, {
        "files": field(snapshot, "files"),
        "errors": [],
        "worker_events": records[0:3].to_owned().to_json(),
      }),
      "success",
    ),
    false.to_json(),
  )
}

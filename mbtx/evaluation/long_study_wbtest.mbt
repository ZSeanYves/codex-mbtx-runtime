///|
test "long oracles reject wrong summaries, lost artifacts and invented execution" {
  for index in 0..<10 {
    for extended in [false, true] {
      let case = long_case(index, extended, 3)
      let task = case.json()
      let files = case.instance.files.copy()
      for path, expected in case.outputs {
        files[path] = match expected {
          String(_) => expected
          _ => expected.stringify().to_json()
        }
      }
      for path in case.absent {
        files.remove(path)
      }
      files["result.json"] = case.instance.expected.stringify().to_json()
      let events : Array[Json] = []
      for i, e in case.worker_events {
        events.push({
          "pid": i.to_json(),
          "event": {
            "phase": "started",
            "action": field(e, "action"),
            "id": field(e, "id"),
            "exit_code": Json::null(),
          },
          "confidence": "observed",
        })
        events.push({
          "pid": i.to_json(),
          "event": {
            "phase": "completed",
            "action": field(e, "action"),
            "id": field(e, "id"),
            "exit_code": field(e, "exit_code"),
          },
          "confidence": "observed",
        })
      }
      let snapshot : Json = {
        "files": files.to_json(),
        "errors": [],
        "worker_events": events.to_json(),
      }
      assert_eq(field(task_oracle(task, snapshot), "success"), true.to_json())
      if index == 4 {
        let forbidden = events.copy()
        forbidden.push({
          "pid": 999,
          "event": {
            "phase": "started",
            "action": "job",
            "id": "job-disabled",
            "exit_code": Json::null(),
          },
          "confidence": "observed",
        })
        assert_eq(
          field(
            task_oracle(task, {
              "files": files.to_json(),
              "errors": [],
              "worker_events": forbidden.to_json(),
            }),
            "success",
          ),
          false.to_json(),
        )
        let reordered = events.copy()
        // A fabricated committed state must not make out-of-order execution valid.
        for i in 0..<2 {
          let first = reordered[i]
          reordered[i] = reordered[i + 4]
          reordered[i + 4] = first
        }
        assert_eq(
          field(
            task_oracle(task, {
              "files": files.to_json(),
              "errors": [],
              "worker_events": reordered.to_json(),
            }),
            "success",
          ),
          false.to_json(),
        )
      }
      files["result.json"] = "{}"
      assert_eq(
        field(
          task_oracle(task, {
            "files": files.to_json(),
            "errors": [],
            "worker_events": events.to_json(),
          }),
          "success",
        ),
        false.to_json(),
      )
      files["result.json"] = case.instance.expected.stringify().to_json()
      if !case.worker_events.is_empty() {
        assert_eq(
          field(
            task_oracle(task, {
              "files": files.to_json(),
              "errors": [],
              "worker_events": [],
            }),
            "success",
          ),
          false.to_json(),
        )
      }
      for path, _ in case.outputs {
        let original = files[path]
        files[path] = "truncated tail"
        assert_eq(
          field(
            task_oracle(task, {
              "files": files.to_json(),
              "errors": [],
              "worker_events": events.to_json(),
            }),
            "success",
          ),
          false.to_json(),
        )
        files[path] = original
      }
    }
  }
}

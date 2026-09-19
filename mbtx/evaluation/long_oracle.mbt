///|
/// Additional files and host-observed worker events are independently checked;
/// a correct summary cannot substitute for missing effects or execution.
fn long_oracle(task : Json, snapshot : Json) -> Json raise {
  let files = object_field(snapshot, "files")
  if field(task, "expected_worker_plan") is { "jobs": Array(expected), .. } &&
    field(task, "worker_plan_path") is String(path) {
    let actual = match files.get(path) {
      Some(String(text)) => @json.parse(text) catch { _ => Json::null() }
      _ => Json::null()
    }
    let wanted = expected.copy()
    guard actual is { "jobs": Array(rows), .. } &&
      rows.all(row => row is { "id": String(_), .. }) else {
      return {
        "success": false,
        "reason": "Missing or malformed derived worker plan",
        "confidence": "observed",
      }
    }
    let rows = match actual {
      { "jobs": Array(rows), .. } => rows.copy()
      _ => []
    }
    wanted.sort_by((a, b) => {
      codepoint_compare(v4_text(a, "id"), v4_text(b, "id"))
    })
    rows.sort_by((a, b) => codepoint_compare(v4_text(a, "id"), v4_text(b, "id")))
    if rows != wanted {
      return {
        "success": false,
        "reason": "Worker plan differs from independently derived dependencies and values",
        "confidence": "observed",
      }
    }
  }
  for path, expected in object_field(task, "expected_outputs") {
    let actual = match (expected, files.get(path)) {
      (String(_), Some(actual)) => actual
      (_, Some(String(text))) => @json.parse(text) catch { _ => Json::null() }
      _ => Json::null()
    }
    if actual != expected {
      return {
        "success": false,
        "reason": ("Missing or incorrect required artifact: " + path).to_json(),
        "confidence": "observed",
      }
    }
  }
  let absent : Array[String] = @json.from_json(field(task, "absent_outputs"))
  for path in absent {
    if files.contains(path) {
      return {
        "success": false,
        "reason": ("Stale artifact was not removed: " + path).to_json(),
        "confidence": "observed",
      }
    }
  }
  let required : Array[Json] = @json.from_json(field(task, "worker_events"))
  if !required.is_empty() ||
    field(task, "strict_worker_receipts") == true.to_json() {
    let records : Array[Json] = @json.from_json(
      field(snapshot, "worker_events"),
    ) catch {
      _ => []
    }
    if records.any(r => field(r, "evidence_error") != Json::null()) {
      return {
        "success": Json::null(),
        "reason": "Worker observation failed",
        "confidence": "unknown",
      }
    }
    if field(task, "strict_worker_receipts") == true.to_json() {
      let strict = worker_receipt_oracle(task, records, required)
      if strict != Json::null() {
        return strict
      }
    }
    let records = worker_operation_records(task, records)
    let completed = records.filter(r => {
      field(field(r, "event"), "phase") == Json::string("completed") &&
      field(r, "confidence") == Json::string("observed")
    })
    if required.any(e => field(e, "action") == Json::string("job")) {
      let plan = if field(task, "worker_plan_path") is String(path) {
        @json.parse(string_field(field(snapshot, "files"), path))
      } else {
        @json.parse(string_field(field(task, "files"), "jobs.json"))
      }
      let jobs : Array[Json] = @json.from_json(field(plan, "jobs"))
      let done : Map[String, Bool] = Map([])
      let initial : Array[String] = @json.from_json(
        field(task, "initial_completed"),
      ) catch {
        _ => []
      }
      for id in initial {
        done[id] = true
      }
      for record in completed {
        let e = field(record, "event")
        if field(e, "action") == Json::string("recover") &&
          field(e, "exit_code") == (0).to_json() {
          done[string_field(e, "id")] = true
        }
      }
      let retried : Map[String, Bool] = Map([])
      let prior_retries : Array[String] = @json.from_json(
        field(task, "initial_retries"),
      ) catch {
        _ => []
      }
      for id in prior_retries {
        retried[id] = true
      }
      let ready : Map[String, Bool] = Map([])
      for record in records {
        if field(record, "confidence") != Json::string("observed") {
          continue
        }
        let event = field(record, "event")
        if field(event, "action") != Json::string("job") {
          continue
        }
        let id = string_field(event, "id")
        let key = field(record, "pid").stringify() + "/" + id
        let row = jobs
          .iter()
          .find_first(job => field(job, "id") == id.to_json())
        if field(event, "phase") == Json::string("started") {
          if row is Some(row) {
            if field(row, "enabled") == false.to_json() {
              return {
                "success": false,
                "reason": "A disabled job was executed",
                "confidence": "observed",
              }
            }
            let deps : Array[String] = @json.from_json(field(row, "deps"))
            ready[key] = deps.all(dep => done.get(dep).unwrap_or(false))
          }
        } else if field(event, "phase") == Json::string("completed") {
          if field(event, "exit_code") == (7).to_json() {
            retried[id] = true
          }
          if field(event, "exit_code") == (0).to_json() {
            guard ready.get(key) is Some(ordered) else {
              return {
                "success": Json::null(),
                "reason": "Successful job lacks a matching start receipt",
                "confidence": "unknown",
              }
            }
            if !ordered {
              return {
                "success": false,
                "reason": "Job succeeded before its dependencies were observed complete",
                "confidence": "observed",
              }
            }
            if row is Some(row) &&
              field(row, "fail_once") == true.to_json() &&
              !retried.get(id).unwrap_or(false) {
              return {
                "success": false,
                "reason": "Retryable job succeeded without its required prior failure",
                "confidence": "observed",
              }
            }
            done[id] = true
          }
        }
      }
    }
    for event in required {
      if !completed.any(r => {
          let actual = field(r, "event")
          field(actual, "action") == field(event, "action") &&
          field(actual, "id") == field(event, "id") &&
          field(actual, "exit_code") == field(event, "exit_code") &&
          (
            field(event, "args") == Json::null() ||
            field(actual, "args") == field(event, "args")
          )
        }) {
        return {
          "success": false,
          "reason": "Required real worker execution was not observed",
          "confidence": "observed",
        }
      }
    }
    if required.any(e => field(e, "action") == Json::string("recover")) &&
      completed.any(r => {
        let event = field(r, "event")
        field(event, "action") == Json::string("recover") &&
        field(event, "exit_code") == (0).to_json() &&
        !required.any(e => field(e, "id") == field(event, "id"))
      }) {
      return {
        "success": false,
        "reason": "An already valid record was unnecessarily re-executed",
        "confidence": "observed",
      }
    }
  }
  {
    "success": true,
    "reason": "Independent final artifacts, authorized mutation, absence and real worker receipts passed",
    "confidence": "observed",
  }
}

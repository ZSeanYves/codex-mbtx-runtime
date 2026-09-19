///|
/// Require paired start/completion receipts. Exact execution counts apply
/// only where the task declares them; read-only diagnostics may be repeated.
/// A missing native receipt is never reconstructed from source text.
fn worker_receipt_oracle(
  task : Json,
  records : Array[Json],
  required : Array[Json],
) -> Json {
  let observed = records.filter(r => {
    field(r, "confidence") == Json::string("observed")
  })
  let operations = worker_operation_records(task, observed)
  let completed = operations.filter(r => {
    field(field(r, "event"), "phase") == Json::string("completed")
  })
  for record in observed {
    let event = field(record, "event")
    let phase = field(event, "phase")
    if phase != Json::string("started") && phase != Json::string("completed") {
      continue
    }
    let partners = observed.filter(other => {
      let e = field(other, "event")
      field(other, "pid") == field(record, "pid") &&
      field(e, "action") == field(event, "action") &&
      field(e, "id") == field(event, "id") &&
      field(e, "args") == field(event, "args") &&
      field(e, "phase") ==
      (if phase == Json::string("started") { "completed" } else { "started" }).to_json()
    })
    if partners.length() != 1 {
      return {
        "success": Json::null(),
        "reason": "Worker request lacks a unique matching start/completion receipt",
        "confidence": "unknown",
      }
    }
  }
  let actions : Array[String] = @json.from_json(
    field(task, "exact_worker_actions"),
  ) catch {
    _ => []
  }
  for record in completed {
    let event = field(record, "event")
    if actions.contains(
        match field(event, "action") {
          String(action) => action
          _ => ""
        },
      ) &&
      !required.any(e => {
        field(e, "action") == field(event, "action") &&
        field(e, "id") == field(event, "id") &&
        field(e, "exit_code") == field(event, "exit_code")
      }) {
      return {
        "success": false,
        "reason": "Unexpected state-changing worker execution or exit status",
        "confidence": "observed",
      }
    }
  }
  for expected in required {
    let required_once = field(expected, "action") == Json::string("recover") &&
      field(expected, "exit_code") == (0).to_json()
    if !required_once &&
      !actions.contains(
        match field(expected, "action") {
          String(action) => action
          _ => ""
        },
      ) {
      continue
    }
    let actual_count = completed
      .filter(r => {
        let e = field(r, "event")
        field(e, "action") == field(expected, "action") &&
        field(e, "id") == field(expected, "id") &&
        field(e, "exit_code") == field(expected, "exit_code")
      })
      .length()
    if actual_count != 1 {
      return {
        "success": false,
        "reason": "Required worker execution count differs from the declared recovery/retry contract",
        "confidence": "observed",
      }
    }
  }
  if field(task, "worker_plan_path") != Json::null() {
    let recovered : Array[Json] = []
    let recoveries = required.filter(e => {
      field(e, "action") == Json::string("recover")
    })
    for r in operations {
      let e = field(r, "event")
      if field(e, "action") == Json::string("recover") &&
        field(e, "phase") == Json::string("completed") &&
        field(e, "exit_code") == (0).to_json() {
        recovered.push(field(e, "id"))
      }
      if field(e, "action") == Json::string("job") &&
        field(e, "phase") == Json::string("started") &&
        !recoveries.all(e => recovered.contains(field(e, "id"))) {
        return {
          "success": false,
          "reason": "A downstream job began before all required recoveries completed",
          "confidence": "observed",
        }
      }
    }
  }
  Json::null()
}

///|
/// Preserve diagnostic processes in raw evidence, but enforce execution policy
/// only on worker actions that crossed the native operation boundary.
fn worker_operation_records(task : Json, records : Array[Json]) -> Array[Json] {
  if field(task, "operation_receipts") != true.to_json() {
    return records
  }
  records.filter(record => {
    let e = field(record, "event")
    let action = field(e, "action")
    if action != Json::string("job") && action != Json::string("recover") {
      return true
    }
    records.any(other => {
      let o = field(other, "event")
      field(other, "confidence") == Json::string("observed") &&
      field(other, "pid") == field(record, "pid") &&
      field(o, "phase") == Json::string("operation") &&
      field(o, "args") == field(e, "args")
    })
  })
}

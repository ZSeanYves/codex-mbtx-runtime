///|
/// A reusable program must succeed without the model on all frozen inputs.
/// Capture failures are unknown/harness errors, never backend failures.
fn submission_oracle(task : Json, validation : Json) -> Json raise {
  let status = field(validation, "status")
  if status == Json::null() ||
    status == Json::string("harness_error") ||
    status == Json::string("incomplete_build") {
    return {
      "success": Json::null(),
      "reason": "Submission validation missing or incomplete",
      "confidence": "unknown",
      "cases": [],
    }
  }
  if status != Json::string("captured") {
    return {
      "success": false,
      "reason": status,
      "confidence": "observed",
      "cases": [],
    }
  }
  let inputs = [task]
  let withheld : Array[Json] = @json.from_json(field(task, "withheld"))
  inputs.append(withheld[:])
  let cases : Array[Json] = @json.from_json(field(validation, "cases"))
  if cases.length() != inputs.length() {
    return {
      "success": Json::null(),
      "reason": "Validation case evidence incomplete",
      "confidence": "unknown",
      "cases": [],
    }
  }
  let outcomes : Array[Json] = []
  let mut success = true
  let mut unknown = false
  for index, input in inputs {
    let process = field(cases[index], "process")
    let oracle = task_oracle(input, field(cases[index], "snapshot"))
    let observed = field(process, "wait_observed") == true.to_json() &&
      field(process, "drain_complete") == true.to_json() &&
      field(process, "residual_group_before_cleanup") != Json::null() &&
      field(oracle, "success") != Json::null()
    let passed = observed &&
      field(process, "exit_code") == (0).to_json() &&
      field(process, "timed_out") == false.to_json() &&
      field(process, "residual_group_before_cleanup") == false.to_json() &&
      field(oracle, "success") == true.to_json()
    if !passed {
      success = false
    }
    if !observed || field(oracle, "success") == Json::null() {
      unknown = true
    }
    outcomes.push({
      "index": index.to_json(),
      "success": if observed {
        passed.to_json()
      } else {
        Json::null()
      },
      "oracle": oracle,
      "process": process,
    })
  }
  {
    "success": if unknown {
      Json::null()
    } else {
      success.to_json()
    },
    "reason": "Saved-source execution and independent output/input-preservation oracles on visible and withheld inputs",
    "confidence": if unknown {
      "unknown"
    } else {
      "observed"
    },
    "cases": outcomes.to_json(),
  }
}

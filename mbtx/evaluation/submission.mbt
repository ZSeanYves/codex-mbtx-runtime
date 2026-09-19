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
    let expected_phases : Array[Json] = match
      field(input, "validation_phases") {
      Array(phases) => phases
      _ => [input]
    }
    let actual_phases : Array[Json] = match field(cases[index], "phases") {
      Array(phases) => phases
      _ => [cases[index]]
    }
    let mut failed_process = false
    for phase_index, facts in actual_phases {
      if phase_index >= expected_phases.length() {
        unknown = true
        continue
      }
      let process = field(facts, "process")
      let oracle = task_oracle(
        expected_phases[phase_index],
        field(facts, "snapshot"),
      )
      let preserved : Array[Json] = @json.from_json(
        field(expected_phases[phase_index], "preserve_previous"),
      ) catch {
        _ => []
      }
      let preservation_known = preserved.is_empty() ||
        field(field(facts, "input_preservation"), "success") != Json::null()
      let observed = field(process, "wait_observed") == true.to_json() &&
        field(process, "drain_complete") == true.to_json() &&
        field(process, "residual_group_before_cleanup") != Json::null() &&
        field(oracle, "success") != Json::null() &&
        preservation_known
      let process_ok = field(process, "exit_code") == (0).to_json() &&
        field(process, "timed_out") == false.to_json() &&
        field(process, "residual_group_before_cleanup") == false.to_json()
      let passed = observed &&
        process_ok &&
        field(oracle, "success") == true.to_json() &&
        field(field(facts, "input_preservation"), "success") != false.to_json()
      if observed && !process_ok {
        failed_process = true
      }
      if !passed {
        success = false
      }
      if !observed {
        unknown = true
      }
      outcomes.push({
        "index": index.to_json(),
        "phase": phase_index.to_json(),
        "success": if observed {
          passed.to_json()
        } else {
          Json::null()
        },
        "input_preservation": field(facts, "input_preservation"),
        "oracle": oracle,
        "process": process,
      })
    }
    if actual_phases.length() != expected_phases.length() && !failed_process {
      unknown = true
    }
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

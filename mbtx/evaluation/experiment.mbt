///|
fn empty_accounting() -> Json {
  {
    "complete": false,
    "coverage": {
      "steps": false,
      "http_requests": false,
      "tool_results": false,
    },
    "metrics": {
      "agent_steps_started": Json::null(),
      "agent_steps": Json::null(),
      "model_requests": Json::null(),
      "tool_errors": Json::null(),
      "tool_calls": Json::null(),
      "repair_steps": Json::null(),
      "process_spawns": Json::null(),
    },
    "observed": {},
    "limitations": ["No usable native trace"],
  }
}

///|
/// Classify facts without conflating an external failure with a tool defect.
pub fn analyze_attempt(task : Json, facts : Json) -> Json raise {
  let trace = field(facts, "trace")
  let accounting = if trace == Json::null() {
    empty_accounting()
  } else {
    analyze_steps(trace)
  }
  let visible_oracle : Json = if task == Json::null() {
    {
      "success": Json::null(),
      "reason": "No valid task assignment",
      "confidence": "unknown",
    }
  } else {
    task_oracle(task, field(facts, "snapshot"))
  }
  let validation = field(facts, "submission")
  let oracle : Json = if field(task, "acceptance") == Json::string("programs") {
    let submitted = submission_oracle(task, validation)
    if field(visible_oracle, "success") != true.to_json() {
      {
        "success": field(visible_oracle, "success"),
        "reason": "Visible workspace oracle did not pass",
        "confidence": field(visible_oracle, "confidence"),
        "submitted_program": submitted,
      }
    } else {
      submitted
    }
  } else {
    visible_oracle
  }
  let outcome = field(facts, "outcome")
  let termination = field(outcome, "termination")
  let exchanges : Array[Json] = @json.from_json(field(facts, "exchanges"))
  let external = exchanges.any(item => {
    let status : Int? = nullable(field(item, "status_code")) catch { _ => None }
    field(item, "transport_error") == true.to_json() ||
    (
      field(item, "sse_expected") == true.to_json() &&
      field(item, "complete") == true.to_json() &&
      field(item, "sse_terminal") != Json::string("completed")
    ) ||
    (match status {
      Some(n) => n >= 400
      None => false
    })
  })
  let status = if field(facts, "execution_error") != Json::null() ||
    field(facts, "integrity") == false.to_json() ||
    field(validation, "status") == Json::string("harness_error") ||
    exchanges.any(item => field(item, "evidence_error") == true.to_json()) {
    "harness_error"
  } else if outcome == Json::null() {
    "censored"
  } else if termination == Json::string("wall_limit") {
    "censored"
  } else if termination == Json::string("timeout") {
    "timeout"
  } else if termination == Json::string("cancelled") {
    "cancelled"
  } else if termination == Json::string("budget_exhausted") {
    "budget_exhausted"
  } else if termination == Json::string("harness_error") {
    "harness_error"
  } else if external {
    "relay_error"
  } else if trace == Json::null() &&
    field(outcome, "exit_code") != (0).to_json() {
    "harness_error"
  } else if field(oracle, "success") == true.to_json() &&
    field(outcome, "exit_code") == (0).to_json() {
    "success"
  } else if field(oracle, "success") == false.to_json() {
    "task_failure"
  } else {
    "unknown"
  }
  {
    "attempt_id": field(facts, "attempt_id"),
    "pair_id": field(facts, "pair_id"),
    "task_id": field(task, "id"),
    "arm": field(facts, "arm"),
    "status": status.to_json(),
    "steps_to_success": if status == "success" &&
      field(field(accounting, "coverage"), "steps") == true.to_json() {
      field(field(accounting, "metrics"), "agent_steps")
    } else {
      Json::null()
    },
    "termination_reason": termination,
    "oracle": oracle,
    "visible_oracle": visible_oracle,
    "submission": validation,
    "workspace": field(facts, "workspace"),
    "family": field(task, "family"),
    "scenario": field(task, "scenario"),
    "variant": field(task, "variant"),
    "acceptance": field(task, "acceptance"),
    "accounting": accounting,
    "tool_outcomes": tool_outcomes(
      trace,
      field(facts, "tool_results"),
      complete=field(field(accounting, "coverage"), "tool_results") ==
        true.to_json(),
    ),
    "upstream_requests_observed": exchanges
    .filter(e => {
      field(field(e, "headers"), "send_ns") != Json::null() ||
      field(e, "send_ns") != Json::null()
    })
    .length()
    .to_json(),
    "outcome": outcome,
    "exchanges": exchanges.to_json(),
    "evidence": field(facts, "evidence"),
    "integrity": field(facts, "integrity"),
    "external_attribution": "Relay/provider boundary observed; their internal causes are not separately observable.",
  }
}

///|
fn infrastructure(status : Json) -> Bool {
  status == Json::string("relay_error") ||
  status == Json::string("provider_error") ||
  status == Json::string("harness_error")
}

///|
/// Continue through isolated infrastructure failures; retain every attempt.
pub fn collection_gate(attempts : Array[Json]) -> Json {
  let n = attempts.length()
  let consecutive = n >= 5 &&
    attempts[n - 5:].all(a => infrastructure(field(a, "status")))
  let window = n >= 16 &&
    attempts[n - 16:].filter(a => infrastructure(field(a, "status"))).length() >=
    8
  {
    "pause": (consecutive || window).to_json(),
    "reason": if consecutive {
      "Five consecutive infrastructure failures"
    } else if window {
      "At least eight infrastructure failures in the last sixteen arms"
    } else {
      "Continue"
    },
  }
}

///|
fn comparable(attempt : Json) -> Bool {
  field(attempt, "status") == Json::string("success") &&
  field(attempt, "integrity") == true.to_json() &&
  field(field(field(attempt, "accounting"), "coverage"), "steps") ==
  true.to_json()
}

///|
fn first_divergence(a : Json, b : Json) -> Json {
  let left : Array[Json] = @json.from_json(
    field(field(a, "accounting"), "steps"),
  ) catch {
    _ => []
  }
  let right : Array[Json] = @json.from_json(
    field(field(b, "accounting"), "steps"),
  ) catch {
    _ => []
  }
  for index in 0..<Int::max(left.length(), right.length()) {
    let l = left.get(index).unwrap_or(Json::null())
    let r = right.get(index).unwrap_or(Json::null())
    let signature = fn(step : Json) -> Json {
      let tools : Array[Json] = @json.from_json(field(step, "emitted_tools")) catch {
        _ => []
      }
      {
        "outcome": field(step, "outcome"),
        "tools": tools
        .map(t => field(field(t, "observation"), "tool_name"))
        .to_json(),
      }
    }
    if signature(l) != signature(r) {
      return {
        "step_index": (index + 1).to_json(),
        "shell": signature(l),
        "mbtx": signature(r),
        "shell_source_seq": field(l, "started_seq"),
        "mbtx_source_seq": field(r, "started_seq"),
        "confidence": "observed",
        "interpretation": "Trajectory difference; tool identity is part of the treatment, not proof of cause.",
      }
    }
  }
  Json::null()
}

///|
/// ITT counts include every scheduled arm. Conditional differences require two
/// successful arms with complete step evidence. Pilot intervals are descriptive.
pub fn analyze_experiment(
  manifest : Json,
  attempts : Array[Json],
) -> Json raise {
  let schedule : Array[Json] = @json.from_json(field(manifest, "schedule"))
  let formal = field(field(manifest, "protocol"), "suite") ==
    Json::string("programs") ||
    field(field(manifest, "protocol"), "suite") == Json::string("workflow")
  let pairs : Array[Json] = []
  let by_arm : Map[String, Json] = Map([])
  for arm in ["shell_tool", "mbtx_program"] {
    let selected = attempts.filter(a => field(a, "arm") == arm.to_json())
    let success = selected
      .filter(a => field(a, "status") == Json::string("success"))
      .length()
    let statuses : Map[String, Int] = Map([])
    for attempt in selected {
      let status = string_field(attempt, "status")
      statuses[status] = statuses.get(status).unwrap_or(0) + 1
    }
    let curve : Array[Json] = []
    for budget in [1, 2, 4, 8, 12] {
      let count = selected
        .filter(a => {
          if !comparable(a) {
            return false
          }
          let steps : Int = @json.from_json(
            field(field(field(a, "accounting"), "metrics"), "agent_steps"),
          ) catch {
            _ => 2147483647
          }
          steps <= budget
        })
        .length()
      curve.push({
        "budget": budget.to_json(),
        "successes": count.to_json(),
        "denominator": schedule.length().to_json(),
      })
    }
    by_arm[arm] = {
      "assigned": schedule.length().to_json(),
      "captured": selected.length().to_json(),
      "not_started": (schedule.length() - selected.length()).to_json(),
      "statuses": statuses.to_json(),
      "successes": success.to_json(),
      "external_failures": selected
      .filter(a => {
        field(a, "status") == Json::string("relay_error") ||
        field(a, "status") == Json::string("provider_error")
      })
      .length()
      .to_json(),
      "infrastructure_failures": selected
      .filter(a => infrastructure(field(a, "status")))
      .length()
      .to_json(),
      "success_by_step_budget": curve.to_json(),
    }
  }
  for pair in schedule {
    let id = field(pair, "pair_id")
    let members = attempts.filter(a => field(a, "pair_id") == id)
    let a = members
      .filter(a => field(a, "arm") == Json::string("shell_tool"))
      .get(0)
    let b = members
      .filter(a => field(a, "arm") == Json::string("mbtx_program"))
      .get(0)
    let baseline_equal : Json = match (a, b) {
      (Some(a), Some(b)) => {
        let left = field(field(a, "workspace"), "baseline_commit")
        let right = field(field(b, "workspace"), "baseline_commit")
        if left == Json::null() || right == Json::null() {
          Json::null()
        } else {
          (left == right).to_json()
        }
      }
      _ => Json::null()
    }
    let difference : Json = match (a, b) {
      (Some(a), Some(b)) if comparable(a) &&
        comparable(b) &&
        (!formal || baseline_equal == true.to_json()) => {
        let ac : Int = @json.from_json(
          field(field(field(a, "accounting"), "metrics"), "agent_steps"),
        )
        let bc : Int = @json.from_json(
          field(field(field(b, "accounting"), "metrics"), "agent_steps"),
        )
        (bc - ac).to_json()
      }
      _ => Json::null()
    }
    let divergence = match (a, b) {
      (Some(a), Some(b)) => first_divergence(a, b)
      _ => Json::null()
    }
    pairs.push({
      "pair_id": id,
      "task_id": field(pair, "task_id"),
      "family": field(pair, "family"),
      "scenario": field(pair, "scenario"),
      "variant": field(pair, "variant"),
      "repeat": field(pair, "repeat"),
      "workspace_baseline_equal": baseline_equal,
      "comparison_exclusion": if formal && baseline_equal != true.to_json() {
        "Missing or unequal initial workspace commits"
      } else if difference == Json::null() {
        "Two successful arms with complete step evidence required"
      } else {
        Json::null()
      },
      "shell_status": match a {
        Some(a) => field(a, "status")
        None => Json::null()
      },
      "mbtx_status": match b {
        Some(b) => field(b, "status")
        None => Json::null()
      },
      "shell_steps": match a {
        Some(a) =>
          field(field(field(a, "accounting"), "metrics"), "agent_steps")
        None => Json::null()
      },
      "mbtx_steps": match b {
        Some(b) =>
          field(field(field(b, "accounting"), "metrics"), "agent_steps")
        None => Json::null()
      },
      "step_difference_mbtx_minus_shell": difference,
      "first_divergence": divergence,
      "attempt_ids": members.map(a => field(a, "attempt_id")).to_json(),
    })
  }
  let deltas = pairs.filter(p => {
    field(p, "step_difference_mbtx_minus_shell") != Json::null()
  })
  let mut total = 0.0
  for p in deltas {
    let n : Double = @json.from_json(
      field(p, "step_difference_mbtx_minus_shell"),
    )
    total += n
  }
  {
    "schema_version": 1,
    "analysis_version": "programmable-steps-v3",
    "run_id": field(manifest, "run_id"),
    "mode": field(manifest, "mode"),
    "platform": field(manifest, "platform"),
    "protocol_id": field(field(manifest, "protocol"), "protocol_id"),
    "manifest_integrity": field(manifest, "manifest_integrity"),
    "collection_stop_reason": field(manifest, "collection_stop_reason"),
    "population": if formal {
      field(field(manifest, "protocol"), "population")
    } else {
      "pilot; all scheduled arms retained"
    },
    "suite": field(field(manifest, "protocol"), "suite"),
    "partial": (attempts.length() != schedule.length() * 2 ||
    attempts.any(a => field(a, "status") == Json::string("censored"))).to_json(),
    "itt": by_arm.to_json(),
    "conditional": if formal {
      study_uncertainty(pairs)
    } else {
      {
        "pairs": deltas.length().to_json(),
        "mean_step_difference": if deltas.is_empty() {
          Json::null()
        } else {
          (total / deltas.length().to_double()).to_json()
        },
        "confidence_interval": Json::null(),
        "interval_reason": "Formal uncertainty design is frozen after the pilot; small pilot repetitions are not independent tasks.",
      }
    },
    "by_family": if formal {
      study_strata(pairs, attempts, "family")
    } else {
      []
    },
    "by_scenario": if formal {
      study_strata(pairs, attempts, "scenario")
    } else {
      []
    },
    "pairs": pairs.to_json(),
    "attempts": attempts.to_json(),
    "gate": collection_gate(attempts),
    "limitations": [
      "Fixed replay validates measurement, not autonomous step benefit.", "Timing is diagnostic; no claim of zero observation perturbation.",
      "Repair candidates remain inferred; process counts and unobserved token usage remain unknown.",
      "Conclusions apply to the recorded model, interfaces, tasks and platform only.",
    ],
  }
}

///|
/// Pure versioned worker protocol shared by collection and offline reports.
pub fn evaluate_request(request : Json) -> Json raise {
  guard field(request, "schema_version") == (1).to_json() else {
    fail("Unsupported analysis protocol")
  }
  match string_field(request, "op") {
    "protocol" =>
      if field(request, "suite") == Json::null() ||
        field(request, "suite") == Json::string("pilot") {
        pilot_protocol()
      } else {
        let suite = string_field(request,"suite")
        if suite == "long-study" || suite == "long-pilot" { long_protocol(suite == "long-pilot") } else { study_protocol(suite) }
      }
    "steps" => analyze_steps(field(request, "trace"))
    "attempt" =>
      analyze_attempt(field(request, "task"), field(request, "facts"))
    "report" =>
      analyze_experiment(
        field(request, "manifest"),
        @json.from_json(field(request, "attempts")),
      )
    "gate" => collection_gate(@json.from_json(field(request, "attempts")))
    _ => fail("Unknown analysis operation")
  }
}

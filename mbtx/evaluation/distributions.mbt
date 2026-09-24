///|
/// Actual counts, including failed attempts, stay distinct from completion
/// counts. Missing totals are not replaced with observed-prefix lower bounds.
fn step_distributions(
  attempts : Array[Json],
  targets : Json,
  group? : String = "complexity",
) -> Json {
  let tiers : Array[String] = []
  for attempt in attempts {
    if field(attempt, group) is String(tier) && !tiers.contains(tier) {
      tiers.push(tier)
    }
  }
  tiers.sort()
  let rows : Array[Json] = []
  for tier in tiers {
    for arm in ["shell_tool", "mbtx_program"] {
      let members = attempts.filter(a => {
        field(a, group) == tier.to_json() && field(a, "arm") == arm.to_json()
      })
      let values : Array[Int] = []
      let observations = members.map(a => {
        let n : Int? = nullable(
          field(field(field(a, "accounting"), "metrics"), "agent_steps"),
        ) catch {
          _ => None
        }
        if n is Some(n) {
          values.push(n)
        }
        (
          {
            "attempt_id": field(a, "attempt_id"),
            "task_id": field(a, "task_id"),
            "pair_id": field(a, "pair_id"),
            "status": field(a, "status"),
            "steps": field(
              field(field(a, "accounting"), "metrics"),
              "agent_steps",
            ),
            "observed_steps": field(
              field(field(a, "accounting"), "observed"),
              "agent_steps",
            ),
            "observed_steps_started": field(
              field(field(a, "accounting"), "observed"),
              "agent_steps_started",
            ),
            "steps_to_success": field(a, "steps_to_success"),
          } : Json)
      })
      observations.sort_by((a, b) => {
        codepoint_compare(
          field(a, "attempt_id").stringify(),
          field(b, "attempt_id").stringify(),
        )
      })
      values.sort()
      let n = values.length()
      let median = if n == 0 {
        Json::null()
      } else if n % 2 == 1 {
        values[n / 2].to_json()
      } else {
        ((values[n / 2 - 1].to_double() + values[n / 2].to_double()) / 2.0).to_json()
      }
      let target : Array[Int] = @json.from_json(field(targets, tier)) catch {
        _ => []
      }
      rows.push({
        "complexity": if group == "complexity" {
          tier.to_json()
        } else {
          Json::null()
        },
        "track": if group == "track" {
          tier.to_json()
        } else {
          Json::null()
        },
        "arm": arm.to_json(),
        "captured": members.length().to_json(),
        "known_totals": n.to_json(),
        "unknown_totals": (members.length() - n).to_json(),
        "values": values.to_json(),
        "median": median,
        "range": if n == 0 {
          Json::null()
        } else {
          [values[0].to_json(), values[n - 1].to_json()]
        },
        "design_target": target.to_json(),
        "within_target": if target.is_empty() {
          Json::null()
        } else {
          values
          .filter(v => v >= target[0] && v <= target[1])
          .length()
          .to_json()
        },
        "observations": observations.to_json(),
        "interpretation": "Descriptive actual steps across all captured outcomes with known totals; observed prefixes are separate, and ranges are calibration targets only",
      })
    }
  }
  rows.to_json()
}

///|
fn repeat_comparisons(
  pairs : Array[Json],
  attempts : Array[Json],
) -> Json raise {
  let tasks : Array[String] = []
  for pair in pairs {
    let id = string_field(pair, "task_id")
    if !tasks.contains(id) {
      tasks.push(id)
    }
  }
  tasks.sort()
  let result : Array[Json] = []
  for task in tasks {
    let assigned = pairs.filter(p => field(p, "task_id") == task.to_json())
    if assigned.length() < 2 {
      continue
    }
    for arm in ["shell_tool", "mbtx_program"] {
      let arm_pairs = assigned.filter(p => report_assigned_arms(p).contains(arm))
      if arm_pairs.length() < 2 {
        continue
      }
      let repetitions = arm_pairs.map(p => {
        let attempt = attempts
          .iter()
          .find_first(a => {
            field(a, "pair_id") == field(p, "pair_id") &&
            field(a, "arm") == arm.to_json()
          })
          .unwrap_or(Json::null())
        (
          {
            "repeat": field(p, "repeat"),
            "pair_id": field(p, "pair_id"),
            "attempt_id": field(attempt, "attempt_id"),
            "status": field(attempt, "status"),
            "steps": field(
              field(field(attempt, "accounting"), "metrics"),
              "agent_steps",
            ),
            "steps_to_success": field(attempt, "steps_to_success"),
          } : Json)
      })
      repetitions.sort_by((a, b) => {
        let a : Int = @json.from_json(field(a, "repeat")) catch { _ => -1 }
        let b : Int = @json.from_json(field(b, "repeat")) catch { _ => -1 }
        a.compare(b)
      })
      let left = repetitions[0]
      let right = repetitions[1]
      let a : Int? = nullable(field(left, "steps")) catch { _ => None }
      let b : Int? = nullable(field(right, "steps")) catch { _ => None }
      result.push({
        "task_id": task.to_json(),
        "arm": arm.to_json(),
        "repetitions": repetitions.to_json(),
        "first_two_step_difference": match (a, b) {
          (Some(a), Some(b)) => (b - a).to_json()
          _ => Json::null()
        },
        "first_two_status_equal": if field(left, "status") == Json::null() ||
          field(right, "status") == Json::null() {
          Json::null()
        } else {
          (field(left, "status") == field(right, "status")).to_json()
        },
        "interpretation": "Nested repetitions of the same fixed task, not independent task definitions or proof of a source of variance",
      })
    }
  }
  result.to_json()
}

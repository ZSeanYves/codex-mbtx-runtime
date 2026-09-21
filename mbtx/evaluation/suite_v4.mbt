///|
/// One input per classic family and two workflows with three frozen tiers.
fn suite_v4(track : String) -> Json raise {
  guard ["basic", "complexity", "all"].contains(track) else {
    fail(
      "Use basic, complexity or all; historical runs require their frozen bundle",
    )
  }
  let tasks : Array[Json] = []
  let names = [
    "repository-impact", "data-reconciliation", "incident-investigation", "configuration-migration",
    "dependent-jobs", "incremental-recovery", "output-forensics", "release-verification",
    "reusable-data-program", "reusable-recovery-program",
  ]
  if track != "complexity" {
    for index, name in names {
      let task = v4_basic(index, 311 + index)
      tasks.push(
        v4_task(
          task,
          name,
          "basic",
          Json::null(),
          index >= 8,
          if index >= 8 {
            [v4_basic(index, 37), v4_basic(index, 83)]
          } else {
            []
          },
        ),
      )
    }
  }
  if track != "basic" {
    for tier, name in ["quick", "standard", "deep"] {
      for family in ["event-recovery", "release-repair"] {
        let make = fn(variant : Int) -> Json {
          if family == "event-recovery" {
            v4_recovery(tier, variant)
          } else {
            v4_release(tier, variant)
          }
        }
        tasks.push(
          v4_task(
            make(401 + tier),
            family,
            "complexity",
            name.to_json(),
            true,
            [make(48), make(89)],
          ),
        )
      }
    }
  }
  {
    "schema_version": 1,
    "protocol_id": "programmable-shell-replacement-v4",
    "suite": track.to_json(),
    "population": "Ten classic basic capabilities and two workflow families at three difficulty tiers; two repetitions nested in each fixed task. Descriptive evidence only.",
    "arms": ["shell_tool", "mbtx_program"],
    "seed": 20260919,
    "repeats": 2,
    "wall_seconds": 7200,
    "tool_output_tokens": 4096,
    "pacing": {
      "concurrency": 1,
      "min_interval_ms": 15000,
      "fallback_cooldown_ms": 30000,
    },
    "observation": {
      "native_trace": true,
      "native_otel": true,
      "billing_lookup": "local 404; recorded and not forwarded; cost unknown",
    },
    "instructions": "Complete the task autonomously in cwd. Both arms receive identical inputs and declared external utilities. Use the assigned language for control flow and orchestration; use suitable allowed utilities for data transformation and search. Do not use utility features that execute other programs. No network access, delegation, other attempts or private oracle access. Shared editing and read_resource are available. Inspect relevant API references instead of repeatedly guessing names. Preserve declared inputs and verify actual outputs before a final answer. No continuation prompts or oracle feedback are provided.",
    "arm_instructions": {
      "shell_tool": "Use Shell for execution and orchestration. For reusable-program tasks save exactly solution.sh, a complete single POSIX sh source that runs as sh solution.sh without arguments from cwd. Other authored files may not be dependencies. Other tasks need no saved source.",
      "mbtx_program": "Use MBTX for execution and orchestration. @shell.Cmd(program,args) uses direct execution and literal argv, without Shell expansion. Use jq for JSON transformations and safe rg queries when helpful; MoonBit composes their results and handles errors. Shell/interpreter/forwarder processes are prohibited. For reusable-program tasks save exactly solution.mbtx, including imports, runnable without arguments from cwd and without auxiliary authored dependencies. Other tasks need no saved source.",
    },
    "tasks": tasks.to_json(),
    "validation": {
      "cases": 3,
      "source_limit_bytes": 65536,
      "deadline": "remaining_attempt_budget",
    },
    "step_targets": {
      "quick": [5, 20],
      "standard": [20, 50],
      "deep": [50, 100],
    },
    "limitations": [
      "Difficulty tiers are structural design targets; actual model steps require online calibration.",
      "Offline reference replay verifies execution and evidence, not autonomous model performance.",
      "Repeated runs are not independent tasks. No pooled effect across basic and complexity tracks.",
      "MoonRun policy constrains direct process requests; complete descendant confinement is not claimed.",
    ],
  }
}

///|
fn v4_task(
  case : Json,
  family : String,
  track : String,
  tier : Json,
  delivery : Bool,
  withheld : Array[Json],
) -> Json {
  let object = match case {
    Object(o) => o.copy()
    _ => abort("case object")
  }
  let allow : Array[Json] = [
    { "program": "jq", "args_prefix": [] },
    { "program": "rg", "args_prefix": ["--no-config", "--json", "--"] },
    { "program": "rg", "args_prefix": ["--no-config", "--files", "--"] },
  ]
  let actions : Array[String] = @json.from_json(field(case, "actions")) catch {
    _ => []
  }
  for action in actions {
    allow.push({
      "program": "fixture-worker",
      "args_prefix": [action.to_json()],
    })
  }
  let forms = allow.map(rule => {
    let args : Array[String] = @json.from_json(field(rule, "args_prefix")) catch {
      _ => []
    }
    v4_text(rule, "program") + " " + args.to_json().stringify()
  })
  object["id"] = (family +
  "-" +
  (match tier {
    String(t) => t
    _ => "basic"
  }) +
  "-v4").to_json()
  object["family"] = family.to_json()
  object["scenario"] = family.to_json()
  object["track"] = track.to_json()
  object["complexity"] = tier
  object["variant"] = (0).to_json()
  object["cohort"] = (if delivery { "program-delivery" } else { "workflow" }).to_json()
  object["acceptance"] = (if delivery { "programs" } else { "workflow" }).to_json()
  object["required_source"] = if delivery {
    { "shell_tool": "solution.sh", "mbtx_program": "solution.mbtx" }
  } else {
    Json::null()
  }
  object["withheld"] = withheld.to_json()
  object["process_allow"] = allow.to_json()
  object["strict_worker_receipts"] = true.to_json()
  object["operation_receipts"] = true.to_json()
  let repeated_delivery = delivery &&
    field(case, "validation_phases") is Array(phases) &&
    phases.length() > 1 &&
    field(field(phases[1], "expected"), "executed") == Json::array([])
  if repeated_delivery {
    object["visible_result_policy"] = "cumulative_receipts_invocation_subset_v1"
  }
  object["goal"] = (v4_text(case, "goal") +
  " Write result.json with schema: " +
  v4_text(case, "output_schema") +
  ". Available direct programs and literal argv prefixes (the runtime policy uses this same list): " +
  forms.join("; ") +
  ". Help and rejected argument parsing are diagnostic requests; only validated job/recover operations count against state execution rules. " +
  (if delivery {
    "Save the single source named by your arm instructions, running without arguments from cwd. Fresh visible and two withheld inputs are validated without model assistance. Where a no-change pass is specified, result.executed describes that invocation only; the evaluator separately archives earlier results and receipts."
  } else {
    "No reusable source submission is required."
  }) +
  (if repeated_delivery {
    " The original workspace is checked against cumulative real worker receipts and final artifacts. Its final result.executed must be a sorted unique subset of IDs requiring work from the original inputs, since earlier invocations may already have completed some work. Saved source is independently run on fresh inputs, where executed is checked exactly for each invocation, including unchanged repeats."
  } else {
    ""
  })).to_json()
  Json::object(object)
}

///|
fn v4_basic(index : Int, variant : Int) -> Json {
  let case = long_case(index, true, variant, basic=true)
  let object = match case.json() {
    Object(o) => o
    _ => abort("case")
  }
  object["goal"] = case.instance.goal
    .replace_all(old="./fixture-worker", new="fixture-worker")
    .to_json()
  object["output_schema"] = case.instance.schema.to_json()
  object["reference_shell"] = ("set -eu\n" +
  case.instance.reference.replace_all(
    old="./fixture-worker",
    new="fixture-worker",
  )).to_json()
  object["reference_mbtx"] = (match index {
    1 | 8 => basic_reconciliation_reference()
    4 => basic_jobs_reference()
    5 | 9 => basic_recovery_reference()
    7 => basic_release_reference()
    _ => basic_jq_reference(case.instance.reference, index == 6)
  }).to_json()
  object["actions"] = (match index {
    4 => ["job"]
    5 | 9 => ["recover"]
    6 => ["emit"]
    7 => ["hash"]
    _ => []
  }).to_json()
  object["strict_worker_receipts"] = true.to_json()
  object["operation_receipts"] = true.to_json()
  object["exact_worker_actions"] = (match index {
    4 => ["job"]
    5 | 9 => ["recover"]
    _ => []
  }).to_json()
  if index == 3 {
    object["goal"] = (case.instance.goal +
    " Explicitly set config.schema_version to integer 2, including migrated.json. Preserve every unrelated field.").to_json()
  }
  if index == 7 {
    object["goal"] = (v4_text(Json::object(object), "goal") +
    " Obtain successful worker hash receipts for each selected candidate and final target using the exact workspace-relative path from release-plan.json, without a leading ./ or an absolute prefix. Additional hashes of rejected candidates and other files are allowed.").to_json()
  }
  let result = Json::object(object)
  if index == 9 {
    v4_with_repeat(result)
  } else {
    result
  }
}

///|
/// Freeze the independently expected no-change pass, keeping the first result.
fn v4_with_repeat(case : Json) -> Json {
  let first = match case {
    Object(o) => o.copy()
    _ => abort("case")
  }
  first["input_updates"] = Json::object({})
  let second = first.copy()
  let expected = match field(case, "expected") {
    Object(o) => o.copy()
    _ => abort("expected")
  }
  if expected.contains("executed") {
    expected["executed"] = Json::array([])
  }
  second["expected"] = Json::object(expected)
  let stable = v4_fields(case, "expected_outputs").keys().to_array()
  stable.sort_by(codepoint_compare)
  second["preserve_previous"] = stable.to_json()
  let events : Array[Json] = @json.from_json(field(case, "worker_events")) catch {
    _ => []
  }
  second["worker_events"] = events
    .filter(e => {
      field(e, "action") != Json::string("job") &&
      field(e, "action") != Json::string("recover")
    })
    .to_json()
  let object = match case {
    Object(o) => o.copy()
    _ => abort("case")
  }
  object["validation_phases"] = [Json::object(first), Json::object(second)].to_json()
  Json::object(object)
}

///|
fn v4_text(value : Json, key : String) -> String {
  match field(value, key) {
    String(text) => text
    _ => abort("fixture text field: " + key)
  }
}

///|
fn v4_fields(value : Json, key : String) -> Map[String, Json] {
  match field(value, key) {
    Object(fields) => fields
    _ => abort("fixture object field: " + key)
  }
}

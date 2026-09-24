///|
/// A small natural-tool-choice calibration protocol. It deliberately keeps
/// the task population narrow so that tool choice can be inspected before a
/// larger collection is attempted.
fn suite_natural_tool_choice() -> Json {
  let tasks = [
    natural_task(v4_basic(1, 911), "natural-data-reconciliation"),
    natural_task(v4_basic(0, 913), "natural-repository-impact"),
  ]
  {
    "schema_version": 1,
    "protocol_id": "natural-tool-choice-v3",
    "suite": "natural",
    "population": "Two small workflow tasks for observing unconstrained tool choice; descriptive calibration only.",
    "experiment": {
      "study": "agent-execution-backend-v1",
      "stage": "natural-calibration",
      "calibration_only": true,
      "execution_modes": ["direct", "code_mode"],
      "backend_arms": ["shell_tool", "mbtx_program"],
      "capability_profiles": ["production", "capability_matched"],
      "condition_source": "run-manifest",
    },
    "arms": ["shell_tool", "mbtx_program"],
    "seed": 20260924,
    "repeats": 1,
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
    "instructions": "Complete the task autonomously in cwd. Both arms receive the same inputs, filesystem boundary, model configuration and acceptance checks. Choose the implementation tools that best fit the task. You may inspect files, write helper files, run available project tools and verify the actual outputs. Do not use network access, delegation, other attempts or private oracle data. Preserve inputs unless the task permits a change, and verify the result before finishing. No continuation prompts or oracle feedback are provided.",
    "arm_instructions": {
      "shell_tool": "Use the normal execution tools available to you. You may choose Shell, Python, Node/JavaScript or other available project tools. Do not assume that a particular language, script filename or single-file layout is required.",
      "mbtx_program": "Use MBTX as the primary scripting and process entry point. MoonBit handles control flow, data transformation and file operations; @shell.Cmd starts task-declared programs with literal argv. The default direct process policy admits jq, rg, Git and the MoonBit toolchain for this calibration, while sh/bash, Python and Node are not direct MBTX entries. Do not assume a source filename is required.",
    },
    "tasks": tasks.to_json(),
    "validation": {
      "cases": 1,
      "source_limit_bytes": 65536,
      "deadline": "remaining_attempt_budget",
    },
    "limitations": [
      "This is a calibration protocol, not a performance estimate.", "The two tasks are selected to expose data-processing and repository-file workflows.",
      "The MBTX process policy constrains direct requests and does not claim complete descendant auditing.",
    ],
  }
}

///|
fn natural_task(task : Json, id : String) -> Json {
  let object = match task {
    Object(fields) => fields.copy()
    _ => abort("natural task")
  }
  object["id"] = id.to_json()
  object["required_source"] = Json::null()
  object["acceptance"] = "workflow".to_json()
  object["cohort"] = "natural-tool-choice".to_json()
  object["process_policy_profile"] = "natural-direct-process-v1".to_json()
  let contract = natural_output_contract(id)
  object["output_contract"] = contract
  let contract_text = "Output contract (identical for both execution backends):\n" +
    contract.stringify(indent=2)
  object["output_contract_text"] = contract_text.to_json()
  object["goal"] = (v4_text(task, "goal") +
  " Choose the implementation approach yourself; no source filename or reusable program is required. Preserve every input file byte-for-byte. Required output JSON is compared exactly after parsing: object key order and whitespace are ignored; array order, values, key names and extra object fields matter. Helper files are allowed and are not graded.\n\n" +
  contract_text).to_json()
  let allow : Array[Json] = [
    { "program": "jq", "args_prefix": [] },
    { "program": "rg", "args_prefix": ["--no-config", "--json", "--"] },
    { "program": "rg", "args_prefix": ["--no-config", "--files", "--"] },
    { "program": "git", "args_prefix": [] },
    { "program": "moon", "args_prefix": [] },
  ]
  object["process_allow"] = allow.to_json()
  let withheld : Array[Json] = @json.from_json(field(task, "withheld")) catch {
    _ => []
  }
  object["withheld"] = withheld
    .map(case => {
      let copy = match case {
        Object(fields) => fields.copy()
        _ => abort("natural withheld task")
      }
      copy["process_allow"] = allow.to_json()
      copy["process_policy_profile"] = "natural-direct-process-v1".to_json()
      Json::object(copy)
    })
    .to_json()
  Json::object(object)
}

///|
fn scenario_case(index : Int, variant : Int) -> StudyCase {
  match index / 6 {
    0 => repository_data_case(index, variant)
    1 => text_process_case(index - 6, variant)
    2 => diagnostics_repair_case(index - 12, variant)
    _ => bounded_recovery_case(index - 18, variant)
  }
}

///|
fn case_json(instance : StudyCase) -> Json {
  {
    "files": instance.files.to_json(),
    "expected": instance.expected,
    "output": "result.json",
  }
}

///|
/// Four content variants per scenario; two unseen cases use different sizes,
/// values and record orders. Repetitions are scheduled independently of inputs.
pub fn study_protocol(suite : String) -> Json raise {
  guard suite == "programs" || suite == "workflow" else {
    fail("Unknown study suite")
  }
  let names = [
    "nested-symbols", "dependency-order", "filtered-inventory", "grouped-totals",
    "relational-join", "time-windows", "unicode-records", "quoted-records", "multiline-records",
    "batch-children", "stdin-argv", "child-failure-recovery", "severity-counts",
    "request-correlation", "test-diagnostics", "config-migration", "constrained-repair",
    "config-precedence", "large-log", "frequency-topk", "multifile-aggregate", "checkpoint-replay",
    "idempotent-updates", "transaction-reconcile",
  ]
  let families = [
    "repository", "structured-data", "text", "process", "diagnostics", "repair",
    "bounded-output", "recovery",
  ]
  let tasks : Array[Json] = []
  for index, name in names {
    for variant in 0..<4 {
      let instance = scenario_case(index, variant)
      let submission = if suite == "programs" {
        " Deliver a self-contained reusable program in the assigned source file specified in your instructions. It must read the current working directory's inputs and write result.json without arguments. The evaluator will run ONLY this saved source (no auxiliary authored files) on fresh copies of this input and two unseen inputs following the same specification. No model assistance is available during validation. Do not hardcode this instance's answer. Common editing tools may create or repair program source. Verify the visible result too."
      } else {
        " Complete the workflow and verify result.json. Complete scripts, programs and common editing tools are allowed."
      }
      tasks.push({
        "id": (name + "-v\{variant + 1}").to_json(),
        "scenario": name.to_json(),
        "family": families[index / 3].to_json(),
        "variant": variant.to_json(),
        "acceptance": suite.to_json(),
        "goal": (instance.goal +
        " Write result.json with exactly this JSON schema: " +
        instance.schema +
        ". Do not change input files." +
        submission).to_json(),
        "output_schema": instance.schema.to_json(),
        "files": instance.files.to_json(),
        "expected": instance.expected,
        "output": "result.json",
        "reference_shell": ("set -eu\n" + instance.reference).to_json(),
        "withheld": [
          case_json(scenario_case(index, 11 + variant * 3)),
          case_json(scenario_case(index, 29 + variant * 5)),
        ],
      })
    }
  }
  let base = object_field(pilot_protocol(), "budgets")
  let version = if suite == "programs" { "v2" } else { "v1" }
  {
    "schema_version": 1,
    "protocol_id": ("programmable-steps-" + suite + "-" + version).to_json(),
    "population": "24 scenarios, four input variants, nested repetitions; configured interfaces",
    "suite": suite.to_json(),
    "arms": ["shell_tool", "mbtx_program"],
    "seed": 20260916,
    "repeats": 2,
    "budgets": base.to_json(),
    "pacing": field(pilot_protocol(), "pacing"),
    "instructions": field(pilot_protocol(), "instructions"),
    // The task and acceptance rules are shared; the required source language
    // and filename follow the assigned interface, never a model choice.
    "arm_instructions": if suite == "programs" {
      {
        "shell_tool": "This attempt uses the Shell interface. Save your complete reusable POSIX sh program as solution.sh in the workspace root. This exact file is the required submission. Execute it on the visible inputs and verify result.json before finishing. Temporary tool calls do not replace the saved submission.",
        "mbtx_program": "This attempt uses the MBTX interface. Save your complete reusable MoonBit program as solution.mbtx in the workspace root, with versioned single-file imports for the wasm target. This exact file is the required submission. Execute it on the visible inputs and verify result.json before finishing. Temporary mbtx tool programs do not replace the saved submission.",
      }
    } else {
      Json::null()
    },
    "observation": field(pilot_protocol(), "observation"),
    "tasks": tasks.to_json(),
    "validation": {
      "cases": 3,
      "case_timeout_seconds": 30,
      "build_timeout_seconds": 120,
      "source_limit_bytes": 131072,
    },
    "uncertainty": {
      "method": "stratified hierarchical paired bootstrap",
      "draws": 2000,
      "seed": 20260916,
    },
  }
}

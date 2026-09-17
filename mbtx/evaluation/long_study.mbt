///|
priv struct LongCase {
  instance : StudyCase
  outputs : Map[String, Json]
  mutable_inputs : Array[String]
  absent : Array[String]
  worker_events : Array[Json]
}

///|
fn long_case(index : Int, extended : Bool, variant : Int) -> LongCase {
  match index {
    0 => long_impact(extended, variant)
    1 | 8 => long_reconciliation(extended, variant)
    2 => long_incident(extended, variant)
    3 => long_migration(extended, variant)
    4 => long_jobs(extended, variant)
    5 | 9 => long_recovery(extended, variant)
    6 => long_forensics(extended, variant)
    _ => long_release(extended, variant)
  }
}

///|
fn LongCase::json(self : LongCase) -> Json {
  {
    "files": self.instance.files.to_json(),
    "expected": self.instance.expected,
    "output": "result.json",
    "expected_outputs": self.outputs.to_json(),
    "mutable_inputs": self.mutable_inputs.to_json(),
    "absent_outputs": self.absent.to_json(),
    "worker_events": self.worker_events.to_json(),
  }
}

///|
fn long_plain(instance : StudyCase) -> LongCase {
  { instance, outputs: Map([]), mutable_inputs: [], absent: [], worker_events: [] }
}

///|
/// Forty independently specified input instances, each repeated twice. Pilot
/// instances use a separate input seed and are never pooled with the study.
pub fn long_protocol(pilot : Bool) -> Json {
  let names = ["repository-impact", "data-reconciliation", "incident-investigation",
    "configuration-migration", "dependent-jobs", "incremental-recovery",
    "output-forensics", "release-verification", "reusable-data-program", "reusable-recovery-program"]
  let tasks : Array[Json] = []
  for index, name in names {
    for tier in 0..<(if pilot { 1 } else { 2 }) {
      for variant in 0..<(if pilot { 1 } else { 2 }) {
        let extended = if pilot { index % 2 == 1 } else { tier == 1 }
        let complexity = if extended { "extended" } else { "standard" }
        let input = if pilot { 101 + index } else { variant }
        let case = long_case(index, extended, input)
        let object = match case.json() { Object(object) => object; _ => abort("object") }
        let delivery = index >= 8
        object["id"] = (name + "-" + complexity + "-v\{variant + 1}").to_json()
        object["scenario"] = name.to_json()
        object["family"] = name.to_json()
        object["complexity"] = complexity.to_json()
        object["cohort"] = (if delivery { "program-delivery" } else { "workflow" }).to_json()
        object["variant"] = variant.to_json()
        object["acceptance"] = (if delivery { "programs" } else { "workflow" }).to_json()
        object["semantic_stages"] = (if extended { 9 } else { 5 }).to_json()
        object["goal"] = (case.instance.goal + " Write result.json with exactly this JSON schema: " + case.instance.schema + ". Preserve inputs except explicitly authorized changes. Work autonomously, verify your outputs, then finish with a final answer. Complete scripts and programs are allowed; no tool trajectory is prescribed." + (if delivery {
          " Also submit a reusable program: solution.sh for Shell or solution.mbtx for MBTX. It must read inputs from cwd and run without arguments or auxiliary authored files. The evaluator runs only this source on the visible inputs and two independently generated unseen inputs with the same specification. Verify the visible execution before finishing."
        } else { " No saved-program submission is required." })).to_json()
        object["output_schema"] = case.instance.schema.to_json()
        object["reference_shell"] = ("set -eu\n" + case.instance.reference).to_json()
        object["withheld"] = if delivery {
          [long_case(index, extended, 13 + variant * 3).json(), long_case(index, extended, 37 + variant * 7).json()].to_json()
        } else { Json::array([]) }
        tasks.push(Json::object(object))
      }
    }
  }
  {
    "schema_version": 1,
    "protocol_id": if pilot { "programmable-long-pilot-v1" } else { "programmable-long-study-v1" },
    "suite": if pilot { "long-pilot" } else { "long-study" },
    "population": "Eight workflow categories and two program-delivery categories; two complexity levels, two input variants, two nested repetitions. Pilot is separate.",
    "arms": ["shell_tool", "mbtx_program"], "seed": 20260916,
    "repeats": if pilot { 1 } else { 2 },
    "wall_seconds": 3600, "tool_output_tokens": 4096,
    "pacing": field(pilot_protocol(), "pacing"),
    "observation": field(pilot_protocol(), "observation"),
    "instructions": "Complete the task autonomously in the current workspace. The task specifies outcomes and constraints, not a command sequence. Both interfaces may use complete scripts/programs and common editing tools. Available utilities are rg, jq, sh, git and fixture-worker. Optional read_resource references: reference:moonbit, reference:shell, reference:tools and reference:examples. Read full output by its issued resource ID when a preview is insufficient. Reading documentation, compilation errors and recovery count as real task work. Preserve input files unless the task explicitly authorizes a change. Do not access the network, delegate, or inspect files outside the workspace or registered references. Verify the result and provide a final answer. No continuation prompts or oracle feedback will be supplied.",
    "arm_instructions": Json::null(),
    "tasks": tasks.to_json(),
    "validation": { "cases": 3, "case_timeout_seconds": 60, "build_timeout_seconds": 120, "source_limit_bytes": 65536 },
    "uncertainty": { "method": "paired bootstrap within fixed category and complexity strata, input variants and nested repetitions", "draws": 2000, "seed": 20260916 },
    "limitations": ["Task semantic stages are not imposed model steps.", "Successful-pair analysis is conditional and may have selection bias.", "Repeated executions do not create independent task definitions."],
  }
}

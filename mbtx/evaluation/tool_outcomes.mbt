///|
/// Keep invocation status distinct from compiler and child-process outcomes.
/// Missing native records or structured MBTX outputs remain unknown.
fn tool_outcomes(trace : Json, results : Json, complete~ : Bool) -> Json {
  guard field(trace, "tool_calls") is Object(tools) else {
    return { "coverage": false, "details": results }
  }
  let mut mbtx_calls = 0
  let mut completed = 0
  let mut edits = 0
  let mut compile_failed = 0
  let mut execution_failed = 0
  let mut preparation_failed = 0
  let mut covered = complete
  let mut resource_reads = 0
  let mut reference_reads = 0
  let mut cache_hits = 0
  let mut compilations = 0
  for id, tool in tools {
    let kind = field(tool, "kind")
    if field(kind,"name") == Json::string("read_resource") {
      resource_reads += 1
      let arguments = field(field(results,id),"arguments")
      if field(arguments,"resource_id") is String(id) && id.has_prefix("reference:") { reference_reads += 1 }
    }
    if field(kind, "type") == Json::string("apply_patch") {
      edits += 1
    }
    if field(kind, "name") != Json::string("mbtx") {
      continue
    }
    mbtx_calls += 1
    let output = field(field(results, id), "output")
    if field(output,"cache") == Json::string("hit") { cache_hits += 1 }
    else if field(output,"build") != Json::null() { compilations += 1 }
    match (field(output, "status"), field(output, "stage")) {
      (String("success"), String("completed")) => completed += 1
      (String("error"), String("compilation")) => compile_failed += 1
      (String("error"), String("execution" | "completed")) =>
        execution_failed += 1
      (String("error"), String("preparation")) => preparation_failed += 1
      _ => covered = false
    }
  }
  let count = fn(value : Int) -> Json {
    if covered {
      value.to_json()
    } else {
      Json::null()
    }
  }
  let terminal_failures = if field(trace, "terminal_operations")
    is Object(operations) {
    let mut failures = 0
    let mut terminal_covered = complete
    for _, operation in operations {
      match field(field(operation, "execution"), "status") {
        String("failed") => failures += 1
        String("completed" | "cancelled") => ()
        _ => terminal_covered = false
      }
    }
    if terminal_covered {
      failures.to_json()
    } else {
      Json::null()
    }
  } else {
    Json::null()
  }
  {
    "mbtx_calls_observed": mbtx_calls.to_json(),
    "resource_reads_observed": resource_reads.to_json(),
    "reference_reads_observed": reference_reads.to_json(),
    "mbtx_cache_hits": count(cache_hits),
    "mbtx_compilations": count(compilations),
    "mbtx_completed_calls": count(completed),
    "mbtx_compile_failures": count(compile_failed),
    "mbtx_execution_failures": count(execution_failed),
    "mbtx_preparation_failures": count(preparation_failed),
    "shell_command_failures": terminal_failures,
    "shared_edit_calls": if complete {
      edits.to_json()
    } else {
      Json::null()
    },
    "mbtx_result_coverage": covered.to_json(),
    "details": results,
    "interpretation": "Observed layers can overlap; tool return success does not imply child success or task correctness. Completed MBTX calls and shared edits do not identify which program produced the task output.",
  }
}

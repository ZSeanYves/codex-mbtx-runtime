///|
/// Join model decisions to actual tool results; no inferred repair accounting.
fn interaction_statistics(accounting : Json, results : Json) -> Json {
  let steps : Array[Json] = @json.from_json(field(accounting, "steps")) catch {
    _ => []
  }
  let mut first_compile : Json = Json::null()
  let mut reference_first = 0
  let mut reference_continuations = 0
  let mut output_continuations = 0
  let mut unknown_reads = 0
  let decisions : Array[Json] = []
  for index, step in steps {
    let emitted : Array[Json] = @json.from_json(field(step, "emitted_tools")) catch {
      _ => []
    }
    let kinds : Array[String] = []
    for emitted in emitted {
      let call = field(emitted, "observation")
      let id = match field(call, "call_id") {
        String(id) => id
        _ => ""
      }
      let details = field(results, id)
      let output = field(details, "output")
      match field(call, "tool_name") {
        String("read_resource") => {
          let arguments = field(details, "arguments")
          let offset : Int = @json.from_json(field(arguments, "offset")) catch {
            _ => 0
          }
          match field(arguments, "resource_id") {
            String(id) if id.has_prefix("reference:") =>
              if offset == 0 {
                reference_first += 1
                kinds.push("reference_first_page")
              } else {
                reference_continuations += 1
                kinds.push("reference_continuation")
              }
            String(_) => {
              if offset > 0 {
                output_continuations += 1
              }
              kinds.push("output_resource")
            }
            _ => {
              unknown_reads += 1
              kinds.push("resource_unknown")
            }
          }
        }
        String("mbtx") => {
          if field(field(output, "build"), "exit_code") == (0).to_json() &&
            first_compile == Json::null() {
            first_compile = (index + 1).to_json()
          }
          kinds.push(
            match (field(output, "status"), field(output, "stage")) {
              (String("error"), String("compilation")) => "compile_failure"
              (String("error"), String("execution" | "completed")) =>
                "runtime_failure"
              (String("success"), String("completed")) => "program_success"
              _ => "mbtx_other_or_unknown"
            },
          )
        }
        String(name) => kinds.push(name)
        _ => kinds.push("unknown_tool")
      }
    }
    decisions.push({
      "step": (index + 1).to_json(),
      "step_id": field(step, "step_id"),
      "activities": kinds.to_json(),
    })
  }
  {
    "confidence": "observed",
    "first_successful_compile_step": first_compile,
    "reference_first_pages_observed": reference_first.to_json(),
    "reference_continuations_observed": reference_continuations.to_json(),
    "output_continuations_observed": output_continuations.to_json(),
    "resource_reads_unknown": unknown_reads.to_json(),
    "decisions": decisions.to_json(),
    "interpretation": "Tool-linked activities may overlap within a decision. First successful compile is not task completion; missing first success remains unknown, and these are not repair-step counts.",
  }
}

///|
/// Validate captured output bytes and input preservation independently of model
/// claims. Snapshot errors are unknown, not a zero-step task success.
pub fn task_oracle(task : Json, snapshot : Json) -> Json raise {
  let output = string_field(task, "output")
  if field(snapshot, "errors") is Array(errors) && !errors.is_empty() {
    return {
      "success": Json::null(),
      "reason": "Output snapshot incomplete",
      "confidence": "unknown",
    }
  }
  let files = object_field(snapshot, "files")
  let mutable_inputs : Array[Json] = @json.from_json(
    field(task, "mutable_inputs"),
  ) catch {
    _ => []
  }
  for path, content in object_field(task, "files") {
    if path != output &&
      !mutable_inputs.contains(path.to_json()) &&
      files.get(path) != Some(content) {
      return {
        "success": false,
        "reason": "An input file was modified or removed",
        "confidence": "observed",
      }
    }
  }
  let parsed = match files.get(output) {
    Some(String(content)) => @json.parse(content) catch { _ => Json::null() }
    _ => Json::null()
  }
  if parsed == field(task, "expected") &&
    field(task, "expected_outputs") != Json::null() {
    return long_oracle(task, snapshot)
  }
  if parsed != field(task, "expected") {
    return artifact_mismatch(
      output,
      field(task, "expected"),
      field(field(snapshot, "files"), output),
    )
  }
  {
    "success": (parsed == field(task, "expected")).to_json(),
    "reason": "Independent structured output comparison",
    "confidence": "observed",
  }
}

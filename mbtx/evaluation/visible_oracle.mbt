///|
/// The model may run a recovery program more than once. The final workspace
/// has cumulative receipts, but no trustworthy boundary for its last invocation.
/// Fresh submission phases still use the exact task oracle independently.
fn visible_task_oracle(task : Json, snapshot : Json) -> Json raise {
  if field(task, "visible_result_policy") !=
    Json::string("cumulative_receipts_invocation_subset_v1") ||
    field(task, "acceptance") != Json::string("programs") {
    return task_oracle(task, snapshot)
  }
  if field(snapshot, "errors") is Array(errors) && !errors.is_empty() {
    return task_oracle(task, snapshot)
  }
  let output = string_field(task, "output")
  let parsed = match field(field(snapshot, "files"), output) {
    String(content) => @json.parse(content) catch { _ => Json::null() }
    _ => Json::null()
  }
  guard field(task, "expected") is Object(expected) &&
    field(field(task, "expected"), "executed") is Array(required) &&
    field(parsed, "executed") is Array(executed) else {
    return task_oracle(task, snapshot)
  }
  let mut prior : String? = None
  for item in executed {
    let ordered = match (prior, item) {
      (None, String(_)) => true
      (Some(previous), String(current)) =>
        codepoint_compare(previous, current) < 0
      _ => false
    }
    if !ordered || !required.contains(item) {
      return {
        "success": false,
        "reason": "Visible executed IDs must be a sorted unique subset of the original required work",
        "confidence": "observed",
      }
    }
    prior = Some(@json.from_json(item))
  }
  let visible_expected = expected.copy()
  visible_expected["executed"] = executed.to_json()
  let visible_task = match task {
    Object(fields) => fields.copy()
    _ => abort("task object")
  }
  visible_task["expected"] = visible_expected.to_json()
  let checked = task_oracle(visible_task.to_json(), snapshot)
  let result = match checked {
    Object(fields) => fields.copy()
    _ => abort("oracle result object")
  }
  result["executed_claim"] = {
    "confidence": "unknown",
    "reason": "The final visible list has a valid subset shape, but the snapshot cannot identify the last invocation. This oracle requires final artifacts and cumulative receipts; fresh delivery phases independently require exact per-invocation execution.",
  }
  result.to_json()
}

///|
/// Audit the public natural-task contract independently of the delivered answer.
/// Older protocols without a machine-checkable public contract remain unaudited.
fn output_contract_audit(task : Json) -> Json raise {
  if field(task, "cohort") != "natural-tool-choice".to_json() {
    return { "status": "not_audited", "errors": [] }
  }
  let errors : Array[Json] = []
  let contract = field(task, "output_contract")
  let id = string_field(task, "id")
  if !["natural-data-reconciliation", "natural-repository-impact"].contains(id) {
    errors.push("Unrecognized natural task contract".to_json())
  } else if contract != natural_output_contract(id) {
    errors.push("Missing or inconsistent public output contract".to_json())
  }
  let prefix = "Output contract (identical for both execution backends):\n"
  let text = v4_text(task, "output_contract_text")
  let published = if text.has_prefix(prefix) {
    @json.parse(text[prefix.length():]) catch {
      _ => Json::null()
    }
  } else {
    Json::null()
  }
  if published != contract ||
    contract == Json::null() ||
    !v4_text(task, "goal").has_suffix(text) {
    errors.push(
      "Public goal does not contain the complete frozen output contract".to_json(),
    )
  }
  let artifacts = match field(task, "expected_outputs") {
    Object(items) => items.copy()
    _ => Map([])
  }
  let output = string_field(task, "output")
  if artifacts.get(output) is Some(expected) &&
    expected != field(task, "expected") {
    errors.push("Primary and additional artifact oracles disagree".to_json())
  }
  artifacts[output] = field(task, "expected")
  let schemas = match field(contract, "files") {
    Object(items) => items
    _ => Map([])
  }
  if schemas.length() != artifacts.length() {
    errors.push("Public contract and grader artifact sets differ".to_json())
  }
  for path, expected in artifacts {
    if schemas.get(path) is Some(schema) {
      if !output_schema_accepts(schema, expected) {
        errors.push(("Oracle violates public schema: " + path).to_json())
      }
    } else {
      errors.push(
        ("Graded artifact is absent from public contract: " + path).to_json(),
      )
    }
  }
  {
    "status": (if errors.is_empty() { "verified" } else { "invalid" }).to_json(),
    "errors": errors.to_json(),
  }
}

///|
fn output_schema_accepts(schema : Json, value : Json) -> Bool {
  match (field(schema, "type"), value) {
    (String("string"), String(_)) => true
    (String("integer"), Number(n, ..)) =>
      !n.is_inf() && !n.is_nan() && n.trunc() == n
    (String("array"), Array(values)) =>
      values.all(item => output_schema_accepts(field(schema, "items"), item))
    (String("object"), Object(values)) => {
      let required = match field(schema, "required") {
        Array(items) => items
        _ => []
      }
      if !required.all(key => key is String(name) && values.contains(name)) {
        return false
      }
      let properties = match field(schema, "properties") {
        Object(items) => items
        _ => Map([])
      }
      for name, item in values {
        match properties.get(name) {
          Some(property) =>
            if !output_schema_accepts(property, item) {
              return false
            }
          None =>
            match field(schema, "additionalProperties") {
              Object(_) as additional =>
                if !output_schema_accepts(additional, item) {
                  return false
                }
              True => ()
              _ => return false
            }
        }
      }
      true
    }
    _ => false
  }
}

///|
/// Bounded JSON Pointer diagnostics; does not weaken exact acceptance rules.
fn output_differences(
  expected : Json,
  actual : Json,
  pointer : String,
  result : Array[Json],
) -> Unit {
  if expected == actual || result.length() >= 32 {
    return
  }
  match (expected, actual) {
    (Object(wanted), Object(found)) => {
      let keys = wanted.keys().to_array()
      keys.sort()
      for key in keys {
        let path = pointer +
          "/" +
          key.replace_all(old="~", new="~0").replace_all(old="/", new="~1")
        match found.get(key) {
          Some(value) => output_differences(wanted[key], value, path, result)
          None =>
            if result.length() < 32 {
              result.push({ "kind": "missing_field", "pointer": path.to_json() })
            }
        }
      }
      let keys = found.keys().to_array()
      keys.sort()
      for key in keys {
        if !wanted.contains(key) && result.length() < 32 {
          result.push({
            "kind": "unexpected_field",
            "pointer": (pointer +
            "/" +
            key.replace_all(old="~", new="~0").replace_all(old="/", new="~1")).to_json(),
          })
        }
      }
    }
    (Array(wanted), Array(found)) => {
      if wanted.length() != found.length() {
        result.push({
          "kind": "array_length",
          "pointer": pointer.to_json(),
          "expected": wanted.length().to_json(),
          "actual": found.length().to_json(),
        })
      }
      for i in 0..<Int::min(wanted.length(), found.length()) {
        output_differences(wanted[i], found[i], pointer + "/\{i}", result)
      }
    }
    _ => result.push({ "kind": "value_mismatch", "pointer": pointer.to_json() })
  }
}

///|
fn artifact_mismatch(path : String, expected : Json, content : Json) -> Json {
  let differences : Array[Json] = []
  let reason = if content == Json::null() {
    differences.push({ "kind": "missing_file", "pointer": "" })
    "Required artifact is missing"
  } else {
    match content {
      String(text) => {
        let actual = @json.parse(text) catch {
          _ => {
            differences.push({ "kind": "invalid_json", "pointer": "" })
            Json::null()
          }
        }
        if differences.is_empty() {
          output_differences(expected, actual, "", differences)
        }
      }
      _ => differences.push({ "kind": "invalid_snapshot", "pointer": "" })
    }
    "Required artifact differs from the task contract"
  }
  {
    "success": false,
    "reason": reason.to_json(),
    "confidence": "observed",
    "artifact": path.to_json(),
    "differences": differences.to_json(),
    "difference_limit": 32,
  }
}

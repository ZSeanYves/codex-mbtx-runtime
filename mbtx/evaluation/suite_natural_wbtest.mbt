///|
test "natural tool-choice protocol leaves implementation language open" {
  let protocol = suite_natural_tool_choice()
  assert_eq(field(protocol, "protocol_id"), "natural-tool-choice-v3".to_json())
  let experiment = field(protocol, "experiment")
  assert_eq(field(experiment, "stage"), "natural-calibration".to_json())
  assert_eq(field(experiment, "calibration_only"), true.to_json())
  assert_true(
    field(experiment, "execution_modes") == ["direct", "code_mode"].to_json(),
  )
  assert_true(
    field(experiment, "capability_profiles") ==
    ["production", "capability_matched"].to_json(),
  )
  let tasks : Array[Json] = @json.from_json(field(protocol, "tasks"))
  assert_eq(tasks.length(), 2)
  for task in tasks {
    assert_eq(field(task, "acceptance"), "workflow".to_json())
    assert_eq(field(task, "required_source"), Json::null())
    assert_eq(
      field(task, "process_policy_profile"),
      "natural-direct-process-v1".to_json(),
    )
    let programs : Array[Json] = match field(task, "process_allow") {
      Array(items) => items
      _ => []
    }
    assert_true(programs.any(rule => field(rule, "program") == "git".to_json()))
    assert_true(
      programs.any(rule => field(rule, "program") == "moon".to_json()),
    )
  }
}

///|
test "natural prompts publish every exact artifact contract without answers" {
  let tasks : Array[Json] = @json.from_json(
    field(suite_natural_tool_choice(), "tasks"),
  )
  for task in tasks {
    let contract = field(task, "output_contract")
    let text = v4_text(task, "output_contract_text")
    assert_true(v4_text(task, "goal").contains(text))
    assert_true(text.contains(contract.stringify(indent=2)))
    let artifacts = object_field(contract, "files").keys().to_array()
    let graded = object_field(task, "expected_outputs").keys().to_array()
    graded.push(v4_text(task, "output"))
    artifacts.sort()
    graded.sort()
    assert_eq(artifacts, graded)
    assert_true(!text.contains(field(task, "expected").stringify()))
    assert_eq(
      field(task_oracle(task, v4_snapshot(task)), "success"),
      true.to_json(),
    )
    // Output contracts cannot quietly become advisory: extra fields and a
    // renamed required result must still fail the now-public exact rules.
    let files = object_field(v4_snapshot(task), "files").copy()
    let result = object_field(task, "expected").copy()
    result["unexpected"] = true.to_json()
    files["result.json"] = Json::object(result).stringify().to_json()
    assert_eq(
      field(task_oracle(task, { "files": files.to_json() }), "success"),
      false.to_json(),
    )
    files["impact.json"] = field(task, "expected").stringify().to_json()
    files.remove("result.json")
    assert_eq(
      field(task_oracle(task, { "files": files.to_json() }), "success"),
      false.to_json(),
    )
  }
}

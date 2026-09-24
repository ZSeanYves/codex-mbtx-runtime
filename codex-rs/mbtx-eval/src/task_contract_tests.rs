use serde_json::json;

#[test]
fn forwarding_requires_the_exact_public_goal_recorded_before_execution() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let task = json!({"cohort":"natural-tool-choice","goal":"Work. Output contract: result.json has total.","output_contract":{"files":{"result.json":{}}},"expected":{"total":3},"expected_outputs":{}});
    super::record(temp.path(), &task, "Complete the task.")?;
    let receipt = crate::evidence::read_json(&temp.path().join("task-contract.json"))?;
    let mut body = json!({"input":[{"role":"user","content":[{"text":task["goal"]}]},{"role":"developer","content":[{"text":"Complete the task."}]}]});
    super::validate(&body, &receipt)?;
    body["input"][0]["content"][0]["text"] = json!("Work.");
    assert!(super::validate(&body, &receipt).is_err());
    body["input"][0]["content"][0]["text"] = task["goal"].clone();
    body["input"][0]["role"] = json!("assistant");
    assert!(super::validate(&body, &receipt).is_err());
    body["input"][0]["role"] = json!("user");
    body["input"][1]["content"][0]["text"] = json!("Incomplete instructions");
    assert!(super::validate(&body, &receipt).is_err());
    Ok(())
}

#[test]
fn process_prompt_uses_enforced_rules_and_does_not_restrict_production_shell() -> anyhow::Result<()>
{
    let rules = json!([
        {"program":"rg","args_prefix":["--no-config","--files","--"]},
        {"program":"jq","args_prefix":[]}
    ]);
    let mut task = json!({"cohort":"natural-tool-choice","process_policy_profile":"natural-direct-process-v1","process_allow":rules});
    let instructions = super::process_instructions(&task, "mbtx_program")?;
    assert!(instructions.contains(&serde_json::to_string_pretty(&rules)?));
    assert_eq!(super::process_instructions(&task, "shell_tool")?, "");
    task["process_allow"][0]["args_prefix"] = json!(["--files"]);
    assert!(super::process_instructions(&task, "mbtx_program").is_err());
    Ok(())
}

use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn delivery_reference_is_independent_of_expected_answers_and_fixture_values() -> Result<()> {
    for family in ["reusable-data-program", "reusable-recovery-program"] {
        let task = json!({"family":family,"reference_mbtx":"async fn main { println(42) }","acceptance":"programs","required_source":{"mbtx_program":"solution.mbtx"},"files":{"input":"first"},"expected":{"answer":"first"},"withheld":[]});
        let mut changed = task.clone();
        changed["files"] = json!({"unseen":"completely different"});
        changed["expected"] = json!({"answer":"withheld answer must never appear"});
        changed["withheld"] = json!([{"files":{"unseen":"hidden"},"expected":"hidden"}]);
        assert_eq!(source(&task)?, source(&changed)?);
    }
    Ok(())
}

#[test]
fn prescribed_delivery_saves_exact_source_and_does_not_spawn_shell() -> Result<()> {
    let task = json!({"family":"reusable-recovery-program","acceptance":"programs","required_source":{"mbtx_program":"solution.mbtx"},"output":"result.json","reference_shell":"sh never-execute-this.sh","reference_mbtx":"import {\"moonbitlang/async@0.21.3\",\"moonbitlang/async@0.21.3/shell\"}\nasync fn main { @shell.Cmd(\"fixture-worker\",[\"echo\"]).run() }"});
    let replies = crate::replay::fixed(&task, "mbtx_program", "direct", /*prefix_turns*/ 0)?;
    let event = String::from_utf8(replies[0].body.clone())?
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(serde_json::from_str::<Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .find(|value| value["type"] == "response.output_item.done")
        .context("tool event")?;
    let arguments: Value =
        serde_json::from_str(event["item"]["arguments"].as_str().context("arguments")?)?;
    let source = arguments["source"].as_str().context("source")?;
    assert!(source.contains("@fs.write_file(\"solution.mbtx\","));
    assert!(!source.contains("never-execute-this"));
    assert!(!source.contains("Cmd(\"sh\""));
    assert!(!source.contains("solution.sh"));
    assert!(source.contains("Cmd(\"fixture-worker\""));
    Ok(())
}

#[test]
fn missing_or_oversized_reference_cannot_silently_become_an_oracle_answer() {
    assert!(source(&json!({"expected":{"answer":42}})).is_err());
    assert!(source(&json!({"reference_mbtx":"x".repeat(65_537)})).is_err());
}

use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

fn delivery() -> Value {
    json!({"acceptance":"programs","required_source":{"shell_tool":"solution.sh","mbtx_program":"solution.mbtx"}})
}

#[test]
fn explicit_contract_rejects_wrong_arm_source_and_inconsistent_cohorts() -> Result<()> {
    assert_eq!(
        required_source(&delivery(), "mbtx_program")?,
        Some("solution.mbtx".to_owned())
    );
    assert_eq!(
        required_source(&delivery(), "shell_tool")?,
        Some("solution.sh".to_owned())
    );
    assert_eq!(
        required_source(
            &json!({"acceptance":"workflow","required_source":null}),
            "mbtx_program"
        )?,
        None
    );
    for task in [
        json!({"acceptance":"programs","required_source":null}),
        json!({"acceptance":"programs","required_source":{"mbtx_program":"solution.sh"}}),
        json!({"acceptance":"workflow","required_source":{"mbtx_program":"solution.mbtx"}}),
    ] {
        assert!(required_source(&task, "mbtx_program").is_err());
    }
    Ok(())
}

#[test]
fn legacy_delivery_contract_remains_readable() -> Result<()> {
    assert_eq!(
        required_source(&json!({"acceptance":"programs"}), "mbtx_program")?,
        Some("solution.mbtx".to_owned())
    );
    assert_eq!(
        required_source(&json!({"acceptance":"workflow"}), "mbtx_program")?,
        None
    );
    Ok(())
}

#[tokio::test]
async fn shell_file_is_never_implicitly_accepted_as_mbtx_delivery() -> Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("solution.sh"), b"printf correct")?;
    let cancellation = crate::cancellation::Cancellation::listen()?;
    let validator = crate::submission::Validator {
        bundle: root.path(),
        bundle_info: &Value::Null,
        path: "",
        utilities: &Value::Null,
        cancellation: &cancellation,
    };
    let facts = validator
        .validate(
            &delivery(),
            "mbtx_program",
            root.path(),
            &root.path().join("work"),
            &root.path().join("evidence"),
            None,
        )
        .await?;
    assert_eq!(
        facts,
        json!({"status":"missing_source","expected_source":"solution.mbtx","source":null,"build":null,"cases":[]})
    );
    assert!(!root.path().join("work").exists());
    Ok(())
}

#[tokio::test]
async fn oversized_and_non_utf8_sources_are_rejected_before_build() -> Result<()> {
    for bytes in [vec![b'a'; SOURCE_LIMIT_BYTES as usize + 1], vec![255]] {
        let root = tempfile::tempdir()?;
        std::fs::write(root.path().join("solution.mbtx"), bytes)?;
        let cancellation = crate::cancellation::Cancellation::listen()?;
        let validator = crate::submission::Validator {
            bundle: root.path(),
            bundle_info: &Value::Null,
            path: "",
            utilities: &Value::Null,
            cancellation: &cancellation,
        };
        let facts = validator
            .validate(
                &delivery(),
                "mbtx_program",
                root.path(),
                &root.path().join("work"),
                &root.path().join("evidence"),
                None,
            )
            .await?;
        assert_eq!(
            facts,
            json!({"status":"invalid_source","source":null,"build":null,"cases":[]})
        );
        assert!(!root.path().join("work").exists());
    }
    Ok(())
}

#[tokio::test]
async fn expired_attempt_budget_captures_source_without_starting_validation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let bytes = b"fn main { println(1) }";
    std::fs::write(root.path().join("solution.mbtx"), bytes)?;
    let cancellation = crate::cancellation::Cancellation::listen()?;
    let validator = crate::submission::Validator {
        bundle: root.path(),
        bundle_info: &Value::Null,
        path: "",
        utilities: &Value::Null,
        cancellation: &cancellation,
    };
    let facts = validator
        .validate(
            &delivery(),
            "mbtx_program",
            root.path(),
            &root.path().join("work"),
            &root.path().join("evidence"),
            Some(std::time::Instant::now()),
        )
        .await?;
    assert_eq!(
        facts,
        json!({"status":"attempt_timeout","source":{"file":"solution.mbtx","sha256":crate::evidence::digest(bytes),"bytes":bytes.len()},"build":null,"cases":[],"scope":"post-submission validation; no model steps or relay calls","process_spawns":null,"phase_counts":{"compiler_starts":0,"program_starts":0},"elapsed_ns":0,"timeout_phase":"before_validation"})
    );
    assert_eq!(
        std::fs::read(root.path().join("evidence/solution.mbtx"))?,
        bytes
    );
    assert!(!root.path().join("work/compile").exists());
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn linked_source_is_rejected_even_when_target_is_inside_workspace() -> Result<()> {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("other.mbtx"), b"fn main {}")?;
    std::os::unix::fs::symlink("other.mbtx", root.path().join("solution.mbtx"))?;
    let cancellation = crate::cancellation::Cancellation::listen()?;
    let validator = crate::submission::Validator {
        bundle: root.path(),
        bundle_info: &Value::Null,
        path: "",
        utilities: &Value::Null,
        cancellation: &cancellation,
    };
    let facts = validator
        .validate(
            &delivery(),
            "mbtx_program",
            root.path(),
            &root.path().join("work"),
            &root.path().join("evidence"),
            None,
        )
        .await?;
    assert_eq!(
        facts,
        json!({"status":"invalid_source","source":null,"build":null,"cases":[]})
    );
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn delivery_preparation_obeys_remaining_deadline_and_interruption() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;
    use std::time::Instant;
    for mode in ["timeout", "interrupt"] {
        let root = tempfile::tempdir()?;
        let source = root.path().join("source");
        let tools = root.path().join("tools");
        std::fs::create_dir(&source)?;
        std::fs::create_dir(&tools)?;
        std::fs::write(source.join("solution.sh"), "exit 0\n")?;
        std::fs::write(root.path().join("fixture-worker"), "unused")?;
        let before_wait = if mode == "interrupt" {
            "kill -INT \"$PPID\"\n"
        } else {
            ""
        };
        std::fs::write(
            tools.join("git"),
            format!("#!/bin/sh\n{before_wait}exec /bin/sleep 30\n"),
        )?;
        std::fs::set_permissions(tools.join("git"), std::fs::Permissions::from_mode(0o755))?;
        let cancellation = crate::cancellation::Cancellation::listen()?;
        let validator = crate::submission::Validator {
            bundle: root.path(),
            bundle_info: &Value::Null,
            path: tools.to_str().unwrap(),
            utilities: &Value::Null,
            cancellation: &cancellation,
        };
        let mut task = delivery();
        task["files"] = json!({});
        task["withheld"] = json!([]);
        let started = Instant::now();
        let budget = if mode == "timeout" {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(5)
        };
        let facts = validator
            .validate(
                &task,
                "shell_tool",
                &source,
                &root.path().join("work"),
                &root.path().join("evidence"),
                Some(started + budget),
            )
            .await?;
        assert_eq!(
            facts["status"],
            if mode == "timeout" {
                "attempt_timeout"
            } else {
                "attempt_cancelled"
            }
        );
        assert_eq!(facts["interrupted_phase"], "case_0_preparation");
        assert_eq!(
            facts["phase_counts"],
            json!({"compiler_starts":0,"program_starts":0})
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(root.path().join("evidence/case-0/workspace.json").exists());
    }
    Ok(())
}

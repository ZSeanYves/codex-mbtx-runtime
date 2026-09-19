//! Real compiler/sandbox checks, explicitly selected after preparing a bundle.
use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
#[ignore = "requires MBTX_TEST_BUNDLE and a working local Codex sandbox"]
async fn fresh_delivery_rejects_invalid_source_auxiliary_dependencies_and_visible_answers()
-> Result<()> {
    let bundle = std::path::PathBuf::from(std::env::var("MBTX_TEST_BUNDLE")?).canonicalize()?;
    let info = crate::evidence::read_json(&bundle.join("bundle.json"))?;
    let path = std::env::var("PATH")?;
    let cancellation = crate::cancellation::Cancellation::listen()?;
    let validator = Validator {
        bundle: &bundle,
        bundle_info: &info,
        path: &path,
        utilities: &Value::Null,
        cancellation: &cancellation,
    };
    let case =
        |text: &str| json!({"files":{"input.txt":text},"output":"result.json","expected":text});
    let mut task = case("visible");
    task["acceptance"] = json!("programs");
    task["required_source"] = json!({"shell_tool":"solution.sh","mbtx_program":"solution.mbtx"});
    task["process_allow"] = json!([]);
    task["withheld"] = json!([case("unseen 雪"), case("different\r\n")]);
    let prefix = "import {\"moonbitlang/async@0.21.3\",\"moonbitlang/async@0.21.3/fs\",\"moonbitlang/core/json\"}\nasync fn main { ";
    let generic = format!(
        "{prefix}@fs.write_file(\"result.json\", @fs.read_file(\"input.txt\").text().to_json().stringify()) }}"
    );
    let auxiliary = format!(
        "{prefix}@fs.write_file(\"result.json\", @fs.read_file(\"private-answer.txt\").text()) }}"
    );
    let hardcoded =
        format!("{prefix}@fs.write_file(\"result.json\", \"visible\".to_json().stringify()) }}");
    let denied = "import {\"moonbitlang/async@0.21.3\",\"moonbitlang/async@0.21.3/shell\"}\nasync fn main { @shell.Cmd(\"sh\",[\"-c\",\"exit 0\"]).run() }";
    let mut analysis = crate::analysis::Analysis::start(&bundle).await?;
    for (label, source) in [
        ("generic", generic.as_str()),
        ("invalid", "this is not MoonBit"),
        ("auxiliary", auxiliary.as_str()),
        ("hardcoded", hardcoded.as_str()),
        ("denied", denied),
    ] {
        let root = tempfile::tempdir()?;
        let original = root.path().join("original");
        fs::create_dir(&original)?;
        fs::write(original.join("solution.mbtx"), source)?;
        fs::write(original.join("private-answer.txt"), "\"visible\"")?;
        let evidence = root.path().join("evidence");
        let validation = validator
            .validate(
                &task,
                "mbtx_program",
                &original,
                &root.path().join("work"),
                &evidence,
                Some(Instant::now() + Duration::from_secs(180)),
            )
            .await?;
        if label == "invalid" {
            assert_eq!(validation["status"], json!("build_failed"), "{validation}");
            continue;
        }
        assert_eq!(
            validation["status"],
            json!("captured"),
            "{validation}\n{}",
            fs::read_to_string(evidence.join("build/stderr"))?
        );
        assert_eq!(
            validation["phase_counts"],
            json!({"compiler_starts":1,"program_starts":3})
        );
        let result = analysis.query(json!({"op":"attempt","task":task,"facts":{
            "trace":null,"exchanges":[],"snapshot":{"files":{"input.txt":"visible","result.json":"\"visible\""},"errors":[]},
            "outcome":{"termination":"exited","exit_code":0},"submission":validation
        }})).await?;
        assert_eq!(
            result["oracle"]["success"],
            json!(label == "generic"),
            "{label}: {result}"
        );
        if label == "hardcoded" {
            let successes = result["oracle"]["cases"]
                .as_array()
                .context("case outcomes")?
                .iter()
                .map(|case| case["success"].clone())
                .collect::<Vec<_>>();
            assert_eq!(successes, vec![json!(true), json!(false), json!(false)]);
        }
        for index in 0..3 {
            let policy =
                crate::evidence::read_json(&evidence.join(format!("case-{index}/policy.json")))?;
            assert_eq!(policy["process"], json!({"allow":[]}));
            if label == "denied" {
                assert!(
                    fs::read_to_string(evidence.join(format!("case-{index}/execution/stderr")))?
                        .contains("Sandbox policy blocked process spawn")
                );
            }
        }
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires MBTX_TEST_BUNDLE and a working local Codex sandbox"]
async fn delivery_retains_state_applies_only_input_transitions_and_archives_each_phase()
-> Result<()> {
    let bundle = std::path::PathBuf::from(std::env::var("MBTX_TEST_BUNDLE")?).canonicalize()?;
    let info = crate::evidence::read_json(&bundle.join("bundle.json"))?;
    let path = std::env::var("PATH")?;
    let cancellation = crate::cancellation::Cancellation::listen()?;
    let validator = Validator {
        bundle: &bundle,
        bundle_info: &info,
        path: &path,
        utilities: &Value::Null,
        cancellation: &cancellation,
    };
    let phase = |input: &str, count: u64, updates: Value| {
        json!({
            "files":{"input.txt":input,"keep.bin":"unchanged\u{0000}\r\n"},
            "input_updates":updates,"preserve_previous":["keep.bin"],
            "output":"result.json","expected":{"count":count,"input":input},
            "expected_outputs":{"state.txt":format!("{count}\n")},
            "absent_outputs":[],"worker_events":[],"mutable_inputs":["state.txt"]
        })
    };
    let initial = phase("first", 1, json!({}));
    let mut task = initial.clone();
    task["validation_phases"] = json!([
        initial,
        phase("second", 2, json!({"input.txt":"second"})),
        phase("second", 3, json!({}))
    ]);
    task["acceptance"] = json!("programs");
    task["required_source"] = json!({"shell_tool":"solution.sh","mbtx_program":"solution.mbtx"});
    task["process_allow"] = json!([]);
    task["withheld"] = json!([]);
    let root = tempfile::tempdir()?;
    let original = root.path().join("original");
    fs::create_dir(&original)?;
    fs::write(
        original.join("solution.sh"),
        r#"set -eu
count=0
if [ -f state.txt ]; then read -r count < state.txt; fi
count=$((count + 1))
input=$(cat input.txt)
printf '%s\n' "$count" > state.txt
printf '{"count":%s,"input":"%s"}\n' "$count" "$input" > result.json
"#,
    )?;
    fs::write(original.join("state.txt"), "99\n")?;
    let evidence = root.path().join("evidence");
    let validation = validator
        .validate(
            &task,
            "shell_tool",
            &original,
            &root.path().join("work"),
            &evidence,
            Some(Instant::now() + Duration::from_secs(180)),
        )
        .await?;
    assert_eq!(validation["status"], json!("captured"));
    assert_eq!(
        validation["phase_counts"],
        json!({"compiler_starts":0,"program_starts":3})
    );
    let phases = validation["cases"][0]["phases"]
        .as_array()
        .context("phase evidence")?;
    let state_bytes = phases
        .iter()
        .map(|facts| facts["snapshot"]["files"]["state.txt"].clone())
        .collect::<Vec<_>>();
    assert_eq!(state_bytes, vec![json!("1\n"), json!("2\n"), json!("3\n")]);
    let hash = crate::evidence::digest(b"unchanged\0\r\n");
    for (index, facts) in phases.iter().enumerate() {
        assert_eq!(
            facts["input_preservation"],
            json!({
                "success":true,"before":{"keep.bin":hash},"after":{"keep.bin":hash}
            })
        );
        assert_eq!(facts["snapshot"]["worker_events"], json!([]));
        let phase_directory = if index == 0 {
            evidence.join("case-0")
        } else {
            evidence.join(format!("case-0/phase-{index}"))
        };
        assert_eq!(
            crate::evidence::read_json(&phase_directory.join("phase-facts.json"))?,
            *facts
        );
        assert!(phase_directory.join("execution/process.json").is_file());
    }
    let mut analysis = crate::analysis::Analysis::start(&bundle).await?;
    let result = analysis
        .query(json!({"op":"attempt","task":task,"facts":{
            "trace":null,"exchanges":[],"snapshot":phases[0]["snapshot"],
            "outcome":{"termination":"exited","exit_code":0},"submission":validation
        }}))
        .await?;
    assert_eq!(result["oracle"]["success"], json!(true));
    // Later phases must execute the same delivered source, including Shell.
    fs::write(
        original.join("solution.sh"),
        "printf 'exit 0\\n' > solution.sh\n",
    )?;
    let changed = validator
        .validate(
            &task,
            "shell_tool",
            &original,
            &root.path().join("changed-work"),
            &root.path().join("changed-evidence"),
            Some(Instant::now() + Duration::from_secs(180)),
        )
        .await?;
    assert_eq!(changed["status"], json!("source_changed"));
    assert_eq!(
        changed["phase_counts"],
        json!({"compiler_starts":0,"program_starts":1})
    );
    assert_eq!(
        changed["cases"][0]["phases"][0]["source_integrity"]["unchanged"],
        json!(false)
    );
    Ok(())
}

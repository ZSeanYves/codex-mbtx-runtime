use super::*;
use pretty_assertions::assert_eq;

#[test]
fn report_verification_survives_removed_host_compiler_but_rejects_changed_bundle() -> Result<()> {
    let root = tempfile::tempdir()?;
    let bundle = root.path().join("bundle");
    let toolchain = root.path().join("toolchain");
    fs::create_dir(&bundle)?;
    fs::create_dir_all(toolchain.join("bin"))?;
    fs::create_dir(toolchain.join("lib"))?;
    fs::write(toolchain.join("bin/moon"), b"recorded compiler")?;
    fs::write(toolchain.join("lib/core"), b"recorded core")?;
    fs::copy(std::env::current_exe()?, bundle.join("mbtx-eval"))?;
    fs::write(bundle.join("evaluation-model.wasm"), b"frozen analysis")?;
    let info = json!({
        "schema_version":1,"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,
        "files":hashes(&bundle)?,"moon_home":toolchain,
        "moon_path":toolchain.join("bin/moon"),"moonrun_path":toolchain.join("bin/moon"),
        "moon_sha256":digest(b"recorded compiler"),"moonrun_sha256":digest(b"recorded compiler"),
        "moon_bin_hashes":toolchain_hashes(&toolchain.join("bin"))?,
        "moon_lib_hashes":toolchain_hashes(&toolchain.join("lib"))?
    });
    json_new(&bundle.join("bundle.json"), &info)?;
    assert_eq!(verify(&bundle)?, info);
    fs::remove_dir_all(&toolchain)?;
    assert!(verify(&bundle).is_err());
    assert_eq!(verify_analysis(&bundle)?, info);
    fs::write(bundle.join("evaluation-model.wasm"), b"changed analysis")?;
    assert!(verify_analysis(&bundle).is_err());
    Ok(())
}

#[test]
fn catalog_variants_change_only_target_tool_mode() -> Result<()> {
    let source = json!({
        "models": [
            {"slug":"gpt-5.6-terra","tool_mode":"code_mode_only","description":"target"},
            {"slug":"gpt-5.5","tool_mode":null,"description":"control"}
        ],
        "metadata": {"frozen":true}
    });
    let (native, direct) = catalog_variants(&source)?;
    assert_eq!(native, source);
    assert_eq!(direct["models"][0]["tool_mode"], "direct");
    assert_eq!(direct["models"][0]["description"], "target");
    assert_eq!(direct["models"][1], source["models"][1]);
    assert_eq!(direct["metadata"], source["metadata"]);
    Ok(())
}

#[test]
fn condition_manifest_covers_matrix_and_separates_nested_tools() {
    let catalogs = json!({
        "native_code_mode": {"file":"models-code-mode.json","sha256":"native"},
        "harness_direct": {"file":"models-direct.json","sha256":"direct"}
    });
    let manifest = condition_manifest(&catalogs, "policy");
    let conditions = manifest["conditions"].as_array().expect("conditions");
    assert_eq!(conditions.len(), 8);
    let code_mbtx = conditions
        .iter()
        .find(|condition| condition["condition_id"] == "code-mode-mbtx-production")
        .expect("code mode MBTX condition");
    assert_eq!(code_mbtx["requested_tool_mode"], "code_mode_only");
    assert_eq!(code_mbtx["model_visible_tools"], json!(["exec", "wait"]));
    assert_eq!(code_mbtx["expected_nested_tools"], json!(["mbtx"]));
    assert_eq!(code_mbtx["catalog_file"], "models-code-mode.json");
    assert_eq!(code_mbtx["policy_sha256"], "policy");
    let direct_mbtx = conditions
        .iter()
        .find(|condition| condition["condition_id"] == "direct-mbtx-production")
        .expect("direct MBTX condition");
    assert_eq!(direct_mbtx["requested_tool_mode"], "direct");
    assert_eq!(direct_mbtx["model_visible_tools"], json!(["mbtx"]));
    assert_eq!(direct_mbtx["expected_nested_tools"], json!([]));
    assert_eq!(direct_mbtx["catalog_file"], "models-direct.json");
}

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

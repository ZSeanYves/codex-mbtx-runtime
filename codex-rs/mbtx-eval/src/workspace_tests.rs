use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn nested_arms_have_identical_clean_baselines_and_no_parent_metadata() -> Result<()> {
    let root = tempfile::tempdir()?;
    let home = root.path().join("home");
    fs::create_dir(&home)?;
    fs::write(
        home.join(".gitconfig"),
        "[alias]\nstatus = !echo inherited\n[commit]\ngpgsign = true\n",
    )?;
    let path = std::env::var("PATH")?;
    prepare(root.path(), &home, &path).await?;
    fs::write(root.path().join("outer-private.txt"), "not a task input")?;
    let mut commits = Vec::new();
    for arm in ["shell", "mbtx"] {
        let workspace = root.path().join(arm);
        fs::create_dir(&workspace)?;
        fs::write(workspace.join("input.json"), "[1,2,3]\n")?;
        fs::write(workspace.join("fixture-worker"), "ignored binary")?;
        let facts = prepare(&workspace, &home, &path).await?;
        commits.push(facts["baseline_commit"].clone());
        let mut command = Command::new("git");
        command
            .args(["status", "--porcelain"])
            .current_dir(&workspace)
            .env_clear()
            .env("PATH", &path)
            .env("HOME", &home);
        git_environment(&mut command, &workspace);
        let result = command.output().await?;
        assert!(result.status.success());
        assert_eq!(String::from_utf8(result.stdout)?, "");
    }
    assert_eq!(commits[0], commits[1]);
    Ok(())
}

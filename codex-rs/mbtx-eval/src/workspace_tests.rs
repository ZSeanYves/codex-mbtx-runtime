use super::*;
use pretty_assertions::assert_eq;

#[test]
#[cfg(unix)]
fn sandbox_reentry_exposes_only_the_bundle_executable() -> Result<()> {
    let root = tempfile::tempdir()?;
    let root = root.path().canonicalize()?;
    let bundle = root.join("bundle with spaces");
    let work = root.join("attempt");
    let moon_home = root.join("moon");
    fs::create_dir(&bundle)?;
    fs::write(bundle.join("codex"), "sandbox helper")?;
    fs::write(bundle.join("models.json"), "private catalog")?;
    fs::write(root.join("run.json"), "private manifest")?;
    for name in ["workspace", "home", "tmp"] {
        fs::create_dir_all(work.join(name))?;
    }
    let socket_root = tempfile::Builder::new().prefix("mw-").tempdir_in("/tmp")?;
    let socket = socket_root.path().join("s");
    let _listener = std::os::unix::net::UnixListener::bind(&socket)?;
    fs::write(
        work.join("worker-socket.json"),
        serde_json::to_vec(&socket)?,
    )?;
    let profile = permissions(&work, &bundle, &moon_home)?;
    let encoded = toml::to_string(&profile)?;
    let decoded: toml::Value = toml::from_str(&encoded)?;
    let filesystem = decoded["evaluation"]["filesystem"].as_table().unwrap();
    let mut expected = toml::Table::from_iter([
        (":minimal".into(), "read".into()),
        (
            socket_root
                .path()
                .canonicalize()?
                .to_string_lossy()
                .into_owned(),
            "write".into(),
        ),
        (
            bundle.join("codex").to_string_lossy().into_owned(),
            "read".into(),
        ),
    ]);
    for name in ["workspace", "home", "tmp"] {
        expected.insert(
            work.join(name).to_string_lossy().into_owned(),
            "write".into(),
        );
    }
    for path in [Path::new("/opt/homebrew"), Path::new("/opt/local")] {
        if path.exists() {
            expected.insert(
                path.canonicalize()?.to_string_lossy().into_owned(),
                "read".into(),
            );
        }
    }
    // A broad grant would expose the model catalog, manifest and sibling arms.
    assert_eq!(filesystem, &expected);
    for (path, access) in filesystem {
        if access.as_str() == Some("write") {
            assert!(
                Path::new(path).is_dir(),
                "writable root must support metadata children: {path}"
            );
        }
    }
    Ok(())
}

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

//! Frozen, task-declared admission for direct MoonRun child requests.
//! Native descendants are still governed by Codex's OS sandbox, not this policy.
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde_json::Value;
use serde_json::json;

pub(crate) struct RuntimePolicy {
    pub path: PathBuf,
    pub execution_path: String,
    pub sha256: String,
}

pub(crate) fn rules(task: &Value) -> Result<Option<&Vec<Value>>> {
    let Some(value) = task.get("process_allow") else {
        return Ok(None);
    };
    let rules = value.as_array().context("process_allow must be an array")?;
    for rule in rules {
        let program = rule["program"].as_str().context("policy program")?;
        let args = rule["args_prefix"]
            .as_array()
            .context("policy argument prefix")?
            .iter()
            .map(|arg| arg.as_str().context("policy argument must be text"))
            .collect::<Result<Vec<_>>>()?;
        let allowed = match program {
            "jq" => args.is_empty(),
            "rg" => matches!(
                args.as_slice(),
                ["--no-config", "--json", "--"] | ["--no-config", "--files", "--"]
            ),
            "fixture-worker" => matches!(
                args.as_slice(),
                ["square" | "checked-square" | "job" | "recover" | "emit" | "hash" | "echo"]
            ),
            // No current pilot task needs Git. Its reference remains available,
            // but adding another command requires a deliberate protocol revision.
            _ => false,
        };
        ensure!(allowed, "unsupported task process rule: {rule}");
        ensure!(
            rule.as_object().is_some_and(|object| object.len() == 2),
            "unexpected process rule fields"
        );
    }
    Ok(Some(rules))
}

pub(crate) fn prepare(
    task: &Value,
    work: &Path,
    evidence: &Path,
    bundle: &Path,
    utilities: &Value,
) -> Result<Option<RuntimePolicy>> {
    let Some(rules) = rules(task)? else {
        return Ok(None);
    };
    let tools = work.join("tools");
    fs::create_dir(&tools)?;
    let mut executables = BTreeMap::new();
    for rule in rules {
        let name = rule["program"].as_str().context("program")?;
        if executables.contains_key(name) {
            continue;
        }
        let source = if name == "fixture-worker" {
            bundle.join("fixture-worker").canonicalize()?
        } else {
            Path::new(
                utilities[name]["path"]
                    .as_str()
                    .context("frozen utility path")?,
            )
            .canonicalize()?
        };
        let hash = crate::evidence::digest(&fs::read(&source)?);
        if name != "fixture-worker" {
            ensure!(
                utilities[name]["sha256"] == hash,
                "utility {name} changed since manifest freeze"
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                fs::metadata(&source)?.permissions().mode() & 0o111 != 0,
                "utility is not executable"
            );
            std::os::unix::fs::symlink(&source, tools.join(name))?;
        }
        #[cfg(not(unix))]
        anyhow::bail!("evaluation process policy currently requires Unix");
        executables.insert(name.to_owned(), json!({"path":source,"sha256":hash}));
    }
    // The existing Codex Linux launcher also resolves bwrap using the child
    // environment. It must remain discoverable without admitting a MoonRun
    // request for it. Keeping it on this PATH uses Codex's system-bwrap
    // capability probing, including support for older distro versions.
    #[cfg(target_os = "linux")]
    {
        let launcher = bundle.join("codex-resources/bwrap").canonicalize()?;
        std::os::unix::fs::symlink(&launcher, tools.join("bwrap"))?;
        executables.insert("bwrap".to_owned(), json!({"path":launcher,"sha256":crate::evidence::digest(&fs::read(&launcher)?),"scope":"Codex sandbox launcher only; not in process.allow"}));
    }
    let execution_path = tools.canonicalize()?.to_string_lossy().into_owned();
    let socket: String = serde_json::from_slice(&fs::read(work.join("worker-socket.json"))?)?;
    let workspace = work.join("workspace").canonicalize()?;
    let temporary = work.join("tmp").canonicalize()?;
    let mut readable = vec![workspace.clone(), temporary.clone(), tools.canonicalize()?];
    if bundle.join("dependencies").is_dir() {
        readable.push(bundle.join("dependencies").canonicalize()?);
    }
    let policy = json!({
        "process":{"allow":rules},
        "net":{},
        "fs":{"read":readable,"write":[workspace,temporary]},
        "env":{"set":{"PATH":execution_path,"LANG":"C.UTF-8","TMPDIR":temporary,"MBTX_WORKER_SOCKET":socket}}
    });
    let path = work.join("runtime-policy.json");
    crate::evidence::json_new(&path, &policy)?;
    let sha256 = crate::evidence::digest(&fs::read(&path)?);
    crate::evidence::json_new(&evidence.join("policy.json"), &policy)?;
    crate::evidence::json_new(
        &evidence.join("policy-evidence.json"),
        &json!({
            "sha256":sha256,"template_sha256":crate::evidence::digest(&serde_json::to_vec(rules)?),
            "executables":executables,"execution_path":execution_path,
            "scope":"direct MoonRun requests; no complete native-descendant or allowed-child audit",
            "process_spawns":null,"process_spawns_confidence":"unknown"
        }),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tools, fs::Permissions::from_mode(0o555))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444))?;
    }
    Ok(Some(RuntimePolicy {
        path,
        execution_path,
        sha256,
    }))
}

#[cfg(test)]
#[path = "process_policy_tests.rs"]
mod tests;

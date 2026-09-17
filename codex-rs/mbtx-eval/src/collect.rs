use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use clap::Args;
use serde_json::Value;
use serde_json::json;

use crate::analysis::Analysis;
use crate::attempt::Execution;
use crate::bundle;
use crate::config::RelayConfig;
use crate::evidence::json_new;
use crate::evidence::now_ms;
use crate::evidence::read_json;
use crate::evidence::safe_relative;
use crate::evidence::seal;
use crate::evidence::write_new;
use crate::gate::Gate;
use crate::gate::Route;
use crate::replay;
use crate::report;
use crate::scheduling::schedule;

#[derive(Args)]
pub(crate) struct RunArgs {
    #[arg(long)]
    pub bundle: PathBuf,
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long, value_parser=["replay","relay"],default_value="replay")]
    pub mode: String,
    #[arg(long,value_parser=["pilot","programs","workflow","long-study","long-pilot"],default_value="pilot")]
    pub suite: String,
    #[arg(long, default_value = "mbtx/config/relay.toml")]
    pub config: PathBuf,
    #[arg(long)]
    pub credentials_file: Option<PathBuf>,
    #[arg(long)]
    pub repeats: Option<usize>,
    /// Stop between pairs after this many newly attempted pairs in this invocation.
    #[arg(long)]
    pub batch_pairs: Option<usize>,
    /// Entire Codex attempt, including external requests and pacing; never a step cutoff.
    #[arg(long, default_value_t = 3600)]
    pub max_wall_seconds: u64,
    #[arg(long, default_value_t = 20260916)]
    pub seed: u64,
    #[arg(long, default_value_t = 15000)]
    pub min_interval_ms: u64,
    #[arg(long, value_delimiter = ',')]
    pub tasks: Vec<String>,
    #[arg(long, value_delimiter = ',')]
    pub scenarios: Vec<String>,
    #[arg(long, value_delimiter = ',')]
    pub variants: Vec<u64>,
    #[arg(long)]
    pub resume: bool,
    #[arg(long)]
    pub replay_source: Option<PathBuf>,
    /// Controlled offline failure; never combined with a real provider.
    #[arg(long,value_parser=["429","500","disconnect","truncated","stall","500-once","disconnect-once"],conflicts_with="replay_source")]
    pub replay_fault: Option<String>,
    /// Controlled decision prefix for offline step, resource and cache validation.
    #[arg(long, default_value_t = 0)]
    pub replay_prefix_turns: usize,
    /// Minimum retains audit evidence; full additionally captures native OTLP.
    #[arg(long,default_value="full",value_parser=["full","minimal"])]
    pub observation: String,
}

fn route(directory: PathBuf, arm: &str, replies: Option<Vec<replay::Reply>>) -> Result<Route> {
    for child in ["http", "otel", "trace"] {
        fs::create_dir(directory.join(child))?;
    }
    Ok(Route {
        directory,
        arm: arm.into(),
        token: uuid::Uuid::new_v4().to_string(),
        replies,
        active: AtomicBool::new(true),
        requests: AtomicUsize::new(0),
        otel_requests: AtomicUsize::new(0),
        observation_failures: AtomicUsize::new(0),
        closed: tokio::sync::Notify::new(),
        otel_write: std::sync::Mutex::new(()),
    })
}

pub(crate) async fn run(args: RunArgs) -> Result<PathBuf> {
    ensure!(
        cfg!(unix),
        "pilot collection currently supports Linux and macOS only"
    );
    ensure!(args.batch_pairs != Some(0), "batch-pairs must be positive");
    ensure!(
        args.mode == "replay" || args.observation == "full",
        "minimal observation is an offline calibration condition only"
    );
    ensure!(
        args.replay_prefix_turns <= 256
            && (args.replay_prefix_turns == 0
                || (args.mode == "replay"
                    && args.replay_source.is_none()
                    && args.replay_fault.is_none())),
        "decision prefix is available only for fixed offline replay"
    );
    ensure!(
        (1..=86400).contains(&args.max_wall_seconds),
        "max-wall-seconds must be 1..86400"
    );
    ensure!(
        args.mode != "relay" || args.min_interval_ms >= 15000,
        "relay start interval must be at least 15000 ms"
    );
    ensure!(
        args.mode == "replay" || (args.replay_fault.is_none() && args.replay_source.is_none()),
        "recorded responses and fault injection require replay mode"
    );
    let bundle = args.bundle.canonicalize()?;
    let bundle_info = bundle::verify(&bundle)?;
    let path = std::env::var("PATH").context("PATH is required")?;
    let mut utilities = serde_json::Map::new();
    for name in ["rg", "jq", "sh", "git"] {
        let executable = which::which(name).with_context(|| format!("required utility {name} is missing; install ripgrep, jq and git before collection"))?.canonicalize()?;
        utilities.insert(
            name.into(),
            json!({"path":executable,"sha256":crate::evidence::digest(&fs::read(&executable)?)}),
        );
    }
    let config = RelayConfig::load(&args.config)?;
    let provider = config.provider()?;
    let retry_policy = json!({
        "request_max_retries":provider.request_max_retries,
        "stream_max_retries":provider.stream_max_retries,
        "unbounded_connection_retries":false,
        "gateway_internal_retries":0,
        "retry_429":false,
        "max_http_sends_per_sampling_call":(provider.request_max_retries+1)*(provider.stream_max_retries+1),
        "scope":"Retries after the initial try, per HTTP request and per logical sampling call; normal task decisions are not capped"
    });
    let key = if args.mode == "relay" {
        config.credential(args.credentials_file.as_deref())?
    } else {
        String::new()
    };
    // Refuse a second evaluator on this host using the same account. Other
    // applications remain outside our control. The lock contains no credential.
    let account_lock = if args.mode == "relay" {
        let cache = std::env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME required for relay account lock")?
            .join(".cache/mbtx-eval");
        fs::create_dir_all(&cache)?;
        let name =
            crate::evidence::digest(format!("{}:{key}", config.provider()?.base_url).as_bytes());
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(cache.join(format!("{name}.lock")))?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            ensure!(
                unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
                "another evaluator on this host is using this relay account"
            );
        }
        Some(file)
    } else {
        None
    };
    let mut analysis = Analysis::start(&bundle).await?;
    let protocol = analysis
        .query(json!({"op":"protocol","suite":args.suite}))
        .await?;
    let repeats = args.repeats.unwrap_or(
        protocol["repeats"]
            .as_u64()
            .context("protocol repetitions")? as usize,
    );
    ensure!((1..=20).contains(&repeats), "repeats must be 1..20");
    if let Some(source) = &args.replay_source {
        ensure!(
            crate::evidence::verify_manifest(source)?,
            "recorded source manifest or fixture changed"
        );
        ensure!(
            read_json(&source.join("run.json"))?["protocol"] == protocol,
            "recorded replay requires the original task and observation protocol"
        );
    }
    let mut tasks = protocol["tasks"]
        .as_array()
        .context("protocol tasks")?
        .clone();
    if !args.tasks.is_empty() {
        ensure!(
            args.tasks
                .iter()
                .all(|id| tasks.iter().any(|t| t["id"] == *id)),
            "unknown task selection"
        );
        tasks.retain(|t| args.tasks.iter().any(|id| t["id"] == *id));
    }
    if !args.scenarios.is_empty() {
        ensure!(
            args.scenarios
                .iter()
                .all(|id| tasks.iter().any(|t| t["scenario"] == *id)),
            "unknown scenario selection"
        );
        tasks.retain(|t| args.scenarios.iter().any(|id| t["scenario"] == *id));
    }
    if !args.variants.is_empty() {
        ensure!(
            args.variants.iter().all(|v| (1..=4).contains(v)),
            "variants use one-based indices 1..4"
        );
        tasks.retain(|t| {
            t["variant"]
                .as_u64()
                .is_some_and(|v| args.variants.contains(&(v + 1)))
        });
    }
    ensure!(!tasks.is_empty(), "task selection is empty");
    let expected_schedule = schedule(&tasks, repeats, args.seed);
    let manifest = if args.resume {
        let previous = read_json(&args.output.join("run.json"))?;
        ensure!(
            crate::evidence::verify_manifest(&args.output)?,
            "run manifest or fixture manifest changed"
        );
        ensure!(
            previous["bundle"] == bundle_info
                && previous["config"] == serde_json::to_value(&config)?
                && previous["mode"] == args.mode
                && previous["min_interval_ms"] == args.min_interval_ms
                && previous["max_wall_seconds"] == args.max_wall_seconds
                && previous["replay_prefix_turns"].as_u64().unwrap_or(0)
                    == args.replay_prefix_turns as u64
                && previous["observation"].as_str().unwrap_or("full") == args.observation
                && previous["retry_policy"] == retry_policy
                && previous["schedule"] == json!(expected_schedule)
                && previous["protocol"] == protocol
                && previous["path"] == path
                && previous["utilities"] == json!(utilities)
                && previous["replay_fault"] == json!(args.replay_fault)
                && previous["replay_source"] == json!(args.replay_source),
            "resume requires the original bundle, protocol, configuration and schedule"
        );
        previous
    } else {
        fs::create_dir_all(args.output.parent().unwrap_or(Path::new(".")))?;
        fs::create_dir(&args.output).context(
            "run directory already exists; use --resume for unfinished schedule entries",
        )?;
        for name in ["attempts", "workspaces", "probes", "reports", "fixtures"] {
            fs::create_dir(args.output.join(name))?;
        }
        for task in &tasks {
            let directory = args
                .output
                .join("fixtures")
                .join(task["id"].as_str().context("task id")?);
            fs::create_dir(&directory)?;
            for (path, content) in task["files"].as_object().context("fixture files")? {
                let file = directory.join(safe_relative(path)?);
                fs::create_dir_all(file.parent().context("fixture parent")?)?;
                write_new(&file, content.as_str().context("fixture text")?.as_bytes())?;
            }
            let baseline = crate::workspace::prepare(&directory, &directory, &path).await?;
            json_new(&directory.join("baseline.json"), &baseline)?;
            seal(&directory)?;
        }
        let fixture_seals = crate::evidence::hashes(&args.output.join("fixtures"))?;
        let manifest = json!({"schema_version":1,"run_id":uuid::Uuid::new_v4().to_string(),"created_ms":now_ms(),"mode":args.mode,"platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"bundle":bundle_info,"protocol":protocol,"schedule":expected_schedule,"config":config,"min_interval_ms":args.min_interval_ms,"seed":args.seed,"path":std::env::var("PATH").unwrap_or_default(),"replay_source":args.replay_source,"replay_fault":args.replay_fault,"fixture_hashes":fixture_seals});
        let mut manifest = manifest;
        manifest["path"] = json!(path);
        manifest["utilities"] = json!(utilities);
        manifest["max_wall_seconds"] = json!(args.max_wall_seconds);
        manifest["replay_prefix_turns"] = json!(args.replay_prefix_turns);
        manifest["observation"] = json!(args.observation);
        manifest["retry_policy"] = retry_policy;
        json_new(&args.output.join("run.json"), &manifest)?;
        write_new(
            &args.output.join("run.sha256"),
            crate::evidence::digest(&fs::read(args.output.join("run.json"))?).as_bytes(),
        )?;
        manifest
    };
    let root = args.output.canonicalize()?;
    // Exclusive run lock is an open-file flock, so SIGKILL does not leave a stale
    // owner. The separate account lock also coordinates relay runs on this host.
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("collector.lock"))?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        ensure!(
            unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
            "another collector owns this run"
        );
    }
    let gate = Gate::start(
        config.provider()?.base_url.clone(),
        key,
        Duration::from_millis(args.min_interval_ms),
    )
    .await?;
    let execution = Execution {
        root: &root,
        bundle: &bundle,
        bundle_info: &bundle_info,
        config: &config,
        manifest: &manifest,
        gate: &gate,
    };
    let result = collect_schedule(&execution, &args, &mut analysis).await;
    gate.stop().await;
    json_new(
        &root.join(format!("collection-session-{}.json", uuid::Uuid::new_v4())),
        &json!({"ended_ms":now_ms(),"stop_reason":result.as_ref().copied().unwrap_or("collector_error"),"error":result.as_ref().err().map(ToString::to_string)}),
    )?;
    // Report whatever survived, including an incomplete attempt after an error.
    let generated = report::generate(&root, &bundle, &mut analysis, "all").await;
    result?;
    generated?;
    drop(account_lock);
    Ok(root)
}

async fn collect_schedule(
    execution: &Execution<'_>,
    args: &RunArgs,
    analysis: &mut Analysis,
) -> Result<&'static str> {
    let root = execution.root;
    let config = execution.config;
    let manifest = execution.manifest;
    let gate = execution.gate;
    crate::preflight::check(
        root,
        &crate::submission::Validator {
            bundle: execution.bundle,
            bundle_info: execution.bundle_info,
            path: manifest["path"].as_str().context("recorded PATH")?,
        },
    )
    .await?;
    if args.mode == "relay" && !args.resume && !probe(root, config, gate).await? {
        return Ok("relay_probe_failed");
    }
    let mut completed = report::assess(root, analysis).await?;
    let pairs = manifest["schedule"].as_array().context("schedule")?;
    let mut started_pairs = 0;
    for pair in pairs {
        let pending = pair["arms"].as_array().context("arms")?.iter().any(|arm| {
            !completed
                .iter()
                .any(|a| a["pair_id"] == pair["pair_id"] && a["arm"] == *arm)
        });
        if !pending {
            continue;
        }
        if args.batch_pairs.is_some_and(|limit| started_pairs >= limit) {
            return Ok("batch_limit");
        }
        started_pairs += 1;
        let task = manifest["protocol"]["tasks"]
            .as_array()
            .context("tasks")?
            .iter()
            .find(|t| t["id"] == pair["task_id"])
            .context("scheduled task")?;
        for arm in pair["arms"].as_array().context("arms")? {
            if completed
                .iter()
                .any(|a| a["pair_id"] == pair["pair_id"] && a["arm"] == *arm)
            {
                continue;
            }
            if analysis
                .query(json!({"op":"gate","attempts":completed}))
                .await?["pause"]
                == true
            {
                return Ok("infrastructure_failure_gate");
            }
            let arm = arm.as_str().context("arm label")?;
            let id = uuid::Uuid::new_v4().to_string();
            let directory = root.join("attempts").join(&id);
            fs::create_dir(&directory)?;
            json_new(
                &directory.join("assignment.json"),
                &json!({"attempt_id":id,"pair_id":pair["pair_id"],"task_id":task["id"],"arm":arm,"assigned_ms":now_ms()}),
            )?;
            eprintln!(
                "[eval] {}/{} arms; {} {arm} {}",
                completed.len(),
                pairs.len() * 2,
                pair["pair_id"],
                task["id"]
            );
            let replies = if let Some(source) = &args.replay_source {
                let source_attempt = report::find_attempt(source, &pair["pair_id"], arm)?;
                ensure!(
                    crate::evidence::verify(&source_attempt)?,
                    "replay source attempt is unsealed or changed"
                );
                Some(replay::recorded(&source_attempt)?)
            } else if let Some(fault) = &args.replay_fault {
                let provider = config.provider()?;
                let replies = if let Some(transient) = fault.strip_suffix("-once") {
                    let mut replies = vec![replay::fault(transient)];
                    replies.extend(replay::fixed(task, arm, 0)?);
                    replies
                } else {
                    // Enough evidence for both independently bounded native
                    // layers; exhausting a fixture is never a relay error.
                    let sends =
                        (provider.request_max_retries + 1) * (provider.stream_max_retries + 1);
                    vec![replay::fault(fault); sends as usize]
                };
                Some(replies)
            } else if args.mode == "replay" {
                Some(replay::fixed(task, arm, args.replay_prefix_turns)?)
            } else {
                None
            };
            let route = gate
                .add(id.clone(), route(directory.clone(), arm, replies)?)
                .await;
            if let Err(error) = execution.execute(task, &pair["pair_id"], &id, &route).await {
                if !directory.join("seal.json").exists() {
                    json_new(
                        &directory.join("execution-error.json"),
                        &json!({"error":error.to_string(),"wall_time_ms":now_ms()}),
                    )?;
                }
                return Err(error);
            }
            let assessment = report::assess_attempt(&directory, manifest, analysis).await?;
            eprintln!(
                "[eval] {} status={} steps={}",
                id, assessment["status"], assessment["accounting"]["metrics"]["agent_steps"]
            );
            {
                use std::io::Write;
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(root.join("progress.jsonl"))?;
                serde_json::to_writer(
                    &mut file,
                    &json!({"attempt_id":id,"pair_id":pair["pair_id"],"arm":arm,"status":assessment["status"],"comparable":assessment["status"]=="success" && assessment["integrity"]==true && assessment["accounting"]["coverage"]["steps"]==true}),
                )?;
                file.write_all(b"\n")?;
                file.sync_all()?;
            }
            completed.push(assessment);
        }
    }
    Ok("schedule_finished")
}

async fn probe(root: &Path, config: &RelayConfig, gate: &Arc<Gate>) -> Result<bool> {
    let client = reqwest::Client::builder()
        .retry(reqwest::retry::never())
        .no_proxy()
        .timeout(Duration::from_secs(120))
        .build()?;
    for index in 1..=3 {
        let id = format!("probe-{}", uuid::Uuid::new_v4());
        let directory = root.join("probes").join(&id);
        fs::create_dir(&directory)?;
        let route = gate
            .add(id.clone(), route(directory.clone(), "probe", None)?)
            .await;
        let response = client
            .post(format!("{}/a/{id}/v1/responses", gate.endpoint))
            .bearer_auth(&route.token)
            .json(&json!({
                "model":config.model,
                "instructions":"Reply with OK.",
                "reasoning":{"effort":config.model_reasoning_effort},
                "stream":true,"store":false,
                "input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"Reply with OK."}]}]
            }))
            .send().await;
        let (status_code, success, detail) = match response {
            Ok(response) => {
                let status = response.status();
                match response.text().await {
                    Ok(body) => {
                        let success = status.is_success()
                            && report::response_summary(&body).0 == Some("completed");
                        let detail = if success {
                            None
                        } else {
                            let value: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                            Some(value["error"]["message"].as_str()
                                .unwrap_or("response did not contain a successful terminal Responses event")
                                .chars().take(400).collect::<String>())
                        };
                        (Some(status.as_u16()), success, detail)
                    }
                    Err(error) => (Some(status.as_u16()), false, Some(error.to_string())),
                }
            }
            Err(error) => (None, false, Some(error.to_string())),
        };
        route.close();
        tokio::time::timeout(Duration::from_secs(15), gate.drain())
            .await
            .context("probe stream failed to close")?;
        json_new(
            &directory.join("summary.json"),
            &json!({
                "success":success,"status_code":status_code,"detail":detail,
                "response_evidence":"http/request-0000/response.body"
            }),
        )?;
        seal(&directory)?;
        eprintln!(
            "[eval] relay probe {index}/3 success={success} status={} detail={} evidence={}",
            json!(status_code),
            json!(detail),
            directory.display()
        );
        if success {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
#[path = "collect_tests.rs"]
mod tests;

//! OS and transport adapter for programmable-MBTX experiments. Evaluation rules
//! live in the prebuilt MoonBit worker, never in the collection client.
mod analysis;
mod attempt;
mod bundle;
mod collect;
mod config;
mod evidence;
mod gate;
mod http_headers;
mod observe;
mod replay;
mod report;
mod request_contract;
mod workspace;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use serde_json::json;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    about = "Auditable programmable-MBTX pilot and offline replay",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Operation,
}

#[derive(Subcommand)]
enum Operation {
    /// Assemble already built components; use the thin preparation entry.
    Bundle {
        #[arg(long)]
        repo: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        fingerprint: String,
    },
    VerifyBundle {
        bundle: PathBuf,
    },
    /// Run pilot goals with a real relay, or prescribed local Responses.
    Run(collect::RunArgs),
    /// Execute recorded HTTP responses into new attempts, never overwrite input.
    Replay {
        run: PathBuf,
        #[arg(long)]
        bundle: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    Report {
        run: PathBuf,
        #[arg(long)]
        bundle: Option<PathBuf>,
        #[arg(long,default_value="all",value_parser=["all","json","md","html","csv"])]
        format: String,
    },
    Log {
        run: PathBuf,
        #[arg(long)]
        bundle: Option<PathBuf>,
        #[arg(long)]
        follow: bool,
    },
    /// Send archived native OTel and step spans to a local SigNoz collector.
    ImportOtel {
        run: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:4318")]
        endpoint: String,
    },
    /// Small deterministic subprocess fixture shared by both arms.
    Square {
        value: i32,
    },
}

fn default_bundle(value: Option<PathBuf>) -> Result<PathBuf> {
    value.map(Ok).unwrap_or_else(|| {
        Ok(std::env::current_exe()?
            .parent()
            .context("executable directory")?
            .to_owned())
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Operation::Bundle {
            repo,
            output,
            fingerprint,
        } => {
            bundle::assemble(&repo, &output, &fingerprint)?;
            println!("{}", output.display());
        }
        Operation::VerifyBundle { bundle: path } => {
            bundle::verify(&path)?;
            println!("Verified {}", path.display());
        }
        Operation::Run(args) => {
            let path = collect::run(args).await?;
            println!("{}", path.display());
        }
        Operation::Replay {
            run,
            bundle,
            output,
        } => {
            let manifest = evidence::read_json(&run.join("run.json"))?;
            let pairs = manifest["schedule"].as_array().context("source schedule")?;
            let repeats = pairs
                .iter()
                .filter_map(|p| p["repeat"].as_u64())
                .max()
                .unwrap_or(0) as usize
                + 1;
            let mut tasks: Vec<String> = pairs
                .iter()
                .filter_map(|p| p["task_id"].as_str().map(str::to_owned))
                .collect();
            tasks.sort();
            tasks.dedup();
            let config_path =
                output.with_extension(format!("config-{}.toml", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(config_path.parent().context("output parent")?)?;
            let configuration: config::RelayConfig =
                serde_json::from_value(manifest["config"].clone())?;
            evidence::write_new(
                &config_path,
                toml::to_string_pretty(&configuration)?.as_bytes(),
            )?;
            anyhow::ensure!(
                evidence::verify_manifest(&run)?,
                "source manifest or fixtures changed"
            );
            let path = collect::run(collect::RunArgs {
                bundle,
                output,
                mode: "replay".into(),
                config: config_path,
                credentials_file: None,
                repeats,
                seed: manifest["seed"].as_u64().context("seed")?,
                min_interval_ms: 15000,
                tasks,
                resume: false,
                replay_source: Some(run.canonicalize()?),
                replay_fault: None,
            })
            .await?;
            println!("{}", path.display());
        }
        Operation::Report {
            run,
            bundle,
            format,
        } => {
            let bundle = default_bundle(bundle)?;
            bundle::verify(&bundle)?;
            let mut worker = analysis::Analysis::start(&bundle).await?;
            println!(
                "{}",
                report::generate(&run, &bundle, &mut worker, &format)
                    .await?
                    .display()
            );
        }
        Operation::Log {
            run,
            bundle,
            follow,
        } => {
            let bundle = default_bundle(bundle)?;
            let mut worker = analysis::Analysis::start(&bundle).await?;
            loop {
                let manifest = evidence::read_json(&run.join("run.json"))?;
                let attempts = report::assess(&run, &mut worker).await?;
                let report = worker
                    .query(json!({"op":"report","manifest":manifest,"attempts":attempts}))
                    .await?;
                let current = report["attempts"]
                    .as_array()
                    .and_then(|a| a.last())
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let exchange = current["exchanges"]
                    .as_array()
                    .and_then(|a| a.last())
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let phase = if current["outcome"].is_object() {
                    "arm finalized"
                } else if exchange["headers"].is_object() {
                    "receiving response / tool work / shutdown; inspect native trace"
                } else if exchange["started"].is_object() {
                    "waiting for response headers"
                } else if exchange["evidence"].is_string() {
                    "request queue"
                } else {
                    "preparing Codex or probing"
                };
                println!(
                    "{}",
                    json!({"run_id":report["run_id"],"partial":report["partial"],"itt":report["itt"],"gate":report["gate"],"current_attempt":current["attempt_id"],"phase":phase,"latest_queue_wait_ns":exchange["started"]["queue_wait_ns"],"remaining_time_estimate":null})
                );
                if !follow || report["partial"] == false || report["gate"]["pause"] == true {
                    break;
                }
                #[cfg(unix)]
                {
                    use std::os::fd::AsRawFd;
                    let lock = std::fs::File::open(run.join("collector.lock"))?;
                    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) } == 0
                    {
                        break;
                    }
                }
                tokio::select! {_=tokio::time::sleep(std::time::Duration::from_secs(2))=>(),_=tokio::signal::ctrl_c()=>break}
            }
        }
        Operation::ImportOtel { run, endpoint } => println!(
            "Imported {} OTLP batches",
            observe::import(&run, &endpoint).await?
        ),
        Operation::Square { value } => println!(
            "{}",
            json!({"value":value,"square":i64::from(value)*i64::from(value)})
        ),
    }
    Ok(())
}

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
mod submission;
mod validation_process;
mod workspace;
mod scheduling;
mod worker_receipts;
mod report_details;
mod report_html;
mod progress;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
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
    Run(Box<collect::RunArgs>),
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
            let path = collect::run(*args).await?;
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
                suite: manifest["protocol"]["suite"]
                    .as_str()
                    .unwrap_or("pilot")
                    .to_owned(),
                config: config_path,
                credentials_file: None,
                repeats: Some(repeats),
                batch_pairs: None,
                max_wall_seconds: manifest["max_wall_seconds"].as_u64().context("recorded wall limit")?,
                seed: manifest["seed"].as_u64().context("seed")?,
                min_interval_ms: 15000,
                tasks,
                scenarios: vec![],
                variants: vec![],
                resume: false,
                replay_source: Some(run.canonicalize()?),
                replay_fault: None,
                replay_prefix_turns: 0,
                observation: manifest["observation"].as_str().unwrap_or("full").to_owned(),
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
            bundle::verify_analysis(&bundle)?;
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
            bundle: _,
            follow,
        } => {
            progress::log(&run,follow).await?;
        }
    }
    Ok(())
}

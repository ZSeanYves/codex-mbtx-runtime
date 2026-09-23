//! Generic task-visible workers. No private evaluation answers are linked here.
use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde_json::Value;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

pub(crate) fn receipt(phase: &str, args: &[String], exit_code: Option<u8>) -> Result<()> {
    let Some(socket) = std::env::var_os("MBTX_WORKER_SOCKET") else {
        return Ok(());
    };
    let mut connection = std::os::unix::net::UnixStream::connect(socket)?;
    connection.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
    let cwd = std::env::current_dir()?;
    serde_json::to_writer(
        &mut connection,
        &json!({"phase":phase,"action":args.first(),"id":args.get(2),"path":if args.first().is_some_and(|a| matches!(a.as_str(), "hash" | "job" | "recover" | "emit")){args.get(1)}else{None},"cwd":cwd,"args":args,"exit_code":exit_code}),
    )?;
    connection.write_all(b"\n")?;
    let mut response = String::new();
    BufReader::new(connection).read_line(&mut response)?;
    ensure!(response == "recorded\n", "worker receipt was not recorded");
    Ok(())
}

pub(crate) fn execute(args: &[String]) -> Result<Option<(ExitCode, u8)>> {
    if args.len() == 2 && args[1] == "--help" {
        match args[0].as_str() {
            "job" | "recover" => println!(
                "fixture-worker {} PLAN.json ID; job exit 7 is retryable",
                args[0]
            ),
            "hash" => println!("fixture-worker hash FILE"),
            "emit" => println!("fixture-worker emit STREAMS.json"),
            _ => return Ok(None),
        }
        return Ok(Some((ExitCode::SUCCESS, 0)));
    }
    match args.first().map(String::as_str) {
        Some("job" | "recover") if args.len() == 3 => {
            let input: Value = serde_json::from_slice(&fs::read(&args[1])?)?;
            let id = &args[2];
            ensure!(
                !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
                "invalid worker ID"
            );
            let key = if args[0] == "job" { "jobs" } else { "records" };
            let row = input[key]
                .as_array()
                .context("worker inputs")?
                .iter()
                .filter(|r| r["id"] == *id)
                .max_by_key(|r| r["generation"].as_i64())
                .context("unknown worker ID")?;
            let mut value = row["value"].as_i64().context("worker value")?;
            if args[0] == "job" {
                if row["enabled"] == false {
                    println!("{}", json!({"id":id,"status":"skipped"}));
                    return Ok(Some((ExitCode::SUCCESS, 0)));
                }
                for dependency in row["deps"].as_array().context("job dependencies")? {
                    let name = dependency.as_str().context("dependency ID")?;
                    ensure!(
                        name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
                        "invalid dependency ID"
                    );
                    let status: Value =
                        serde_json::from_slice(&fs::read(format!("state/{name}.json"))?)?;
                    value = value
                        .checked_add(status["result"].as_i64().context("dependency result")?)
                        .context("result overflow")?;
                }
            } else {
                value = value.checked_mul(value).context("result overflow")?;
            }
            // Only a validated, enabled operation reaches this event. Usage and
            // malformed requests still retain their ordinary process receipts.
            receipt("operation", args, None)?;
            fs::create_dir_all("state")?;
            let retry = format!("state/{id}.retry");
            if args[0] == "job" && row["fail_once"] == true && !Path::new(&retry).exists() {
                fs::write(retry, b"retryable\n")?;
                eprintln!("controlled retryable failure");
                return Ok(Some((ExitCode::from(7), 7)));
            }
            let result = json!({"id":id,"generation":row["generation"],"result":value});
            let path = format!("state/{id}.json");
            let temporary = format!("state/{id}.pending");
            fs::write(&temporary, serde_json::to_vec(&result)?)?;
            fs::rename(temporary, path)?;
            println!("{result}");
        }
        Some("emit") if args.len() == 2 => {
            let input: Value = serde_json::from_slice(&fs::read(&args[1])?)?;
            for stream in ["stdout", "stderr"] {
                let name = input[stream].as_str().context("stream input path")?;
                let bytes = fs::read(name)?;
                if stream == "stdout" {
                    std::io::stdout().write_all(&bytes)?;
                } else {
                    std::io::stderr().write_all(&bytes)?;
                }
            }
        }
        Some("hash") if args.len() == 2 => println!("{:x}", Sha256::digest(fs::read(&args[1])?)),
        _ => return Ok(None),
    }
    Ok(Some((ExitCode::SUCCESS, 0)))
}

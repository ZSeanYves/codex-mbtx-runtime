//! Generic task-visible workers. No private evaluation answers are linked here.
use std::{fs, io::{BufRead, BufReader, Write}, path::Path, process::ExitCode};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(crate) fn receipt(phase: &str, args: &[String], exit_code: Option<u8>) -> Result<()> {
    let Some(socket) = std::env::var_os("MBTX_WORKER_SOCKET") else { return Ok(()); };
    let mut connection = std::os::unix::net::UnixStream::connect(socket)?;
    connection.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
    serde_json::to_writer(&mut connection, &json!({"phase":phase,"action":args.first(),"id":args.get(2),"args":args,"exit_code":exit_code}))?;
    connection.write_all(b"\n")?;
    let mut response = String::new();
    BufReader::new(connection).read_line(&mut response)?;
    ensure!(response == "recorded\n", "worker receipt was not recorded");
    Ok(())
}

pub(crate) fn execute(args: &[String]) -> Result<Option<(ExitCode, u8)>> {
    match args.first().map(String::as_str) {
        Some("job" | "recover") if args.len() == 3 => {
            let input: Value = serde_json::from_slice(&fs::read(&args[1])?)?;
            let id = &args[2];
            ensure!(!id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'), "invalid worker ID");
            let key = if args[0] == "job" { "jobs" } else { "records" };
            let row = input[key].as_array().context("worker inputs")?.iter().filter(|r| r["id"] == *id).max_by_key(|r| r["generation"].as_i64()).context("unknown worker ID")?;
            fs::create_dir_all("state")?;
            let mut value = row["value"].as_i64().context("worker value")?;
            if args[0] == "job" {
                if row["enabled"] == false { println!("{}", json!({"id":id,"status":"skipped"})); return Ok(Some((ExitCode::SUCCESS, 0))); }
                for dependency in row["deps"].as_array().context("job dependencies")? {
                    let name = dependency.as_str().context("dependency ID")?;
                    ensure!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'), "invalid dependency ID");
                    let status: Value = serde_json::from_slice(&fs::read(format!("state/{name}.json"))?)?;
                    value = value.checked_add(status["result"].as_i64().context("dependency result")?).context("result overflow")?;
                }
                let retry = format!("state/{id}.retry");
                if row["fail_once"] == true && !Path::new(&retry).exists() {
                    fs::write(retry, b"retryable\n")?;
                    eprintln!("controlled retryable failure");
                    return Ok(Some((ExitCode::from(7), 7)));
                }
            } else { value = value.checked_mul(value).context("result overflow")?; }
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
                if stream == "stdout" { std::io::stdout().write_all(&bytes)?; }
                else { std::io::stderr().write_all(&bytes)?; }
            }
        }
        Some("hash") if args.len() == 2 => println!("{:x}", Sha256::digest(fs::read(&args[1])?)),
        _ => return Ok(None),
    }
    Ok(Some((ExitCode::SUCCESS, 0)))
}

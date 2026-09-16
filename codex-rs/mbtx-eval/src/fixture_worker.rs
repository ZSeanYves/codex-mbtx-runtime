//! Deliberately separate from the evaluator: no task protocol or oracle APIs.
use std::io::Read;
use std::process::ExitCode;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde_json::json;

fn run(args: &[String]) -> Result<ExitCode> {
    match args.first().map(String::as_str) {
        #[cfg(unix)]
        Some("launch") if args.len() >= 3 => {
            use std::os::unix::process::CommandExt;
            std::fs::write(&args[1], b"entered sandbox\n")?;
            return Err(std::process::Command::new(&args[2])
                .args(&args[3..])
                .exec()
                .into());
        }
        Some("square" | "checked-square") if args.len() == 2 => {
            let value: i32 = args[1].parse().context("expected signed integer")?;
            if args[0] == "checked-square" && value % 5 == 0 {
                eprintln!("controlled rejection");
                return Ok(ExitCode::from(7));
            }
            println!(
                "{}",
                json!({"value":value,"square":i64::from(value)*i64::from(value)})
            );
        }
        Some("echo") => {
            let mut bytes = Vec::new();
            std::io::stdin().take(2_000_001).read_to_end(&mut bytes)?;
            anyhow::ensure!(bytes.len() <= 2_000_000, "stdin fixture limit exceeded");
            println!(
                "{}",
                json!({"args":&args[1..],"stdin":std::str::from_utf8(&bytes)?,"stdin_bytes":bytes.len()})
            );
        }
        _ => bail!("usage: fixture-worker square N | checked-square N | echo [ARGS...]"),
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(status) => status,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

//! Deliberately separate from the evaluator: no task protocol or oracle APIs.
use std::io::Read;
use std::process::ExitCode;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde_json::json;

mod fixture_actions;

fn run(args: &[String]) -> Result<u8> {
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
                return Ok(7);
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
    Ok(0)
}

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let actions = matches!(
        args.first().map(String::as_str),
        Some("job" | "recover" | "emit" | "hash")
    );
    let result = (|| -> Result<ExitCode> {
        let observed = actions
            || matches!(
                args.first().map(String::as_str),
                Some("square" | "checked-square" | "echo")
            );
        if observed {
            fixture_actions::receipt("started", &args, None)?;
            let result = if actions {
                fixture_actions::execute(&args)
            } else {
                run(&args).map(|code| Some((ExitCode::from(code), code)))
            };
            let code = match &result {
                Ok(Some((_, code))) => *code,
                _ => 2,
            };
            fixture_actions::receipt("completed", &args, Some(code))?;
            return Ok(result?.context("invalid worker action")?.0);
        }
        run(&args).map(ExitCode::from)
    })();
    match result {
        Ok(status) => status,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

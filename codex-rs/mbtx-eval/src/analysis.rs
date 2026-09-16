use std::path::Path;
use std::process::Stdio;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde_json::Value;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::ChildStdin;
use tokio::process::ChildStdout;
use tokio::process::Command;

pub(crate) struct Analysis {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Analysis {
    pub async fn start(bundle: &Path) -> Result<Self> {
        let mut child = Command::new(bundle.join("moonrun"))
            .arg(bundle.join("evaluation-model.wasm"))
            .args(["--", "--worker"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .context("start prebuilt MoonBit analysis worker")?;
        let input = child.stdin.take().context("analysis stdin")?;
        let output = BufReader::new(child.stdout.take().context("analysis stdout")?);
        Ok(Self {
            child,
            input,
            output,
        })
    }

    pub async fn query(&mut self, mut request: Value) -> Result<Value> {
        request["schema_version"] = 1.into();
        let mut bytes = serde_json::to_vec(&request)?;
        bytes.push(b'\n');
        self.input.write_all(&bytes).await?;
        self.input.flush().await?;
        let mut line = String::new();
        ensure!(
            self.output.read_line(&mut line).await? > 0,
            "analysis worker stopped: {:?}",
            self.child.try_wait()?
        );
        let mut response: Value =
            serde_json::from_str(&line).context("analysis worker response")?;
        ensure!(response["ok"] == true, "analysis: {}", response["error"]);
        Ok(response["result"].take())
    }
}

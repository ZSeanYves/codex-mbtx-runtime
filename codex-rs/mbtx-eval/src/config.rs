use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RelayConfig {
    pub model_provider: String,
    pub model: String,
    pub model_reasoning_effort: String,
    pub model_providers: BTreeMap<String, Provider>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Provider {
    pub name: String,
    pub base_url: String,
    pub wire_api: String,
    pub env_key: String,
    pub requires_openai_auth: bool,
    pub request_max_retries: u64,
    pub stream_max_retries: u64,
}

impl RelayConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let config: Self = toml::from_str(&fs::read_to_string(path)?)?;
        let provider = config.provider()?;
        let url = reqwest::Url::parse(&provider.base_url)?;
        ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "provider URL must not contain credentials or query parameters"
        );
        ensure!(
            url.scheme() == "https"
                || (url.scheme() == "http"
                    && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))),
            "relay requires HTTPS; HTTP is allowed only for loopback validation"
        );
        ensure!(
            provider.wire_api == "responses" && !provider.requires_openai_auth,
            "evaluation requires Responses and env_key authentication"
        );
        ensure!(
            provider.request_max_retries <= 2 && provider.stream_max_retries <= 2,
            "request_max_retries and stream_max_retries must each be 0..=2 retries after the initial attempt"
        );
        ensure!(
            !config.model.is_empty() && !provider.env_key.is_empty(),
            "model and env_key are required"
        );
        Ok(config)
    }

    pub fn provider(&self) -> Result<&Provider> {
        self.model_providers
            .get(&self.model_provider)
            .context("selected model_provider is missing")
    }

    pub fn credential(&self, file: Option<&Path>) -> Result<String> {
        let name = &self.provider()?.env_key;
        let key = if let Some(file) = file {
            let json: Value = serde_json::from_slice(&fs::read(file)?)?;
            json[name]
                .as_str()
                .context("credential JSON lacks the configured env_key")?
                .to_owned()
        } else {
            std::env::var(name).with_context(|| {
                format!("set {name} or use --credentials-file with private JSON")
            })?
        };
        ensure!(
            !key.trim().is_empty() && key != "sk-xxx" && !key.contains(['\r', '\n']),
            "credential is empty, a placeholder, or contains a newline"
        );
        Ok(key)
    }
}

pub(crate) struct AttemptContext<'a> {
    pub arm: &'a str,
    pub endpoint: &'a str,
    pub attempt_id: &'a str,
    pub instructions: &'a str,
    pub work: &'a Path,
    pub evidence: &'a Path,
}

pub(crate) fn child_config(
    config: &RelayConfig,
    bundle: &Path,
    bundle_info: &Value,
    context: AttemptContext<'_>,
) -> Result<String> {
    let AttemptContext {
        arm,
        endpoint,
        attempt_id,
        instructions,
        work,
        evidence,
    } = context;
    let provider = config.provider()?;
    let mbtx = arm == "mbtx_program";
    let mut value = toml::Table::new();
    for (key, text) in [
        ("model_provider", config.model_provider.as_str()),
        ("model", config.model.as_str()),
        (
            "model_reasoning_effort",
            config.model_reasoning_effort.as_str(),
        ),
        ("approval_policy", "never"),
        ("web_search", "disabled"),
        ("developer_instructions", instructions),
    ] {
        value.insert(key.to_owned(), text.into());
    }
    value.insert(
        "model_catalog_json".into(),
        bundle.join("models.json").to_string_lossy().as_ref().into(),
    );
    value.insert("tool_output_token_limit".into(), 4096.into());
    let socket: String = serde_json::from_slice(&fs::read(work.join("worker-socket.json"))?)?;
    let mut environment = serde_json::json!({"MBTX_WORKER_SOCKET":socket});
    if work.join("tools").is_dir() {
        environment["PATH"] = serde_json::json!(format!(
            "{}:{}",
            work.join("tools").display(),
            std::env::var("PATH").context("host PATH")?
        ));
    }
    value.insert(
        "shell_environment_policy".into(),
        toml::Value::try_from(serde_json::json!({"set":environment}))?,
    );
    value.insert("project_doc_max_bytes".into(), 0.into());
    value.insert("project_root_markers".into(), toml::Value::Array(vec![]));
    value.insert("allow_login_shell".into(), false.into());
    value.insert(
        "agents".into(),
        toml::Value::try_from(BTreeMap::from([("enabled", false)]))?,
    );
    value.insert(
        "skills".into(),
        toml::Value::try_from(
            serde_json::json!({"include_instructions":false,"bundled":{"enabled":false}}),
        )?,
    );
    value.insert("suppress_unstable_features_warning".into(), true.into());
    value.insert(
        "analytics".into(),
        toml::Value::try_from(BTreeMap::from([("enabled", false)]))?,
    );
    let mut providers = toml::Table::new();
    providers.insert(
        config.model_provider.clone(),
        toml::Value::try_from(BTreeMap::from([
            ("name", toml::Value::from(provider.name.clone())),
            ("base_url", endpoint.into()),
            ("wire_api", "responses".into()),
            ("env_key", "MBTX_LOCAL_KEY".into()),
            ("requires_openai_auth", false.into()),
            ("supports_websockets", false.into()),
            (
                "request_max_retries",
                (provider.request_max_retries as i64).into(),
            ),
            (
                "stream_max_retries",
                (provider.stream_max_retries as i64).into(),
            ),
            ("stream_idle_timeout_ms", 120_000.into()),
        ]))?,
    );
    value.insert("model_providers".into(), providers.into());
    value.insert(
        "features".into(),
        toml::Value::try_from(BTreeMap::from([
            ("shell_tool", !mbtx),
            ("unified_exec", !mbtx),
            ("code_mode", true),
            ("code_mode_only", false),
            ("code_mode_host", true),
            ("multi_agent_v2", false),
            ("plugins", false),
            ("apps", false),
            ("memories", false),
            // Upstream enables this by default and it bypasses stream_max_retries.
            ("unbounded_connection_retries", false),
        ]))?,
    );
    value.insert("default_permissions".into(), "evaluation".into());
    value.insert(
        "permissions".into(),
        crate::workspace::permissions(
            work,
            bundle,
            Path::new(bundle_info["moon_home"].as_str().context("MoonBit root")?),
        )?,
    );
    let moon = bundle_info["moon_path"]
        .as_str()
        .context("bundle moon path")?;
    let moonrun = bundle_info["moonrun_path"]
        .as_str()
        .context("bundle moonrun path")?;
    value.insert(
        "mbtx".into(),
        toml::Value::try_from(BTreeMap::from([
            ("enabled", toml::Value::from(mbtx)),
            ("moon", moon.into()),
            ("moonrun", moonrun.into()),
            (
                "reference_directory",
                bundle.join("reference").to_string_lossy().as_ref().into(),
            ),
            (
                "output_directory",
                evidence.join("resources").to_string_lossy().as_ref().into(),
            ),
            ("observation_socket", socket.into()),
            (
                "dependency_cache",
                bundle
                    .join("dependencies")
                    .to_string_lossy()
                    .as_ref()
                    .into(),
            ),
        ]))?,
    );
    let mut otel = toml::Table::new();
    otel.insert("environment".into(), "mbtx-pilot".into());
    otel.insert("log_user_prompt".into(), false.into());
    for field in ["exporter", "trace_exporter"] {
        let mut export = toml::Table::new();
        let signal = if field == "exporter" {
            "logs"
        } else {
            "traces"
        };
        export.insert("endpoint".into(), format!("{endpoint}/{signal}").into());
        export.insert("protocol".into(), "json".into());
        let mut kind = toml::Table::new();
        kind.insert("otlp-http".into(), export.into());
        otel.insert(field.into(), kind.into());
    }
    otel.insert("metrics_exporter".into(), "none".into());
    otel.insert(
        "span_attributes".into(),
        toml::Value::try_from(BTreeMap::from([
            ("mbtx.attempt_id", attempt_id),
            ("mbtx.arm", arm),
        ]))?,
    );
    value.insert("otel".into(), otel.into());
    Ok(toml::to_string_pretty(&value)?)
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;

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
            provider.wire_api == "responses"
                && !provider.requires_openai_auth
                && provider.request_max_retries == 0
                && provider.stream_max_retries == 0,
            "evaluation requires Responses, env_key authentication and disabled request/stream retries"
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

pub(crate) fn child_config(
    config: &RelayConfig,
    bundle: &Path,
    bundle_info: &Value,
    arm: &str,
    endpoint: &str,
    attempt_id: &str,
    instructions: &str,
) -> Result<String> {
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
        ("sandbox_mode", "workspace-write"),
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
            ("request_max_retries", 0.into()),
            ("stream_max_retries", 0.into()),
            ("stream_idle_timeout_ms", 120_000.into()),
        ]))?,
    );
    value.insert("model_providers".into(), providers.into());
    value.insert(
        "features".into(),
        toml::Value::try_from(BTreeMap::from([
            ("shell_tool", !mbtx),
            ("unified_exec", true),
            ("code_mode", false),
            ("code_mode_only", false),
            ("code_mode_host", false),
            ("multi_agent", false),
            ("multi_agent_v2", false),
            ("plugins", false),
            ("apps", false),
            ("memories", false),
        ]))?,
    );
    value.insert(
        "sandbox_workspace_write".into(),
        toml::Value::try_from(BTreeMap::from([("network_access", false)]))?,
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

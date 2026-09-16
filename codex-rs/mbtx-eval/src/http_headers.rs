//! Preserve Codex protocol metadata. Credentials and HTTP framing belong to
//! the local gateway and its upstream client.
use anyhow::Result;
use axum::http::HeaderMap;
use axum::http::HeaderName;
use axum::http::HeaderValue;
use serde_json::Value;

pub(crate) fn forward(headers: &HeaderMap) -> HeaderMap {
    let connection: Vec<_> = headers
        .get_all("connection")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(|name| name.trim().to_ascii_lowercase())
        .collect();
    headers
        .iter()
        .filter(|(name, _)| {
            !matches!(
                name.as_str(),
                "authorization"
                    | "proxy-authorization"
                    | "x-api-key"
                    | "cookie"
                    | "set-cookie"
                    | "host"
                    | "content-length"
                    | "content-encoding"
                    | "accept-encoding"
                    | "connection"
                    | "keep-alive"
                    | "transfer-encoding"
                    | "te"
                    | "trailer"
                    | "upgrade"
            ) && !connection.iter().any(|hop| hop == name.as_str())
        })
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

pub(crate) fn facts(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.to_string(), value.to_owned()))
        })
        .collect()
}

pub(crate) fn recorded(value: &Value) -> Result<HeaderMap> {
    let entries: Vec<(String, String)> = serde_json::from_value(value.clone())?;
    let mut headers = HeaderMap::new();
    for (name, value) in entries {
        headers.append(
            HeaderName::from_bytes(name.as_bytes())?,
            HeaderValue::from_str(&value)?,
        );
    }
    Ok(forward(&headers))
}

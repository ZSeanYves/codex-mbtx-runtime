use super::*;
use axum::routing::post;
use pretty_assertions::assert_eq;
use std::sync::atomic::AtomicUsize;

#[test]
fn cooldown_uses_seconds_dates_and_invalid_fallback() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    assert_eq!(retry_delay(Some("12"), now), Duration::from_secs(12));
    let date = httpdate::fmt_http_date(now + Duration::from_secs(45));
    assert_eq!(retry_delay(Some(&date), now), Duration::from_secs(45));
    assert_eq!(retry_delay(Some("invalid"), now), Duration::from_secs(30));
    assert_eq!(retry_delay(None, now), Duration::from_secs(30));
}

#[tokio::test]
async fn cancellation_before_headers_is_recorded_before_drain_and_seal() -> Result<()> {
    preheader_cancellation(WaitingClient::Connected).await
}

#[tokio::test]
async fn client_disconnect_before_headers_preserves_the_exchange_until_cancellation() -> Result<()>
{
    preheader_cancellation(WaitingClient::Disconnected).await
}

enum WaitingClient {
    Connected,
    Disconnected,
}

async fn preheader_cancellation(client: WaitingClient) -> Result<()> {
    let received = Arc::new(Notify::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let upstream = Router::new().route(
        "/responses",
        post({
            let received = Arc::clone(&received);
            move || {
                let received = Arc::clone(&received);
                async move {
                    received.notify_one();
                    std::future::pending::<Response>().await
                }
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let gate = Gate::start(
        format!("http://{address}"),
        "test-key".into(),
        Duration::ZERO,
    )
    .await?;
    let temp = tempfile::tempdir()?;
    std::fs::create_dir(temp.path().join("http"))?;
    let route = gate
        .add(
            "cancel".into(),
            Route {
                directory: temp.path().to_owned(),
                arm: "probe".into(),
                token: "local".into(),
                replies: None,
                active: AtomicBool::new(true),
                budget_exhausted: AtomicBool::new(false),
                requests: AtomicUsize::new(0),
                otel_requests: AtomicUsize::new(0),
                request_budget: 1,
                observation_failures: AtomicUsize::new(0),
                closed: Notify::new(),
                otel_write: std::sync::Mutex::new(()),
            },
        )
        .await;
    let request = reqwest::Client::builder()
        .no_proxy()
        .build()?
        .post(format!("{}/a/cancel/v1/responses", gate.endpoint))
        .bearer_auth("local")
        .json(&json!({"input":[]}));
    let response = tokio::spawn(async move { request.send().await });
    tokio::time::timeout(Duration::from_secs(5), received.notified()).await?;
    let response = match client {
        WaitingClient::Connected => Some(response),
        WaitingClient::Disconnected => {
            response.abort();
            assert!(response.await.unwrap_err().is_cancelled());
            // The upstream has accepted the request. A downstream disconnect
            // must not silently release its slot or abandon the evidence.
            assert!(
                tokio::time::timeout(Duration::from_millis(100), gate.drain())
                    .await
                    .is_err(),
                "client disconnect abandoned an in-flight upstream exchange"
            );
            None
        }
    };
    route.close();
    tokio::time::timeout(Duration::from_secs(5), gate.drain()).await?;
    let result = crate::evidence::read_json(&temp.path().join("http/request-0000/result.json"))?;
    let send = result["send_ns"].as_u64().context("send boundary")?;
    let end = result["end_ns"].as_u64().context("cancel boundary")?;
    assert!(end >= send);
    assert_eq!(
        result,
        json!({
            "status_code":null,"transport_error":null,"complete":false,"cancelled":true,
            "phase":"awaiting_headers","send_ns":send,"end_ns":end,
            "first_byte_ns":null,"response_bytes":0,"clock_domain":gate.clock_domain,
        })
    );
    assert_eq!(route.observation_failures.load(Ordering::SeqCst), 0);
    assert!(!temp.path().join("http/request-0000/headers.json").exists());
    crate::evidence::seal(temp.path())?;
    if let Some(response) = response {
        assert_eq!(response.await??.status(), StatusCode::REQUEST_TIMEOUT);
    }
    gate.stop().await;
    assert!(crate::evidence::verify(temp.path())?);
    server.abort();
    Ok(())
}

#[tokio::test]
async fn serializes_entire_stream_and_records_wire_failures_without_key() -> Result<()> {
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let count = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let upstream = Router::new().route(
        "/responses",
        post({
            let active = active.clone();
            let maximum = maximum.clone();
            let count = count.clone();
            move |headers: HeaderMap| {
                let active = active.clone();
                let maximum = maximum.clone();
                let count = count.clone();
                async move {
                    assert_eq!(headers["authorization"], "Bearer private-key-never-persist");
                    assert_eq!(
                        headers.get_all("content-type").iter().collect::<Vec<_>>(),
                        vec!["application/json"],
                        "the relay must receive exactly one JSON Content-Type"
                    );
                    assert_eq!(headers["accept-encoding"], "identity");
                    assert_eq!(headers["x-codex-turn-state"], "client-turn");
                    maximum.fetch_max(active.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
                    let index = count.fetch_add(1, Ordering::SeqCst);
                    let body = futures::stream::once(async move {
                        tokio::time::sleep(Duration::from_millis(80)).await;
                        active.fetch_sub(1, Ordering::SeqCst);
                        Ok::<_, std::io::Error>(Bytes::from_static(b"data: complete\n\n"))
                    });
                    Response::builder()
                        .status(if index == 0 { 429 } else { 200 })
                        .header("retry-after", "0")
                        .header("x-codex-turn-state", "provider-turn")
                        .body(Body::from_stream(body))
                        .unwrap()
                }
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });
    let gate = Gate::start(
        format!("http://{address}"),
        "private-key-never-persist".into(),
        Duration::from_millis(120),
    )
    .await?;
    let temp = tempfile::tempdir()?;
    for child in ["http", "otel", "trace"] {
        std::fs::create_dir(temp.path().join(child))?;
    }
    let route = gate
        .add(
            "test".into(),
            Route {
                directory: temp.path().to_owned(),
                arm: "probe".into(),
                token: "local".into(),
                replies: None,
                active: AtomicBool::new(true),
                budget_exhausted: AtomicBool::new(false),
                requests: AtomicUsize::new(0),
                otel_requests: AtomicUsize::new(0),
                request_budget: 3,
                observation_failures: AtomicUsize::new(0),
                closed: Notify::new(),
                otel_write: std::sync::Mutex::new(()),
            },
        )
        .await;
    let client = reqwest::Client::builder().no_proxy().build()?;
    let send = || async {
        let response = client
            .post(format!("{}/a/test/v1/responses", gate.endpoint))
            .bearer_auth("local")
            .header("x-codex-turn-state", "client-turn")
            .json(&json!({"input":[]}))
            .send()
            .await?;
        let status = response.status();
        assert_eq!(response.headers()["x-codex-turn-state"], "provider-turn");
        response.bytes().await?;
        Ok::<_, anyhow::Error>(status)
    };
    let (a, b) = tokio::join!(send(), send());
    let statuses = [a?, b?];
    assert!(statuses.contains(&StatusCode::TOO_MANY_REQUESTS));
    assert_eq!(maximum.load(Ordering::SeqCst), 1);
    gate.drain().await;
    assert_eq!(route.requests.load(Ordering::SeqCst), 2);
    let billing = client
        .post(format!(
            "{}/a/test/v1/analytics/codex/turn-costs",
            gate.endpoint
        ))
        .bearer_auth("local")
        .json(&json!({"turn_ids":["owned-turn"]}))
        .send()
        .await?;
    assert_eq!(billing.status(), StatusCode::NOT_FOUND);
    assert_eq!(route.requests.load(Ordering::SeqCst), 2);
    assert_eq!(count.load(Ordering::SeqCst), 2);
    let first =
        crate::evidence::read_json(&temp.path().join("http/request-0000/headers.json"))?["send_ns"]
            .as_u64()
            .context("first send")?;
    let second =
        crate::evidence::read_json(&temp.path().join("http/request-0001/headers.json"))?["send_ns"]
            .as_u64()
            .context("second send")?;
    assert!(
        second.abs_diff(first) >= 120_000_000,
        "minimum start interval includes the full response stream"
    );
    let result = crate::evidence::read_json(&temp.path().join("http/request-0000/result.json"))?;
    assert_eq!(result["complete"], true);
    for entry in walkdir::WalkDir::new(temp.path()) {
        let entry = entry?;
        if entry.file_type().is_file() {
            assert!(
                !String::from_utf8_lossy(&std::fs::read(entry.path())?)
                    .contains("private-key-never-persist")
            );
        }
    }
    gate.stop().await;
    server.abort();
    Ok(())
}

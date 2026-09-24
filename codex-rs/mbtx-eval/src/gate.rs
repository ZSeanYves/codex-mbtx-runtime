use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

use anyhow::Context;
use anyhow::Result;
use axum::Router;
use axum::body::Body;
use axum::body::Bytes;
use axum::extract::DefaultBodyLimit;
use axum::extract::Path;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::http::header::ACCEPT_ENCODING;
use axum::http::header::CONTENT_TYPE;
use axum::response::Response;
use axum::routing::post;
use futures::StreamExt;
use serde_json::json;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::evidence::json_new;
use crate::evidence::now_ms;
use crate::evidence::write_new;
use crate::replay::Reply;

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;

pub(crate) struct Route {
    pub directory: PathBuf,
    pub arm: String,
    pub execution_mode: String,
    pub token: String,
    pub replies: Option<Vec<Reply>>,
    pub active: AtomicBool,
    pub requests: AtomicUsize,
    pub replay_cursor: AtomicUsize,
    pub otel_requests: AtomicUsize,
    pub observation_failures: AtomicUsize,
    pub closed: Notify,
    pub otel_write: std::sync::Mutex<()>,
}

impl Route {
    pub fn close(&self) {
        let _write = self.otel_write.lock().unwrap_or_else(|error| {
            self.observation_failures.fetch_add(1, Ordering::SeqCst);
            error.into_inner()
        });
        self.active.store(false, Ordering::SeqCst);
        self.closed.notify_waiters();
        // Retain a permit if the one active stream is between checking the flag
        // and registering its cancellation future.
        self.closed.notify_one();
    }
}

pub(crate) struct Gate {
    pub endpoint: String,
    pub routes: RwLock<HashMap<String, Arc<Route>>>,
    epoch: Instant,
    clock_domain: String,
    interval: Duration,
    rate: Arc<Mutex<Instant>>,
    upstream: String,
    key: String,
    client: reqwest::Client,
    server: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Gate {
    pub async fn start(upstream: String, key: String, interval: Duration) -> Result<Arc<Self>> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let gate = Arc::new(Self {
            endpoint: format!("http://{address}"),
            routes: RwLock::new(HashMap::new()),
            epoch: Instant::now(),
            clock_domain: format!("collector-{}", uuid::Uuid::new_v4()),
            interval,
            rate: Arc::new(Mutex::new(Instant::now())),
            upstream,
            key,
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                // Codex owns the two observable retry counters. A hidden
                // reqwest resend here would evade both the gate and evidence.
                .retry(reqwest::retry::never())
                // The owning attempt closes the route at its total deadline.
                // An active response stream must not have a shorter HTTP cap.
                .build()?,
            server: Mutex::new(None),
        });
        let router = Router::new()
            .route("/a/{id}/v1/{*route}", post(handle))
            .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
            .with_state(Arc::clone(&gate));
        *gate.server.lock().await = Some(tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        }));
        Ok(gate)
    }

    pub async fn add(&self, id: String, route: Route) -> Arc<Route> {
        let route = Arc::new(route);
        self.routes.write().await.insert(id, Arc::clone(&route));
        route
    }

    pub async fn stop(&self) {
        for route in self.routes.read().await.values() {
            route.close();
        }
        if let Some(server) = self.server.lock().await.take() {
            server.abort();
        }
    }

    pub async fn drain(&self) {
        drop(self.rate.lock().await);
    }
}

pub(crate) fn retry_delay(value: Option<&str>, now: SystemTime) -> Duration {
    value
        .and_then(|v| {
            v.parse::<u64>().ok().map(Duration::from_secs).or_else(|| {
                httpdate::parse_http_date(v)
                    .ok()
                    .map(|date| date.duration_since(now).unwrap_or_default())
            })
        })
        .unwrap_or(Duration::from_secs(30))
}

fn transport_error_details(error: &reqwest::Error) -> serde_json::Value {
    // Do not persist Display/Debug or the source chain: they can include the
    // upstream URL or credentials. These flags describe transport, not blame.
    json!({
        "is_timeout": error.is_timeout(),
        "is_connect": error.is_connect(),
        "is_body": error.is_body(),
    })
}

async fn handle(
    State(gate): State<Arc<Gate>>,
    Path((id, path)): Path<(String, String)>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Response {
    // Hyper drops the handler when Codex disconnects before response headers.
    // The exchange must retain its upstream slot and observations until it
    // finishes or the owning attempt closes, even if there is no client left.
    tokio::spawn(async move {
        match exchange(Arc::clone(&gate), id.clone(), path, headers, bytes).await {
            Ok(response) => response,
            Err(error) => {
                if let Some(route) = gate.routes.read().await.get(&id)
                    && route.active.load(Ordering::SeqCst)
                {
                    route.observation_failures.fetch_add(1, Ordering::SeqCst);
                    let _ = json_new(
                        &route
                            .directory
                            .join(format!("collector-error-{}.json", uuid::Uuid::new_v4())),
                        &json!({"error":error.to_string(),"wall_time_ms":now_ms()}),
                    );
                }
                let mut response = Response::new(Body::from(format!("collector error: {error:#}")));
                *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                response
            }
        }
    })
    .await
    .unwrap_or_else(|error| {
        let mut response = Response::new(Body::from(format!("collector task failed: {error}")));
        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        response
    })
}

async fn exchange(
    gate: Arc<Gate>,
    id: String,
    path: String,
    headers: HeaderMap,
    bytes: Bytes,
) -> Result<Response> {
    let route = gate
        .routes
        .read()
        .await
        .get(&id)
        .cloned()
        .context("unknown attempt route")?;
    if path == "traces" || path == "logs" || path == "metrics" {
        let _write = route
            .otel_write
            .lock()
            .map_err(|_| anyhow::anyhow!("OTel evidence lock poisoned"))?;
        anyhow::ensure!(
            route.active.load(Ordering::SeqCst),
            "attempt already sealed"
        );
        let index = route.otel_requests.fetch_add(1, Ordering::SeqCst);
        write_new(
            &route
                .directory
                .join("otel")
                .join(format!("{index:05}-{path}.json")),
            &bytes,
        )?;
        return Ok(Response::new(Body::from("{}")));
    }
    anyhow::ensure!(
        path == "responses" || path == "responses/compact" || path == "analytics/codex/turn-costs",
        "unsupported provider endpoint: {path}"
    );
    anyhow::ensure!(
        headers.get("authorization").and_then(|v| v.to_str().ok())
            == Some(&format!("Bearer {}", route.token)),
        "invalid local credential"
    );
    anyhow::ensure!(
        route.active.load(Ordering::SeqCst),
        "attempt already closed"
    );
    if path == "analytics/codex/turn-costs" {
        // Native OTel logging starts this optional upstream billing worker.
        // The gateway has no billing service. Preserve the local response and
        // its provenance without sending an extra relay request.
        let _write = route
            .otel_write
            .lock()
            .map_err(|_| anyhow::anyhow!("evidence lock poisoned"))?;
        anyhow::ensure!(
            route.active.load(Ordering::SeqCst),
            "attempt already closed"
        );
        let request: serde_json::Value = serde_json::from_slice(&bytes)?;
        json_new(
            &route
                .directory
                .join(format!("auxiliary-{}.json", uuid::Uuid::new_v4())),
            &json!({"path":path,"purpose":"billing_lookup","status_code":404,"forwarded":false,"origin":"local evaluation gateway; billing unavailable by protocol","request":request,"wall_time_ms":now_ms()}),
        )?;
        return Ok(Response::builder()
            .status(404)
            .header("content-type", "application/json")
            .body(Body::from(
                "{\"error\":\"billing lookup unavailable in this evaluation protocol\"}",
            ))?);
    }
    let ordinal = route.requests.fetch_add(1, Ordering::SeqCst);
    let body: serde_json::Value = serde_json::from_slice(&bytes)?;
    let directory = route
        .directory
        .join("http")
        .join(format!("request-{ordinal:04}"));
    std::fs::create_dir(&directory)?;
    let preparing = gate.epoch.elapsed().as_nanos() as u64;
    let mut forwarded_headers = crate::http_headers::forward(&headers);
    // RequestBuilder::header appends rather than replaces. Normalize these
    // gateway-owned headers before recording or sending them, exactly once.
    forwarded_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    forwarded_headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    json_new(
        &directory.join("request-headers.json"),
        &json!(crate::http_headers::facts(&forwarded_headers)),
    )?;
    write_new(&directory.join("request.json"), &bytes)?;
    // Preserve rejected requests too; they are harness evidence, never relay failures.
    if path == "responses" {
        let contract = route.directory.join("task-contract.json");
        if contract.exists() {
            crate::task_contract::validate(&body, &crate::evidence::read_json(&contract)?)?;
        }
        crate::request_contract::validate(&body, &route.arm, &route.execution_mode)?;
        if route
            .replies
            .as_ref()
            .is_some_and(|replies| replies.iter().any(|reply| reply.wait_for_code_cells))
        {
            crate::request_contract::validate_replay_history(
                &body,
                &route.arm,
                &route.execution_mode,
            )?;
        }
    }
    let queued = gate.epoch.elapsed().as_nanos() as u64;
    let mut permit = gate.rate.clone().lock_owned().await;
    anyhow::ensure!(
        route.active.load(Ordering::SeqCst),
        "cancelled in rate queue"
    );
    if route.replies.is_none() {
        tokio::select! { _=tokio::time::sleep_until((*permit).into())=>(), _=route.closed.notified()=>anyhow::bail!("attempt cancelled in rate queue") }
    }
    anyhow::ensure!(
        route.active.load(Ordering::SeqCst),
        "cancelled before upstream send"
    );
    let sent = gate.epoch.elapsed().as_nanos() as u64;
    json_new(
        &directory.join("started.json"),
        &json!({"request_id":format!("{id}:{ordinal}"),"route":path,"queued_ns":queued,"slot_acquired_ns":sent,"queue_wait_ns":sent-queued,"request_evidence_write_ns":queued-preparing,"clock_domain":gate.clock_domain,"wall_time_ms":now_ms()}),
    )?;
    let reply = if let Some(replies) = &route.replies {
        let next = replies
            .get(route.replay_cursor.load(Ordering::SeqCst))
            .context("recorded responses exhausted")?;
        if let Some(wait) = next
            .wait_for_code_cells
            .then(|| crate::replay::pending_wait(&body, ordinal))
            .transpose()?
            .flatten()
        {
            Some(wait)
        } else {
            route.replay_cursor.fetch_add(1, Ordering::SeqCst);
            Some(next.clone())
        }
    } else {
        None
    };
    *permit = Instant::now() + gate.interval;
    let send_ns = gate.epoch.elapsed().as_nanos() as u64;
    let upstream = if reply.is_none() {
        Some(tokio::select! {
            value=gate.client.post(format!("{}/{}",gate.upstream.trim_end_matches('/'),path))
                .headers(forwarded_headers).bearer_auth(&gate.key).body(bytes).send()=>value,
            _=route.closed.notified()=>{
                // Closing an attempt is a local terminal event, not a relay
                // error. Persist it before releasing the serialization permit
                // so drain/seal cannot lose a request waiting for headers.
                let end_ns = gate.epoch.elapsed().as_nanos() as u64;
                if let Err(error) = json_new(
                    &directory.join("result.json"),
                    &json!({"status_code":null,"transport_error":null,"complete":false,"cancelled":true,"phase":"awaiting_headers","send_ns":send_ns,"end_ns":end_ns,"first_byte_ns":null,"response_bytes":0,"clock_domain":gate.clock_domain}),
                ) {
                    route.observation_failures.fetch_add(1, Ordering::SeqCst);
                    return Err(error);
                }
                return Ok(Response::builder().status(StatusCode::REQUEST_TIMEOUT)
                    .body(Body::from("evaluation attempt cancelled before response headers"))?);
            },
        })
    } else {
        None
    };
    let (status, content_type, retry_after, response) = match upstream {
        Some(Ok(response)) => (
            response.status().as_u16(),
            response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_owned(),
            response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned),
            Some(response),
        ),
        Some(Err(error)) => {
            json_new(
                &directory.join("result.json"),
                &json!({"status_code":null,"transport_error":true,"transport_error_details":transport_error_details(&error),"phase":"awaiting_headers","send_ns":send_ns,"end_ns":gate.epoch.elapsed().as_nanos() as u64,"complete":false}),
            )?;
            return Ok(Response::builder()
                .status(502)
                .body(Body::from("upstream transport failed"))?);
        }
        None => {
            let value = reply.as_ref().context("fixed response")?;
            (
                value.status,
                value.content_type.clone(),
                value.retry_after.clone(),
                None,
            )
        }
    };
    if status == 429 {
        *permit =
            (*permit).max(Instant::now() + retry_delay(retry_after.as_deref(), SystemTime::now()));
    }
    let headers_ns = gate.epoch.elapsed().as_nanos() as u64;
    let response_headers = response
        .as_ref()
        .map(|r| crate::http_headers::forward(r.headers()))
        .unwrap_or_else(|| {
            reply
                .as_ref()
                .map(|r| r.headers.clone())
                .unwrap_or_default()
        });
    json_new(
        &directory.join("headers.json"),
        &json!({"status_code":status,"content_type":content_type,"retry_after":retry_after,"send_ns":send_ns,"headers_ns":headers_ns,"protocol_headers":crate::http_headers::facts(&response_headers)}),
    )?;
    let file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("response.body"))
        .await?;
    let (sender, receiver) = mpsc::channel::<std::result::Result<Bytes, std::io::Error>>(4);
    let mut stream = match (response, reply) {
        (Some(response), _) => response
            .bytes_stream()
            .map(|value| value.map_err(|error| Some(transport_error_details(&error))))
            .boxed(),
        (None, Some(fixed)) => {
            let end = if fixed.disconnect {
                // Offline injection is not a reqwest failure; do not invent
                // typed transport evidence for it.
                vec![Err(None)]
            } else {
                vec![]
            };
            futures::stream::once(async move {
                tokio::time::sleep(Duration::from_millis(fixed.delay_ms)).await;
                Ok(Bytes::from(fixed.body))
            })
            .chain(futures::stream::iter(end))
            .boxed()
        }
        (None, None) => anyhow::bail!("no upstream or offline response"),
    };
    tokio::spawn(async move {
        let mut file = file;
        let mut size = 0u64;
        let mut disconnected = false;
        let mut transport_error = false;
        let mut transport_details = None;
        let mut evidence_error = false;
        let mut cancelled = false;
        let mut first_byte = None;
        loop {
            if !route.active.load(Ordering::SeqCst) {
                cancelled = true;
                break;
            }
            let chunk = tokio::select! {value=stream.next()=>value,_=route.closed.notified()=>{cancelled=true;break;}};
            let Some(chunk) = chunk else {
                break;
            };
            match chunk {
                Ok(chunk) => {
                    first_byte.get_or_insert_with(|| gate.epoch.elapsed().as_nanos() as u64);
                    size += chunk.len() as u64;
                    if size > 32 * 1024 * 1024 || file.write_all(&chunk).await.is_err() {
                        evidence_error = true;
                        break;
                    }
                    if !disconnected {
                        tokio::select! {
                            sent=sender.send(Ok(chunk))=>{if sent.is_err(){disconnected=true;}},
                            _=route.closed.notified()=>{cancelled=true;break;}
                        }
                    }
                }
                Err(details) => {
                    transport_error = true;
                    transport_details = details;
                    break;
                }
            }
        }
        let stream_ended_ns = gate.epoch.elapsed().as_nanos() as u64;
        if file.sync_all().await.is_err() {
            evidence_error = true;
        }
        let evidence_sync_ns = gate.epoch.elapsed().as_nanos() as u64 - stream_ended_ns;
        if evidence_error {
            route.observation_failures.fetch_add(1, Ordering::SeqCst);
        }
        let result = json!({"status_code":status,"transport_error":transport_error,"transport_error_details":transport_details,"phase":"response_body","evidence_error":evidence_error,"complete":!transport_error && !evidence_error && !cancelled,"cancelled":cancelled,"client_disconnected":disconnected,"first_byte_ns":first_byte,"end_ns":stream_ended_ns,"evidence_sync_ns":evidence_sync_ns,"response_bytes":size});
        if json_new(&directory.join("result.json"), &result).is_err() {
            route.observation_failures.fetch_add(1, Ordering::SeqCst);
        }
        if route.observation_failures.load(Ordering::SeqCst) > 0 || transport_error {
            let _ = sender.try_send(Err(std::io::Error::other(
                "upstream stream or evidence incomplete",
            )));
        }
        drop(sender);
        // The slot remains owned through the entire stream and evidence close.
        drop(permit);
    });
    let mut response = Response::builder()
        .status(StatusCode::from_u16(status)?)
        .header("content-type", content_type);
    if let Some(value) = retry_after {
        response = response.header("retry-after", value);
    }
    response
        .headers_mut()
        .context("response headers")?
        .extend(response_headers);
    Ok(response.body(Body::from_stream(ReceiverStream::new(receiver)))?)
}

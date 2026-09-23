use super::*;
use axum::Json;
use axum::Router;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::post;
use pretty_assertions::assert_eq;
use std::sync::atomic::Ordering;

#[test]
fn exact_slots_validate_against_the_complete_schedule() -> Result<()> {
    let schedule = vec![
        json!({"pair_id":"pair-0016","arms":["shell_tool","mbtx_program"]}),
        json!({"pair_id":"pair-0027","arms":["mbtx_program","shell_tool"]}),
    ];
    let slots = validate_slots(
        &[
            "pair-0016/mbtx_program".into(),
            "pair-0016/shell_tool".into(),
            "pair-0027/mbtx_program".into(),
        ],
        &schedule,
    )?;
    assert_eq!(
        slots,
        [
            "pair-0016/mbtx_program",
            "pair-0016/shell_tool",
            "pair-0027/mbtx_program",
        ]
    );
    assert!(validate_slots(&["pair-0016/unknown".into()], &schedule).is_err());
    assert!(validate_slots(&["pair-0001/shell_tool".into()], &schedule).is_err());
    assert!(
        validate_slots(
            &[
                "pair-0016/mbtx_program".into(),
                "pair-0016/mbtx_program".into()
            ],
            &schedule,
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn unselected_slots_are_not_counted_or_run() {
    let schedule = vec![
        json!({"pair_id":"pair-0016","arms":["shell_tool","mbtx_program"]}),
        json!({"pair_id":"pair-0027","arms":["mbtx_program","shell_tool"]}),
    ];
    let manifest = json!({"schedule_selection":{"slots":[
        "pair-0016/mbtx_program",
        "pair-0016/shell_tool",
        "pair-0027/mbtx_program"
    ]}});
    assert_eq!(selected_arm_count(&manifest, &schedule), 3);
    assert!(slot_selected(
        &manifest,
        &json!("pair-0016"),
        &json!("mbtx_program")
    ));
    assert!(!slot_selected(
        &manifest,
        &json!("pair-0027"),
        &json!("shell_tool")
    ));
}

#[test]
fn seeded_assignment_preserves_pairs_and_reverses_each_input_between_rounds() {
    let tasks = vec![
        json!({"id":"a","scenario":"one","family":"text","variant":0}),
        json!({"id":"b","scenario":"two","family":"data","variant":1}),
    ];
    let pairs = schedule(&tasks, 2, 42);
    assert_eq!(pairs, schedule(&tasks, 2, 42));
    for task in tasks {
        let rows = pairs
            .iter()
            .filter(|p| p["task_id"] == task["id"])
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["arms"][0], rows[1]["arms"][1]);
        assert_eq!(rows[0]["arms"][1], rows[1]["arms"][0]);
        assert_eq!(rows[0]["scenario"], task["scenario"]);
    }
}

#[tokio::test]
async fn probe_uses_native_content_and_preserves_rejected_and_incomplete_responses() -> Result<()> {
    let rejected =
        r#"{"error":{"message":"Unsupported content type","type":"invalid_request_error"}}"#;
    let unfinished = "data: {\"type\":\"response.created\",\"note\":\"response.completed\"}\n\n";
    let completed = "data: {\"type\":\n\
                     data: \"response.completed\",\"response\":{\"id\":\"probe\"}}\n\n";
    for (replies, expected) in [
        (vec![(400, rejected); 3], false),
        (
            vec![(400, rejected), (200, unfinished), (200, completed)],
            true,
        ),
    ] {
        let root = tempfile::tempdir()?;
        fs::create_dir(root.path().join("probes"))?;
        let calls = Arc::new(AtomicUsize::new(0));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let app = Router::new().route(
            "/responses",
            post({
                let calls = Arc::clone(&calls);
                move |headers: HeaderMap, Json(body): Json<Value>| {
                    let index = calls.fetch_add(1, Ordering::SeqCst);
                    let (status, text) = replies
                        .get(index)
                        .copied()
                        .unwrap_or((500, "unexpected request"));
                    async move {
                        // Exercise the actual probe through the forwarding gate.
                        // This endpoint refuses duplicate MIME headers and the
                        // shorthand content shape absent from native Codex traffic.
                        if headers.get_all("content-type").iter().count() != 1
                            || headers
                                .get("content-type")
                                .is_none_or(|v| v != "application/json")
                            || body["input"][0]["content"]
                                != json!([{"type":"input_text","text":"Reply with OK."}])
                            || body["reasoning"]["effort"] != "xhigh"
                        {
                            return (
                                StatusCode::BAD_REQUEST,
                                [("content-type", "application/json")],
                                rejected,
                            )
                                .into_response();
                        }
                        assert_eq!(headers["authorization"], "Bearer probe-test-key");
                        let content_type = if status == 200 {
                            "text/event-stream"
                        } else {
                            "application/json"
                        };
                        (
                            StatusCode::from_u16(status).unwrap(),
                            [("content-type", content_type)],
                            text,
                        )
                            .into_response()
                    }
                }
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let upstream = format!("http://{address}");
        let gate = Gate::start(
            upstream.clone(),
            "probe-test-key".into(),
            Duration::from_millis(2),
        )
        .await?;
        let config = RelayConfig {
            model_provider: "local".into(),
            model: "test-model".into(),
            model_reasoning_effort: "xhigh".into(),
            model_providers: std::collections::BTreeMap::from([(
                "local".into(),
                crate::config::Provider {
                    name: "Local regression fixture".into(),
                    base_url: upstream,
                    wire_api: "responses".into(),
                    env_key: "UNUSED_TEST_KEY".into(),
                    requires_openai_auth: false,
                    request_max_retries: 0,
                    stream_max_retries: 0,
                },
            )]),
        };
        let result = probe(root.path(), &config, &gate).await;
        gate.stop().await;
        server.abort();
        assert_eq!(result?, expected);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        let directories = report::directories(&root.path().join("probes"))?;
        assert_eq!(directories.len(), 3);
        let mut successes = 0;
        for directory in directories {
            assert!(crate::evidence::verify(&directory)?);
            let summary = read_json(&directory.join("summary.json"))?;
            successes += usize::from(summary["success"] == true);
            let response = fs::read_to_string(directory.join("http/request-0000/response.body"))?;
            if summary["status_code"] == 400 {
                assert_eq!(response, rejected);
                assert_eq!(summary["detail"], "Unsupported content type");
            }
            let stored_headers: Vec<(String, String)> = serde_json::from_value(read_json(
                &directory.join("http/request-0000/request-headers.json"),
            )?)?;
            assert_eq!(
                stored_headers
                    .iter()
                    .filter(|(name, _)| name == "content-type")
                    .count(),
                1
            );
            assert!(
                !stored_headers
                    .iter()
                    .any(|(name, _)| name == "authorization")
            );
        }
        assert_eq!(successes, usize::from(expected));
    }
    Ok(())
}

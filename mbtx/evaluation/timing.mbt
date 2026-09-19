///|
fn observed_interval(start : Json, end : Json) -> Json {
  let first : Double? = nullable(start) catch { _ => None }
  let last : Double? = nullable(end) catch { _ => None }
  match (first, last) {
    (Some(first), Some(last)) if last >= first =>
      ((last - first) / 1000000.0).to_json()
    _ => Json::null()
  }
}

///|
fn request_interval(start : Json, end : Json, clock : Json) -> Json {
  match clock {
    String(name) if name != "" => observed_interval(start, end)
    _ => Json::null()
  }
}

///|
/// Describe observed intervals, not an additive causal partition. Each request
/// uses one collector clock; tool durations are local elapsed observations.
fn attempt_timing(facts : Json) -> Json {
  let requests : Array[Json] = @json.from_json(field(facts, "exchanges")) catch {
    _ => []
  }
  let phases : Array[Json] = []
  if field(facts, "tool_results") is Object(tools) {
    let ids = tools.keys().to_array()
    ids.sort()
    for id in ids {
      let result = tools[id]
      let output = field(result, "output")
      if field(output, "target") == Json::string("wasm") {
        phases.push({
          "call_id": id.to_json(),
          "preparation_ms": field(output, "preparation_ms"),
          "cache": field(output, "cache"),
          "compile_ms": if field(output, "cache") == Json::string("hit") {
            (0).to_json()
          } else {
            field(field(output, "build"), "duration_ms")
          },
          "cached_build_original_ms": if field(output, "cache") ==
            Json::string("hit") {
            field(field(output, "build"), "duration_ms")
          } else {
            Json::null()
          },
          "run_ms": field(field(output, "run"), "duration_ms"),
          "build_reused_from": field(output, "build_reused_from"),
          "confidence": "observed",
        })
      }
    }
  }
  {
    "attempt_process_ms": observed_interval(
      (0).to_json(),
      field(field(facts, "outcome"), "elapsed_ns"),
    ),
    "workspace_preparation_ns": field(
      field(facts, "workspace"),
      "preparation_ns",
    ),
    "template_preparation_ns": field(
      field(facts, "workspace"),
      "template_preparation_ns",
    ),
    "post_execution": field(facts, "postprocess"),
    "collector_drain_ns": field(field(facts, "outcome"), "collector_drain_ns"),
    "requests": requests
    .map(r => {
      let started = field(r, "started")
      let headers = field(r, "headers")
      let send = if field(headers, "send_ns") != Json::null() {
        field(headers, "send_ns")
      } else {
        field(r, "send_ns")
      }
      let clock = field(started, "clock_domain")
      {
        "request_id": field(started, "request_id"),
        "clock_domain": clock,
        "queue_and_pacing_ms": request_interval(
          field(started, "queued_ns"),
          field(started, "slot_acquired_ns"),
          clock,
        ),
        "send_to_headers_ms": request_interval(
          send,
          field(headers, "headers_ns"),
          clock,
        ),
        "send_to_first_observed_byte_ms": request_interval(
          send,
          field(r, "first_byte_ns"),
          clock,
        ),
        "send_to_stream_end_ms": request_interval(
          send,
          field(r, "end_ns"),
          clock,
        ),
        "request_evidence_write_ns": field(started, "request_evidence_write_ns"),
        "response_evidence_sync_ns": field(r, "evidence_sync_ns"),
        "confidence": if clock is String(name) && name != "" {
          "observed"
        } else {
          "unknown"
        },
      }
    })
    .to_json(),
    "mbtx_calls": phases.to_json(),
    "unattributed_ms": Json::null(),
    "interpretation": "Intervals overlap and are not summed. Request windows include client transport and observation backpressure; provider compute and relay waiting cannot be separated. Compile/run include spawn, wait and drain. Cache hits retain original diagnostics but perform zero compilation. Unattributed time is unknown, never forced to zero.",
  }
}

///|
/// Derive auditable counts from a native `codex debug trace-reduce` result.
/// No environment, provider credentials, or task-success inference is used.
/// Missing observations constrain totals; observed prefix counts remain visible.
pub fn analyze_steps(trace : Json) -> Json raise {
  let events : Array[StepEvent] = match field(trace, "step_events") {
    Null => []
    value => @json.from_json(value)
  }
  let steps : Map[String, StepRecord] = Map([])
  let ordered_steps : Array[StepRecord] = []
  let scopes : Map[String, RequestScope] = Map([])
  let requests : Map[String, RequestRecord] = Map([])
  let ordered_requests : Array[RequestRecord] = []
  let links : Map[String, InferenceLink] = Map([])
  let warnings : Array[String] = match field(trace, "replay_warnings") {
    Null => []
    value => @json.from_json(value)
  }
  let mut last_seq = 0
  for event in events {
    guard event.seq > last_seq else {
      fail("duplicate or unordered step evidence")
    }
    last_seq = event.seq
    let value = event.observation
    match string_field(value, "type") {
      "started" => {
        let id = string_field(value, "step_id")
        guard !steps.contains(id) else { fail("duplicate step start: \{id}") }
        let step : StepRecord = {
          step_id: id,
          thread_id: event.thread_id,
          codex_turn_id: event.codex_turn_id,
          started_seq: event.seq,
          ended_seq: None,
          outcome: None,
          response_id: None,
          reason: None,
          inference_ids: [],
          emitted_tools: [],
          dispatched_tools: [],
        }
        steps[id] = step
        ordered_steps.push(step)
      }
      "finished" => {
        let step = require_step(steps, string_field(value, "step_id"))
        guard step.ended_seq is None else { fail("duplicate step finish") }
        let outcome = string_field(value, "outcome")
        guard ["accepted", "failed", "cancelled", "preempted", "abandoned"].contains(
          outcome,
        ) else {
          fail("unsupported step outcome: \{outcome}")
        }
        step.ended_seq = Some(event.seq)
        step.outcome = Some(outcome)
        step.response_id = nullable(field(value, "response_id"))
        step.reason = nullable(field(value, "reason"))
        if outcome == "accepted" && step.response_id is None {
          fail("accepted response requires an observed response id")
        }
      }
      "inference_linked" => {
        let link : InferenceLink = @json.from_json(value)
        guard !links.contains(link.inference_call_id) else {
          fail("duplicate inference association")
        }
        if link.step_id is String(id) {
          let step = require_step(steps, id)
          guard link.purpose == "task" else {
            fail("auxiliary inference cannot own a task step")
          }
          step.inference_ids.push(link.inference_call_id)
        } else if link.purpose == "task" {
          warnings.push("task inference has no logical step")
        }
        links[link.inference_call_id] = link
      }
      "tool_emitted" | "tool_dispatched" as kind => {
        let step = require_step(steps, string_field(value, "step_id"))
        let fact : Json = { "seq": event.seq.to_json(), "observation": value }
        if kind == "tool_emitted" {
          step.emitted_tools.push(fact)
        } else {
          step.dispatched_tools.push(fact)
        }
      }
      "http_scope" => {
        let scope : RequestScope = @json.from_json(value)
        guard !scopes.contains(scope.scope_id) else {
          fail("duplicate HTTP scope")
        }
        if scope.step_id is String(id) {
          ignore(require_step(steps, id))
        }
        scopes[scope.scope_id] = scope
      }
      "http_started" => {
        let id = string_field(value, "request_id")
        let scope_id = string_field(value, "scope_id")
        guard scopes.get(scope_id) is Some(scope) else {
          fail("unknown HTTP scope")
        }
        guard !requests.contains(id) else { fail("duplicate HTTP send") }
        let request : RequestRecord = {
          request_id: id,
          scope,
          attempt: @json.from_json(field(value, "attempt")),
          started_seq: event.seq,
          ended_seq: None,
          status_code: None,
          error: None,
        }
        requests[id] = request
        ordered_requests.push(request)
      }
      "http_finished" => {
        let id = string_field(value, "request_id")
        guard requests.get(id) is Some(request) else {
          fail("HTTP result without send")
        }
        guard request.ended_seq is None else { fail("duplicate HTTP result") }
        request.ended_seq = Some(event.seq)
        request.status_code = nullable(field(value, "status_code"))
        request.error = nullable(field(value, "error"))
      }
      kind => fail("unsupported step observation: \{kind}")
    }
  }
  let inferences = object_field(trace, "inference_calls")
  let tools = object_field(trace, "tool_calls")
  let mut associated = !events.is_empty()
  let mut http_covered = !events.is_empty()
  for id, _ in inferences {
    if !links.contains(id) {
      associated = false
    }
    if !scopes.values().any(scope => scope.inference_call_id == id.to_json()) {
      http_covered = false
    }
  }
  for id, _ in links {
    if !inferences.contains(id) {
      associated = false
    }
  }
  if field(trace, "compaction_requests") is Object(compactions) {
    for id, _ in compactions {
      if !scopes
        .values()
        .any(scope => scope.compaction_request_id == id.to_json()) {
        http_covered = false
      }
    }
  }
  let mut accepted = 0
  let mut emitted = 0
  let mut dispatched = 0
  let mut polls = 0
  let mut unfinished = 0
  for step in ordered_steps {
    if step.outcome == Some("accepted") {
      accepted += 1
    }
    if step.outcome is None || step.outcome == Some("abandoned") {
      unfinished += 1
    }
    emitted += step.emitted_tools.length()
    dispatched += step.dispatched_tools.length()
    for fact in step.emitted_tools {
      let tool_name = string_field(field(fact, "observation"), "tool_name")
      if tool_name == "write_stdin" || tool_name == "wait" {
        polls += 1
      }
    }
  }
  let mut tool_errors = 0
  let mut tool_results_covered = true
  for step in ordered_steps {
    for fact in step.dispatched_tools {
      if !tools.contains(
          string_field(field(fact, "observation"), "tool_call_id"),
        ) {
        tool_results_covered = false
      }
    }
  }
  let repairs : Array[Json] = []
  for id, tool in tools {
    let execution = field(tool, "execution")
    let status = field(execution, "status")
    if status != Json::string("completed") &&
      status != Json::string("failed") &&
      status != Json::string("cancelled") {
      tool_results_covered = false
    }
    if field(execution, "status") == Json::string("failed") {
      tool_errors += 1
      let ended_seq : Int? = nullable(field(execution, "ended_seq"))
      if ended_seq is Some(seq) {
        for step in ordered_steps {
          if step.started_seq > seq &&
            field(tool, "thread_id") == @json.to_json(step.thread_id) {
            repairs.push({
              "step_id": step.step_id.to_json(),
              "tool_call_id": id.to_json(),
              "source_seq": seq.to_json(),
              "confidence": "inferred",
              "reason": "First subsequent round after a tool failure; repair intent is unproven.",
            })
            break
          }
        }
      }
    }
  }
  let capture_status = field(trace, "status")
  if capture_status != Json::string("completed") &&
    capture_status != Json::string("failed") &&
    capture_status != Json::string("aborted") {
    warnings.push(
      "capture has no terminal rollout event; totals remain unknown",
    )
  }
  let intact = warnings.is_empty()
  let step_covered = associated && intact && unfinished == 0
  if events.is_empty() {
    warnings.push("logical-step instrumentation absent")
  }
  if !associated {
    warnings.push("inference-to-step/purpose coverage incomplete")
  }
  if !http_covered {
    warnings.push("HTTP coverage incomplete; WebSocket sends are not counted")
  }
  if !tool_results_covered {
    warnings.push("tool results incomplete; tool error total is unknown")
  }
  if unfinished > 0 {
    warnings.push(
      "unfinished sampling scopes; only observed prefix counts are available",
    )
  }
  let complete = step_covered && http_covered && tool_results_covered
  let count = fn(value : Int, covered : Bool) -> Json {
    if covered {
      value.to_json()
    } else {
      Json::null()
    }
  }
  let retries = ordered_requests.filter(request => request.attempt > 0).length()
  // A new sampling inference within the same logical decision is a stream
  // reconnect. HTTP transport attempts retain that inference identity.
  let mut stream_retries = 0
  for step in ordered_steps {
    stream_retries += Int::max(0, step.inference_ids.length() - 1)
  }
  let task_requests = ordered_requests
    .filter(request => request.scope.purpose == "task")
    .length()
  let compaction_requests = ordered_requests
    .filter(request => request.scope.purpose == "compaction")
    .length()
  {
    "schema_version": 1,
    "trace_id": field(trace, "trace_id"),
    "rollout_id": field(trace, "rollout_id"),
    "population": "Observed task loops in this capture, including failed and interrupted attempts.",
    "complete": complete.to_json(),
    "coverage": {
      "steps": step_covered.to_json(),
      "http_requests": (step_covered && http_covered).to_json(),
      "tool_results": (step_covered && tool_results_covered).to_json(),
    },
    "metrics": {
      "agent_steps_started": count(ordered_steps.length(), step_covered),
      "agent_steps": count(accepted, step_covered),
      "model_requests": count(
        ordered_requests.length(),
        http_covered && step_covered,
      ),
      "http_transport_retries": count(retries, http_covered && step_covered),
      "agent_stream_retries": count(stream_retries, step_covered),
      "tool_calls": count(emitted, step_covered),
      "tool_executions": count(dispatched, step_covered),
      "poll_or_input_calls": count(polls, step_covered),
      "tool_errors": count(tool_errors, step_covered && tool_results_covered),
      "repair_steps": Json::null(),
      "process_spawns": Json::null(),
      "task_success": Json::null(),
    },
    "observed": {
      "agent_steps_started": ordered_steps.length().to_json(),
      "agent_steps": accepted.to_json(),
      "model_requests": ordered_requests.length().to_json(),
      "http_transport_retries": retries.to_json(),
      "agent_stream_retries": stream_retries.to_json(),
      "task_requests": task_requests.to_json(),
      "compaction_requests": compaction_requests.to_json(),
      "tool_calls": emitted.to_json(),
      "tool_executions": dispatched.to_json(),
      "tool_errors": tool_errors.to_json(),
    },
    "steps": ordered_steps.to_json(),
    "requests": ordered_requests.to_json(),
    "tools": tools.to_json(),
    "inferences": inferences.to_json(),
    "possible_repairs": repairs.to_json(),
    "limitations": warnings.to_json(),
    "request_scope": "Native inference HTTP starts, including work canceled before upstream dispatch. Compare separately with the collector's observed upstream sends. Auxiliary endpoints and WebSocket sends have no total coverage claim.",
    "tool_error_scope": "Tool invocation failures only. A successfully returned exec_command can contain a nonzero child exit. Compiler, MBTX execution and Shell command failures are reported separately by attempt analysis.",
    "repair_attribution": "unknown; candidates are inferred, not confirmed repair steps",
  }
}

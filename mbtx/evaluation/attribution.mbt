///|
/// Separate retained observations from claims about why a task failed.
fn v3_attribution(
  facts : Json,
  tools : Json,
  visible : Json,
  status : String,
  external : Int,
) -> Json {
  let trace = field(facts, "trace")
  let outcome = field(facts, "outcome")
  let model_terminal = if field(outcome, "model_termination") == Json::null() {
    field(outcome, "termination")
  } else {
    field(outcome, "model_termination")
  }
  let workers = field(field(facts, "snapshot"), "worker_events")
  let worker_errors = match workers {
    Array(events) =>
      events.filter(e => field(e, "evidence_error") != Json::null()).length()
    _ => 0
  }
  {
    "sandbox/launcher": {
      "confidence": "unknown",
      "cause": Json::null(),
      "reason": "Task failure alone does not diagnose sandbox entry; inspect process and raw stderr evidence.",
    },
    "worker IPC": {
      "confidence": if workers == Json::null() {
        "unknown"
      } else {
        "observed"
      },
      "evidence_errors": worker_errors.to_json(),
      "events": worker_statistics(
        match workers {
          Array(events) => events
          _ => []
        },
      ),
      "cause": Json::null(),
    },
    "relay/provider": {
      "confidence": "observed",
      "external_failures": external.to_json(),
      "cause": Json::null(),
      "reason": "Client-side responses cannot distinguish relay internals from provider internals.",
    },
    "Codex scheduling": {
      "confidence": if trace == Json::null() {
        "unknown"
      } else {
        "observed"
      },
      "session_terminal": field(trace, "status"),
      "model_process_terminal": model_terminal,
      "cause": Json::null(),
      "reason": "Terminal state is observed; a scheduling defect is not inferred from unsuccessful task completion.",
    },
    "MBTX compile/runtime": {
      "confidence": if field(tools, "mbtx_result_coverage") == true.to_json() {
        "observed"
      } else {
        "unknown"
      },
      "compile_failures": field(tools, "mbtx_compile_failures"),
      "runtime_failures": field(tools, "mbtx_execution_failures"),
      "denial_diagnostics": field(tools, "process_denial_diagnostics_observed"),
      "cause": Json::null(),
    },
    "fixture/oracle": {
      "confidence": field(visible, "confidence"),
      "visible": visible,
      "submission_status": field(field(facts, "submission"), "status"),
      "cause": Json::null(),
      "reason": "Oracle mismatch establishes a contract failure, not whether the model, fixture, or oracle caused it.",
    },
    "harness": {
      "confidence": if status == "harness_error" {
        "inferred"
      } else {
        "unknown"
      },
      "integrity": field(facts, "integrity"),
      "execution_error": field(facts, "execution_error"),
      "reason": "Attribution follows retained adapter/capture errors; raw evidence is authoritative.",
    },
    "timeout": {
      "confidence": if outcome == Json::null() {
        "unknown"
      } else {
        "observed"
      },
      "deadline_reached": (field(outcome, "termination") ==
      Json::string("wall_limit") ||
      field(outcome, "termination") == Json::string("timeout")).to_json(),
      "validation_phase": field(field(facts, "submission"), "timeout_phase"),
    },
    "censored": {
      "confidence": "inferred",
      "value": (status == "censored").to_json(),
      "reason": "A missing terminal outcome or reached deadline leaves completion steps unknown.",
    },
  }
}

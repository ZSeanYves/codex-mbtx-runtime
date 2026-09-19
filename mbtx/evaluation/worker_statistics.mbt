///|
/// Receipt totals describe registered workers only. Utilities and all native
/// child processes are not covered by this observation channel.
fn worker_statistics(records : Array[Json]) -> Json {
  let actions : Map[String, Json] = Map([])
  let names : Array[String] = []
  let trusted = records.filter(r => {
    field(r, "confidence") == Json::string("observed") &&
    field(r, "evidence_error") == Json::null()
  })
  for r in trusted {
    if field(field(r, "event"), "action") is String(action) &&
      !names.contains(action) {
      names.push(action)
    }
  }
  names.sort()
  for name in names {
    let events = trusted.filter(r => {
      field(field(r, "event"), "action") == name.to_json()
    })
    let completed = events.filter(r => {
      field(field(r, "event"), "phase") == Json::string("completed")
    })
    let codes : Map[String, Int] = Map([])
    for r in completed {
      let code = field(field(r, "event"), "exit_code")
      if code != Json::null() {
        let key = code.stringify()
        codes[key] = codes.get(key).unwrap_or(0) + 1
      }
    }
    actions[name] = {
      "starts_observed": events
      .filter(r => field(field(r, "event"), "phase") == Json::string("started"))
      .length()
      .to_json(),
      "completions_observed": completed.length().to_json(),
      "operations_observed": events
      .filter(r => {
        field(field(r, "event"), "phase") == Json::string("operation")
      })
      .length()
      .to_json(),
      "exit_codes_observed": codes.to_json(),
    }
  }
  {
    "confidence": "observed",
    "source": "worker-events.jsonl",
    "by_action": actions.to_json(),
    "receipt_records_observed": trusted.length().to_json(),
    "observation_errors": records
    .filter(r => field(r, "evidence_error") != Json::null())
    .length()
    .to_json(),
    "all_child_process_count": Json::null(),
    "scope": "Observed registered fixture-worker receipts only; utilities and total child-process counts remain unknown. Job/recover operation events identify validated state operations; started/completed also include help and argument diagnostics. Read-only hash/emit do not emit operation. Exit 7 alone does not establish a retry.",
  }
}

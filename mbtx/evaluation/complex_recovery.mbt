///|
fn recovery_id(i : Int, variant : Int, epoch : Int, tier : Int) -> String {
  if tier == 2 && epoch == 2 && i == 3 {
    "r999"
  } else {
    "r\{100 + (i * 7 + variant) % 97}"
  }
}

///|
fn recovery_case(tier : Int, variant : Int, epoch : Int) -> Json {
  let n = [12, 24, 48][tier]
  let shard_count = [2, 4, 8][tier]
  let job_count = [0, 18, 36][tier]
  let layers = [0, 5, 10][tier]
  let files : Map[String, Json] = {
    "state/preserve.txt": "unrelated state; preserve\r\n",
  }
  let previous = if epoch > 0 {
    recovery_case(tier, variant, epoch - 1)
  } else {
    Json::null()
  }
  if epoch > 0 {
    for path, value in v4_fields(previous, "expected_outputs") {
      files[path] = (match value {
        String(s) => s
        _ => value.stringify()
      }).to_json()
    }
  }
  let shards : Array[Array[Json]] = Array::makei(shard_count, _ => [])
  let aliases : Map[String, Json] = Map([])
  let records : Array[Json] = []
  let states : Array[Json] = []
  let original_ids : Array[String] = []
  let transactions : Array[Json] = []
  if tier > 0 {
    transactions.push({
      "id": "committed",
      "status": "committed",
      "members": ["event-0", "event-1"],
    })
    transactions.push({
      "id": "aborted",
      "status": "aborted",
      "members": ["aborted-7"],
    })
    transactions.push({
      "id": "incomplete",
      "status": "committed",
      "members": ["incomplete-6", "missing-member"],
    })
  }
  let current_wins = variant % 2 == 0
  for i in 0..<n {
    let id = recovery_id(i, variant, epoch, tier)
    let original = recovery_id(i, variant, 0, tier)
    original_ids.push(original)
    let key = if tier == 2 && i % 3 == 0 { "legacy-\{i}" } else { id }
    if key != id {
      aliases[key] = id.to_json()
    }
    let generation = 2 + (if epoch > 0 && i == 0 { epoch } else { 0 })
    let value = 2 +
      (variant + i) % 13 +
      (if epoch > 0 && i == 0 { epoch } else { 0 })
    let deleted = (tier > 0 && i % 11 == 10) || (epoch > 0 && i == 2)
    let transaction = if tier > 0 && i < 2 {
      "committed".to_json()
    } else {
      Json::null()
    }
    let event : Json = if tier == 2 && i % 2 == 0 {
      {
        "schema": 1,
        "event_id": "event-\{i}".to_json(),
        "key": key.to_json(),
        "revision": generation.to_json(),
        "amount": value.to_json(),
        "source": "current",
        "op": "upsert",
        "tx": transaction,
      }
    } else {
      {
        "schema": 2,
        "event_id": "event-\{i}".to_json(),
        "id": key.to_json(),
        "generation": generation.to_json(),
        "value": value.to_json(),
        "source": "current",
        "op": "upsert",
        "tx": transaction,
      }
    }
    shards[i % shard_count].push(event)
    if i % 4 == 0 {
      shards[(i + 1) % shard_count].push(event)
    }
    shards[(i + 1) % shard_count].push({
      "schema": 2,
      "event_id": "old-\{i}".to_json(),
      "id": key.to_json(),
      "generation": 1,
      "value": 99,
      "op": "upsert",
      "source": "mirror",
      "tx": Json::null(),
    })
    if tier > 0 && i % 7 == 0 {
      shards[(i + 2) % shard_count].push({
        "schema": 2,
        "event_id": "mirror-\{i}".to_json(),
        "id": key.to_json(),
        "generation": generation.to_json(),
        "value": (value + 1).to_json(),
        "op": "upsert",
        "source": "mirror",
        "tx": Json::null(),
      })
    }
    if tier > 0 && (i == 6 || i == 7) {
      shards[(i + 1) % shard_count].push({
        "schema": 2,
        "event_id": (if i == 6 { "incomplete-6" } else { "aborted-7" }).to_json(),
        "id": key.to_json(),
        "generation": 999,
        "value": 999,
        "op": "upsert",
        "source": "current",
        "tx": (if i == 6 { "incomplete" } else { "aborted" }).to_json(),
      })
    }
    if deleted {
      shards[(i + 2) % shard_count].push({
        "schema": 2,
        "event_id": "deleted-\{i}".to_json(),
        "id": key.to_json(),
        "generation": 100,
        "value": Json::null(),
        "op": "delete",
        "source": "current",
        "tx": Json::null(),
      })
    } else {
      let value = value +
        (if tier > 0 && i % 7 == 0 && !current_wins { 1 } else { 0 })
      records.push({
        "id": id.to_json(),
        "generation": generation.to_json(),
        "value": value.to_json(),
      })
      states.push({
        "id": id.to_json(),
        "generation": generation.to_json(),
        "result": (value * value).to_json(),
      })
    }
  }
  records.sort_by((a, b) => {
    codepoint_compare(v4_text(a, "id"), v4_text(b, "id"))
  })
  states.sort_by((a, b) => codepoint_compare(v4_text(a, "id"), v4_text(b, "id")))
  let record_states = states.copy()
  let nodes : Array[Json] = []
  let jobs : Array[Json] = []
  let job_states : Array[Json] = []
  for i in 0..<job_count {
    let level = i * layers / job_count
    let deps : Array[String] = []
    for j in 0..<i {
      if j * layers / job_count == level - 1 &&
        (deps.is_empty() || (j + variant) % 2 == i % 2) {
        deps.push(v4_text(jobs[j], "id"))
      }
    }
    let id = "j\{100+(i*11+variant)%97}"
    let record = record_states[i * 3 % record_states.length()]
    let mut generation : Int = @json.from_json(field(record, "generation")) catch {
      _ => 0
    }
    generation += 1
    let base : Int = @json.from_json(field(record, "result")) catch { _ => 0 }
    let value = base + i + 1
    let mut result = value
    for dep in deps {
      let state = job_states
        .iter()
        .find_first(s => field(s, "id") == dep.to_json())
        .unwrap()
      generation += @json.from_json(field(state, "generation")) catch { _ => 0 }
      result += @json.from_json(field(state, "result")) catch { _ => 0 }
    }
    let retry = i % 8 == 3
    nodes.push({
      "id": id.to_json(),
      "record": field(record, "id"),
      "revision": 1,
      "bias": (i + 1).to_json(),
      "deps": deps.to_json(),
      "enabled": true,
      "fail_once": retry.to_json(),
    })
    jobs.push({
      "id": id.to_json(),
      "value": value.to_json(),
      "generation": generation.to_json(),
      "deps": deps.to_json(),
      "enabled": true,
      "fail_once": retry.to_json(),
    })
    let state : Json = {
      "id": id.to_json(),
      "generation": generation.to_json(),
      "result": result.to_json(),
    }
    job_states.push(state)
    states.push(state)
  }
  nodes.rev_in_place()
  let shard_names = Array::makei(shard_count, i => "events/part-\{i} 雪.jsonl")
  for i, events in shards {
    let suffix = if tier == 2 { "{damaged-tail\r\n" } else { "" }
    files[shard_names[i]] = (events
    .map(e => e.stringify())
    .join(if i % 2 == 0 { "\r\n" } else { "\n" }) +
    "\n" +
    suffix).to_json()
  }
  files["workflow.json"] = (
      {
        "epoch": epoch.to_json(),
        "shards": shard_names.to_json(),
        "transactions": transactions.to_json(),
        "aliases": aliases.to_json(),
        "priorities": if current_wins {
          { "current": 2, "mirror": 1 }
        } else {
          { "current": 1, "mirror": 2 }
        },
        "jobs": nodes.to_json(),
      } : Json)
    .stringify()
    .to_json()
  if epoch == 0 {
    files["checkpoint.json"] = "{\"epoch\":999,\"completed\":[\"invented\"],\"generations\":{}}"
    files["state/interrupted.pending"] = "unfinished write"
    for i, state in states {
      let path = "state/" + v4_text(state, "id") + ".json"
      if (i + variant) % 6 == 0 {
        files[path] = state.stringify().to_json()
      } else if (i + variant) % 6 == 1 {
        files[path] = "{broken"
      } else if (i + variant) % 6 == 2 {
        files[path] = "{\"generation\":0,\"result\":0}"
      }
    }
  }
  let outputs : Map[String, Json] = Map([])
  let events : Array[Json] = []
  let executed : Array[String] = []
  let initial : Array[String] = []
  let initial_retries : Array[String] = []
  let mutable : Array[String] = [
    "checkpoint.json", "records-selected.json", "jobs-selected.json",
  ]
  let absent : Array[String] = ["state/interrupted.pending"]
  let mut total = 0
  for state in states {
    let id = v4_text(state, "id")
    let path = "state/" + id + ".json"
    outputs[path] = state
    mutable.push(path)
    total += @json.from_json(field(state, "result")) catch { _ => 0 }
    let before = match files.get(path) {
      Some(String(s)) => @json.parse(s) catch { _ => Json::null() }
      _ => Json::null()
    }
    let job = jobs.iter().find_first(j => field(j, "id") == id.to_json())
    if files.get("state/" + id + ".retry") == Some(Json::string("retryable\n")) {
      initial_retries.push(id)
    }
    if before == state {
      initial.push(id)
    } else {
      executed.push(id)
      let action = if job is Some(_) { "job" } else { "recover" }
      if job is Some(job) &&
        field(job, "fail_once") == true.to_json() &&
        !files.contains("state/" + id + ".retry") {
        events.push({ "action": "job", "id": id.to_json(), "exit_code": 7 })
      }
      events.push({
        "action": action.to_json(),
        "id": id.to_json(),
        "exit_code": 0,
      })
    }
    if job is Some(job) &&
      field(job, "fail_once") == true.to_json() &&
      before != state {
      outputs["state/" + id + ".retry"] = "retryable\n"
    }
  }
  for path, _ in files {
    if path.has_prefix("state/") &&
      (path.has_suffix(".json") || path.has_suffix(".pending")) &&
      !outputs.contains(path) {
      absent.push(path)
      mutable.push(path)
    }
  }
  for id in original_ids {
    if !states.any(s => field(s, "id") == id.to_json()) {
      absent.push("state/" + id + ".json")
    }
  }
  executed.sort_by(codepoint_compare)
  job_states.sort_by((a, b) => {
    codepoint_compare(v4_text(a, "id"), v4_text(b, "id"))
  })
  let ids = states.map(s => v4_text(s, "id"))
  ids.sort_by(codepoint_compare)
  let generations : Map[String, Json] = Map([])
  for s in states {
    generations[v4_text(s, "id")] = field(s, "generation")
  }
  outputs["checkpoint.json"] = {
    "epoch": epoch.to_json(),
    "completed": ids.to_json(),
    "generations": generations.to_json(),
  }
  outputs["records-selected.json"] = { "records": records.to_json() }
  // Topological ties may be resolved differently; job-plan arrays are checked
  // by the worker dependency oracle rather than exact reference array order.
  let expected : Json = {
    "epoch": epoch.to_json(),
    "records": record_states.to_json(),
    "jobs": job_states.to_json(),
    "executed": executed.to_json(),
    "total": total.to_json(),
  }
  {
    "files": files.to_json(),
    "expected": expected,
    "expected_outputs": outputs.to_json(),
    "mutable_inputs": mutable.to_json(),
    "absent_outputs": absent.to_json(),
    "worker_events": events.to_json(),
    "initial_completed": initial.to_json(),
    "output": "result.json",
    "worker_plan_path": "jobs-selected.json",
    "expected_worker_plan": { "jobs": jobs.to_json() },
    "initial_retries": initial_retries.to_json(),
    "strict_worker_receipts": true,
    "operation_receipts": true,
    "exact_worker_actions": ["recover", "job"],
    "actions": ["recover", "job"],
    "goal": "Recover the event pipeline described by workflow.json. Read every listed JSONL shard; skip malformed lines, deduplicate identical event_id, and accept transaction events only for committed transactions whose complete member IDs exist. Normalize schema 1 key/revision/amount to schema 2 id/generation/value, resolve aliases, reject invalid numeric fields, then select greatest generation, greatest configured source priority, and lexical event_id per canonical ID. The selected delete removes that record. Save records-selected.json with active {id,generation,value} records sorted by Unicode code point. Expected recovered state is {id,generation,result:value squared}; reuse only byte-parsed JSON states equal to this value, otherwise call fixture-worker recover records-selected.json ID. Never manufacture worker states. For each enabled job whose record is active, resolve the DAG: value=record result+bias; generation=record generation+revision+sum dependency generations; worker result=value+sum dependency results. Save jobs-selected.json with these computed fields plus deps, enabled, fail_once. Reuse only equal states; execute remaining jobs dependency-first using fixture-worker job jobs-selected.json ID. Retry only after exit 7, once for that operation. All required record recoveries must precede jobs. A checkpoint is a hint, not proof. Remove retired state JSON and pending files, preserve unrelated files, then write checkpoint.json {epoch,completed:all sorted active IDs,generations:{id:generation}}. result.json contains actual committed states and executed IDs for this invocation, sorted by ID. The program will be run again unchanged: no state-changing worker operations are allowed. Deep inputs additionally advance through three event epochs, retaining prior state; revalidate and invalidate only changed records and their dependent jobs.",
    "output_schema": "{epoch:integer,records:[{id,generation,result}],jobs:[{id,generation,result}],executed:string[],total:integer}",
    "reference_shell": recovery_reference_shell().to_json(),
    "reference_mbtx": recovery_reference_mbtx().to_json(),
  }
}

///|
fn v4_recovery(tier : Int, variant : Int) -> Json {
  let base = recovery_case(tier, variant, 0)
  let object = match base {
    Object(o) => o.copy()
    _ => abort("case")
  }
  let phases : Array[Json] = []
  for epoch in 0..<(if tier == 2 { 3 } else { 1 }) {
    let current = recovery_case(tier, variant, epoch)
    let repeated = v4_with_repeat(current)
    let pair : Array[Json] = @json.from_json(
      field(repeated, "validation_phases"),
    ) catch {
      _ => []
    }
    for index, phase in pair {
      let p = match phase {
        Object(o) => o.copy()
        _ => abort("phase")
      }
      if epoch > 0 && index == 0 {
        let updates : Map[String, Json] = Map([])
        for path, value in v4_fields(current, "files") {
          if path == "workflow.json" || path.has_prefix("events/") {
            updates[path] = value
          }
        }
        p["input_updates"] = updates.to_json()
      }
      phases.push(Json::object(p))
    }
  }
  object["validation_phases"] = phases.to_json()
  Json::object(object)
}

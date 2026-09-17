///|
fn long_jobs(extended : Bool, variant : Int) -> LongCase {
  let n = if extended { 6 } else { 4 }
  let jobs : Array[Json] = []
  let completed : Array[Json] = []
  let results : Array[Int] = []
  let events : Array[Json] = []
  let outputs : Map[String, Json] = Map([])
  let mut total = 0
  for i in 0..<n {
    let id = "job-\{i}"
    let deps : Array[String] = if i == 0 { [] } else if extended && i > 1 { ["job-\{i - 1}","job-0"] } else { ["job-\{i - 1}"] }
    let value = variant + i + 1
    let result = value + (if i == 0 { 0 } else if extended && i > 1 { results[i - 1] + results[0] } else { results[i - 1] })
    results.push(result)
    total += result
    let retry = i == 1 || (extended && i == 3)
    jobs.push({"id":id.to_json(),"value":value.to_json(),"generation":variant.to_json(),"deps":deps.to_json(),"enabled":true,"fail_once":retry.to_json()})
    completed.push({"id":id.to_json(),"result":result.to_json()})
    outputs["state/" + id + ".json"] = {"id":id.to_json(),"result":result.to_json(),"generation":variant.to_json()}
    if retry { events.push({"action":"job","id":id.to_json(),"exit_code":7}) }
    events.push({"action":"job","id":id.to_json(),"exit_code":0})
  }
  jobs.rev_in_place()
  jobs.push({"id":"job-disabled","value":999,"generation":variant.to_json(),"deps":[],"enabled":false,"fail_once":false})
  let base = long_plain({
    goal: "Execute enabled jobs from jobs.json with ./fixture-worker job jobs.json ID. Input order is not execution order. Honor dependencies; a worker requires committed state/DEPENDENCY.json files and computes its own result. Code 7 is a controlled retryable failure; retry that job only after observing the failure. Other failures require diagnosis. Do not execute disabled jobs or manufacture worker state files. After all enabled work completes, read committed state files, sort completed jobs by id, sum results, and list disabled IDs as skipped. Write result.json and verify every dependency has a committed result. Worker invocations are independently recorded.",
    schema: "{completed:[{id:string,result:integer}],skipped:string[],total:integer}",
    files: {"jobs.json":({"jobs":jobs.to_json()}:Json).stringify().to_json()},
    expected: {"completed":completed.to_json(),"skipped":["job-disabled"],"total":total.to_json()},
    reference: (#|jq -r '.jobs|sort_by(.id)[]|select(.enabled)|.id' jobs.json > order.txt
               #|while IFS= read -r id; do if ./fixture-worker job jobs.json "$id"; then :; else code=$?; test "$code" -eq 7; ./fixture-worker job jobs.json "$id"; fi; done < order.txt
               #|jq -s --slurpfile plan jobs.json '{completed:(sort_by(.id)|map({id,result})),skipped:([$plan[0].jobs[]|select(.enabled|not)|.id]|sort),total:([.[].result]|add)}' state/*.json > result.json
               #|
    ),
  })
  { ..base, outputs, worker_events: events, absent: ["state/job-disabled.json"] }
}

///|
fn long_recovery(extended : Bool, variant : Int) -> LongCase {
  let n = if extended { 6 } else { 4 }
  let records : Array[Json] = []
  let outputs : Map[String, Json] = Map([])
  let files : Map[String, Json] = Map([])
  let events : Array[Json] = []
  let ids : Array[String] = []
  let completed : Array[Json] = []
  let mut total = 0
  for i in 0..<n {
    let id = "record-\{i}"
    let value = variant + i + 2
    let generation = variant + 2
    ids.push(id)
    records.push({"id":id.to_json(),"value":value.to_json(),"generation":generation.to_json()})
    let state : Json = {"id":id.to_json(),"result":(value * value).to_json(),"generation":generation.to_json()}
    outputs["state/" + id + ".json"] = state
    completed.push(state)
    total += value * value
    if i == 0 { files["state/" + id + ".json"] = state.stringify().to_json() }
    else { events.push({"action":"recover","id":id.to_json(),"exit_code":0}) }
    if i == 1 { files["state/" + id + ".json"] = ({"id":id.to_json(),"result":0,"generation":(generation - 1).to_json()}:Json).stringify().to_json() }
    if extended && i % 2 == 0 { records.push({"id":id.to_json(),"value":-1,"generation":(generation - 1).to_json()}) }
  }
  records.rev_in_place()
  files["records.json"] = ({"records":records.to_json()}:Json).stringify().to_json()
  files["checkpoint.json"] = ({"completed":ids.to_json(),"generation":0}:Json).stringify().to_json()
  files["state/record-2.pending"] = "incomplete write 雪".to_json()
  let checkpoint : Json = {"completed":ids.to_json(),"generation":(variant + 2).to_json()}
  outputs["checkpoint.json"] = checkpoint
  let mutable = outputs.keys().to_array()
  mutable.push("state/record-2.pending")
  let base = long_plain({
    goal: "Recover records.json using committed state and checkpoint.json. Deduplicate by id, selecting greatest generation (duplicates at a generation are identical). A committed state is valid only when id, generation and squared result match the selected record. The checkpoint is only a hint and may overclaim completion. Keep valid committed state without re-executing its record. For every missing or invalid state, invoke ./fixture-worker recover records.json ID, check success, then verify its committed JSON. The worker selects the latest generation and squares value. Remove stale *.pending files. Rebuild checkpoint.json with sorted completed IDs and the maximum selected generation. Write result.json with all sorted states, total result and IDs that required execution. Do not edit records.json or fabricate worker output. Verify that a second recovery pass would schedule no work; do not re-execute already valid records. Worker invocations are independently recorded.",
    schema: "{completed:[{id:string,generation:integer,result:integer}],total:integer,executed:string[]}", files,
    expected: {"completed":completed.to_json(),"total":total.to_json(),"executed":ids[1:].to_owned().to_json()},
    reference: (#|jq '.records|sort_by(.id,.generation)|group_by(.id)|map(last)' records.json > selected.json
               #|: > executed.txt
               #|jq -r '.[].id' selected.json | while IFS= read -r id; do
               #| expected=$(jq -c --arg id "$id" '.[]|select(.id==$id)|{id,generation,result:(.value*.value)}' selected.json)
               #| if test -f "state/$id.json" && jq -e --argjson expected "$expected" '.==$expected' "state/$id.json" >/dev/null; then :; else ./fixture-worker recover records.json "$id"; printf '%s\n' "$id" >> executed.txt; fi
               #|done
               #|rm -f state/*.pending
               #|jq '{completed:map(.id),generation:(map(.generation)|max)}' selected.json > checkpoint.json
               #|jq -Rn '[inputs]' executed.txt > executed.json
               #|jq -s --slurpfile e executed.json '{completed:sort_by(.id),total:([.[].result]|add),executed:$e[0]}' state/*.json > result.json
               #|
    ),
  })
  { ..base, outputs, mutable_inputs: mutable, absent: ["state/record-2.pending"], worker_events: events }
}

///|
fn long_forensics(extended : Bool, variant : Int) -> LongCase {
  let files : Map[String, Json] = {"streams.json":"{\"stdout\":\"stdout.input\",\"stderr\":\"stderr.input\"}"}
  let outputs : Map[String, Json] = Map([])
  let summaries : Array[Json] = []
  for stream in ["stdout","stderr"] {
    let n = (if extended { 3600 } else { 1200 }) + variant % 7
    let lines : Array[Json] = []
    let errors = if extended { [17,n - 2] } else { [17] }
    for i in 0..<n {
      lines.push({"seq":i.to_json(),"level":(if errors.contains(i) { "ERROR" } else { "INFO" }).to_json(),"message":(if i == n - 1 { "tail 雪 λ" } else { "event \{variant}/\{i} \{stream}" }).to_json()})
    }
    let text = lines.map(v=>v.stringify()).join("\n") + "\n"
    files[stream + ".input"] = text.to_json()
    outputs[stream + ".log"] = text.to_json()
    summaries.push({"stream":stream.to_json(),"records":n.to_json(),"tail":lines[n - 1],"findings":errors.map(i=>({"seq":i.to_json(),"context":[lines[i - 1],lines[i],lines[i + 1]]}:Json)).to_json()})
  }
  let base = long_plain({
    goal: "Run ./fixture-worker emit streams.json and preserve its complete separate stdout/stderr bytes in stdout.log and stderr.log. Large previews are insufficient evidence of completeness. Parse the emitted JSONL records. For each ERROR, retain exactly its preceding/current/following record in that stream, preserving per-stream order. Report record counts, full last records (including Unicode), and findings. Verify the saved logs exactly match their corresponding immutable .input files, including trailing newlines. Do not merge streams or sort away their order. Write result.json with stdout first, then stderr. The emitting worker must actually run and is independently observed.",
    schema: "{streams:[{stream:string,records:integer,tail:object,findings:[{seq:integer,context:object[]}]}]}", files,
    expected: {"streams":summaries.to_json()},
    reference: (#|./fixture-worker emit streams.json > stdout.log 2> stderr.log
               #|cmp stdout.input stdout.log
               #|cmp stderr.input stderr.log
               #|jq -s --arg stream stdout '. as $rows | {stream:$stream,records:length,tail:last,findings:[to_entries[]|select(.value.level=="ERROR")|.key as $i|{seq:.value.seq,context:$rows[$i-1:$i+2]}]}' stdout.log > stdout-summary.json
               #|jq -s --arg stream stderr '. as $rows | {stream:$stream,records:length,tail:last,findings:[to_entries[]|select(.value.level=="ERROR")|.key as $i|{seq:.value.seq,context:$rows[$i-1:$i+2]}]}' stderr.log > stderr-summary.json
               #|jq -s '{streams:.}' stdout-summary.json stderr-summary.json > result.json
               #|
    ),
  })
  { ..base, outputs, worker_events: [{"action":"emit","id":Json::null(),"exit_code":0}] }
}

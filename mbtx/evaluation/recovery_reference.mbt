///|
/// Private, input-driven reference algorithms. Never inserted into task prompts.
fn recovery_selection_filter() -> String {
  let filter =
    #| .config as $c | (.events | map(select(type=="object")) | unique_by(.event_id)) as $events |
    #| ($events | map(select(.tx == null or (.tx as $t | any($c.transactions[]; .id==$t and .status=="committed" and all(.members[]; . as $m | any($events[]; .event_id==$m)))))) |
    #| map(if .schema==1 then {event_id,id:.key,generation:.revision,value:.amount,op,source,tx} else . end) |
    #| map(.id=($c.aliases[.id] // .id)) |
    #| map(select((.id|type)=="string" and (.generation|type)=="number" and (.generation|floor)==.generation and ((.op=="delete") or ((.value|type)=="number" and (.value|floor)==.value)))) |
    #| sort_by(.id,.generation,($c.priorities[.source] // -1),.event_id) | group_by(.id) | map(last) | map(select(.op!="delete")) | sort_by(.id) | map({id,generation,value})) as $records |
    #| ($records | map({id,generation,result:(.value*.value)})) as $rs |
    #| ($c.jobs | map(select(.enabled and (.record as $r | any($rs[];.id==$r))))) as $nodes |
    #| (reduce range(0;($nodes|length)) as $iteration ({jobs:[],states:[]}; . as $done |
    #| ([$nodes[] | select(.id as $id | all($done.jobs[];.id!=$id)) | select(all(.deps[]; . as $id | any($done.jobs[];.id==$id)))] | sort_by(.id) | first) as $node |
    #| if $node==null then error("invalid enabled dependency graph") else
    #| ($rs[]|select(.id==$node.record)) as $record |
    #| {id:$node.id,value:($record.result+$node.bias),generation:($record.generation+$node.revision+([$done.states[]|select(.id as $id | $node.deps|index($id))|.generation]|add // 0)),deps:$node.deps,enabled:true,fail_once:$node.fail_once} as $job |
    #| .jobs += [$job] | .states += [{id:$job.id,generation:$job.generation,result:($job.value+([$done.states[]|select(.id as $id | $job.deps|index($id))|.result]|add // 0))}] end)) as $graph |
    #| {records:$records,jobs:$graph.jobs,states:($rs+$graph.states),record_states:$rs,job_states:($graph.states|sort_by(.id)),epoch:$c.epoch}
  filter
}

///|
fn recovery_reference_mbtx() -> String {
  let prefix =
    #|import { "moonbitlang/async@0.21.3", "moonbitlang/async@0.21.3/fs", "moonbitlang/async@0.21.3/shell", "moonbitlang/core/json" }
    #|async fn query(filter : String, input : Json) -> Json {
    #| let child=@shell.Cmd("jq",["-c",filter],stdin=@shell.Text(input.stringify())).output(max_output_bytes=1000000)
    #| assert_eq(child.exit_code(),0)
    #| @json.parse(child.stdout())
    #|}
    #|async fn main {
    #| let config=@json.parse(@fs.read_file("workflow.json").text())
    #| guard config is {"shards":Array(shards),..} else {fail("shards")}
    #| let events : Array[Json]=[]
    #| for file in shards { guard file is String(file) else {fail("shard name")}; for line in @fs.read_file(file).text().split("\n") { try { events.push(@json.parse(line.to_owned())) } catch { _=>() } } }
  let body =
    #| guard plan is {"records":Array(records),"jobs":Array(jobs),"states":Array(states),..} else {fail("plan")}
    #| let rp : Json={"records":records.to_json()}; let jp : Json={"jobs":jobs.to_json()}
    #| @fs.write_file("records-selected.json",rp.stringify()); @fs.write_file("jobs-selected.json",jp.stringify())
    #| let executed : Array[String]=[]
    #| for group in [records,jobs] {
    #|   for row in group {
    #|     guard row is {"id":String(id),..} else {fail("id")}
    #|     let expected=states.iter().find_first(s=>match s {{"id":String(i),..}=>i==id;_=>false}).unwrap()
    #|     let path="state/"+id+".json"
    #|     let valid=try {@json.parse(@fs.read_file(path).text())==expected} catch {_=>false}
    #|     if !valid {
    #|       let job=match row {{"deps":Array(_),..}=>true;_=>false}
    #|       let args=[if job {"job"} else {"recover"},if job {"jobs-selected.json"} else {"records-selected.json"},id]
    #|       let child=@shell.Cmd("fixture-worker",args).output()
    #|       if child.exit_code()==7 && job { assert_eq(@shell.Cmd("fixture-worker",args).output().exit_code(),0) } else {assert_eq(child.exit_code(),0)}
    #|       executed.push(id)
    #|     }
    #|     assert_eq(@json.parse(@fs.read_file(path).text()),expected)
    #|   }
    #| }
    #| if !@fs.exists("state") {@fs.mkdir("state")}
    #| for name in @fs.readdir("state") {
    #|   let id=name.replace(old=".json",new="")
    #|   if name.has_suffix(".pending") || (name.has_suffix(".json") && !states.any(s=>match s {{"id":String(i),..}=>i==id;_=>false})) {@fs.remove("state/"+name)}
    #| }
    #| let checkpoint=query("{epoch,completed:([.states[].id]|sort),generations:(reduce .states[] as $s ({};.[$s.id]=$s.generation))}",plan)
    #| @fs.write_file("checkpoint.json",checkpoint.stringify())
    #| let output=query("{epoch:.plan.epoch,records:.plan.record_states,jobs:.plan.job_states,executed:(.executed|sort),total:([.plan.states[].result]|add // 0)}",{"plan":plan,"executed":executed.to_json()})
    #| @fs.write_file("result.json",output.stringify())
    #|}
  prefix +
  "\n let plan=query(" +
  recovery_selection_filter().to_json().stringify() +
  ",{\"config\":config,\"events\":events.to_json()})\n" +
  body
}

///|
fn recovery_reference_shell() -> String {
  let preparation =
    #|set -eu
    #|: > events-all.jsonl
    #|jq -r '.shards[]' workflow.json > shard-list.txt
    #|while IFS= read -r path; do cat "$path" >> events-all.jsonl; printf '\n' >> events-all.jsonl; done < shard-list.txt
    #|jq -Rn '[inputs | try fromjson catch null]' events-all.jsonl > parsed-events.json
  let execution =
    #|jq '{records}' plan.json > records-selected.json
    #|jq '{jobs}' plan.json > jobs-selected.json
    #|: > executed.txt
    #|jq -r '.records[].id,.jobs[].id' plan.json > execution-order.txt
    #|while IFS= read -r id; do
    #| expected=$(jq -c --arg id "$id" '.states[]|select(.id==$id)' plan.json)
    #| if ! test -f "state/$id.json" || ! jq -e --argjson expected "$expected" '.==$expected' "state/$id.json" >/dev/null 2>&1; then
    #|   if jq -e --arg id "$id" 'any(.jobs[];.id==$id)' plan.json >/dev/null; then
    #|     if fixture-worker job jobs-selected.json "$id"; then :; else code=$?; test "$code" -eq 7; fixture-worker job jobs-selected.json "$id"; fi
    #|   else fixture-worker recover records-selected.json "$id"; fi
    #|   printf '%s\n' "$id" >> executed.txt
    #| fi
    #| jq -e --argjson expected "$expected" '.==$expected' "state/$id.json" >/dev/null
    #|done < execution-order.txt
    #|mkdir -p state
    #|for path in state/*.pending state/*.json; do
    #| test -f "$path" || continue
    #| case "$path" in *.pending) rm "$path";; *.json) id=${path#state/}; id=${id%.json}; if ! jq -e --arg id "$id" 'any(.states[];.id==$id)' plan.json >/dev/null; then rm "$path"; fi;; esac
    #|done
    #|jq '{epoch,completed:([.states[].id]|sort),generations:(reduce .states[] as $s ({};.[$s.id]=$s.generation))}' plan.json > checkpoint.json
    #|jq -Rn '[inputs]' executed.txt > executed.json
    #|jq -n --slurpfile p plan.json --slurpfile e executed.json '{epoch:$p[0].epoch,records:$p[0].record_states,jobs:$p[0].job_states,executed:($e[0]|sort),total:([$p[0].states[].result]|add // 0)}' > result.json
  preparation +
  "\njq -n --slurpfile c workflow.json --slurpfile e parsed-events.json '{config:$c[0],events:$e[0]}' | jq '" +
  recovery_selection_filter() +
  "' > plan.json\n" +
  execution +
  "\n"
}

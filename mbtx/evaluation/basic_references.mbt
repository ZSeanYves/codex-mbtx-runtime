///|
fn basic_reconciliation_reference() -> String {
  basic_jq_reference(
    long_reconciliation(true, 0, basic=true).instance.reference,
    false,
  )
}

///|
fn basic_recovery_reference() -> String {
  let source =
    #|import {
    #|  "moonbitlang/async@0.21.3",
    #|  "moonbitlang/async@0.21.3/fs",
    #|  "moonbitlang/async@0.21.3/shell",
    #|  "moonbitlang/core/json",
    #|}
    #|async fn main {
    #|  let selection = @shell.Cmd("jq", [".records|sort_by(.id,.generation)|group_by(.id)|map(last)", "records.json"]).output()
    #|  assert_eq(selection.exit_code(), 0)
    #|  guard @json.parse(selection.stdout()) is Array(records) else { fail("record array") }
    #|  let states : Array[Json] = []
    #|  let executed : Array[String] = []
    #|  let ids : Array[String] = []
    #|  let mut generation = 0
    #|  let mut total = 0
    #|  for record in records {
    #|    guard record is { "id": String(id), "generation": Number(gen, ..), "value": Number(value, ..), .. } else { fail("record shape") }
    #|    let expected : Json = { "id": id.to_json(), "generation": gen.to_int().to_json(), "result": (value.to_int() * value.to_int()).to_json() }
    #|    let path = "state/" + id + ".json"
    #|    let valid = try { @json.parse(@fs.read_file(path).text()) == expected } catch { _ => false }
    #|    if !valid {
    #|      let result = @shell.Cmd("fixture-worker", ["recover", "records.json", id]).output()
    #|      assert_eq(result.exit_code(), 0)
    #|      executed.push(id)
    #|    }
    #|    let state = @json.parse(@fs.read_file(path).text())
    #|    assert_eq(state, expected)
    #|    states.push(state)
    #|    ids.push(id)
    #|    generation = Int::max(generation, gen.to_int())
    #|    total += value.to_int() * value.to_int()
    #|  }
    #|  for name in @fs.readdir("state") {
    #|    if name.has_suffix(".pending") { @fs.remove("state/" + name) }
    #|  }
    #|  let checkpoint : Json = { "completed": ids.to_json(), "generation": generation.to_json() }
    #|  @fs.write_file("checkpoint.json", checkpoint.stringify() + "\n")
    #|  for index, id in ids {
    #|    assert_eq(@json.parse(@fs.read_file("state/" + id + ".json").text()), states[index])
    #|  }
    #|  let output : Json = { "completed": states.to_json(), "total": total.to_json(), "executed": executed.to_json() }
    #|  @fs.write_file("result.json", output.stringify() + "\n")
    #|}
  source
}

///|
/// Translate only our frozen jq-only reference fragments into literal argv.
/// This is fixture authoring, not a model-visible Shell execution facility.
fn basic_jq_reference(script : String, forensic : Bool) -> String {
  let parts = script.split("'").map(p => p.to_owned()).to_array()
  let commands : Array[Json] = []
  for i in 0..<parts.length() {
    if i % 2 == 0 {
      continue
    }
    let lines = parts[i - 1].split("\n").map(p => p.to_owned()).to_array()
    let prefix = lines[lines.length() - 1].trim().to_owned()
    if prefix != "jq" && !prefix.has_prefix("jq ") {
      abort("reference fragment must use literal jq")
    }
    let tokens = prefix
      .split(" ")
      .filter(p => !p.is_empty())
      .map(p => p.to_owned())
      .to_array()
    let args = tokens[1:].to_owned()
    args.push(parts[i])
    let suffix = parts[i + 1].split("\n").to_array()[0]
      .split(">")
      .map(p => p.to_owned())
      .to_array()
    if suffix.length() != 2 {
      abort("jq reference requires one output file")
    }
    for arg in suffix[0].split(" ") {
      if !arg.is_empty() {
        args.push(arg.to_owned())
      }
    }
    commands.push({
      "args": args.to_json(),
      "output": suffix[1].trim().to_owned().to_json(),
    })
  }
  let intro =
    #|import {"moonbitlang/async@0.21.3","moonbitlang/async@0.21.3/fs","moonbitlang/async@0.21.3/shell","moonbitlang/core/json"}
    #|async fn main {
  let capture = if forensic {
    (
      #| let emitted=@shell.Cmd("fixture-worker",["emit","streams.json"]).output(max_output_bytes=4000000)
      #| assert_eq(emitted.exit_code(),0)
      #| assert_eq(emitted.stdout(),@fs.read_file("stdout.input").text())
      #| assert_eq(emitted.stderr(),@fs.read_file("stderr.input").text())
      #| @fs.write_file("stdout.log",emitted.stdout());@fs.write_file("stderr.log",emitted.stderr())
    )
  } else {
    ""
  }
  let body =
    #| for command in commands {
    #|  guard command is {"args":Array(argv),"output":String(path),..} else {fail("command")}
    #|  let args=argv.map(a=>match a {String(s)=>s;_=>abort("arg")})
    #|  let child=@shell.Cmd("jq",args).output(max_output_bytes=4000000)
    #|  assert_eq(child.exit_code(),0);@fs.write_file(path,child.stdout())
    #| }
    #|}
  intro +
  "\n" +
  capture +
  "\n let commands : Array[Json] = " +
  commands.to_json().stringify() +
  "\n" +
  body
}

///|
fn basic_jobs_reference() -> String {
  let source =
    #|import {"moonbitlang/async@0.21.3","moonbitlang/async@0.21.3/fs","moonbitlang/async@0.21.3/shell","moonbitlang/core/json"}
    #|async fn main {
    #| let order=@shell.Cmd("jq",["-c",".jobs as $all | ($all|map(select(.enabled))) as $nodes | reduce range(0;($nodes|length)) as $i ([]; . as $done | . + ([$nodes[] | select(.id as $id | $done|index($id)|not) | select(all(.deps[]; . as $id | $done|index($id))) | .id]|sort|.[0:1]))","jobs.json"]).output()
    #| assert_eq(order.exit_code(),0)
    #| guard @json.parse(order.stdout()) is Array(ids) else {fail("job order")}
    #| let states : Array[Json]=[]
    #| for id in ids {
    #|  guard id is String(id) else {fail("id")}
    #|  let args=["job","jobs.json",id]
    #|  let child=@shell.Cmd("fixture-worker",args).output()
    #|  if child.exit_code()==7 {assert_eq(@shell.Cmd("fixture-worker",args).output().exit_code(),0)} else {assert_eq(child.exit_code(),0)}
    #|  states.push(@json.parse(@fs.read_file("state/"+id+".json").text()))
    #| }
    #| let input : Json={"states":states.to_json(),"plan":@json.parse(@fs.read_file("jobs.json").text())}
    #| let result=@shell.Cmd("jq",["-c","{completed:(.states|sort_by(.id)|map({id,result})),skipped:([.plan.jobs[]|select(.enabled|not)|.id]|sort),total:([.states[].result]|add)}"],stdin=@shell.Text(input.stringify())).output()
    #| assert_eq(result.exit_code(),0);@fs.write_file("result.json",result.stdout())
    #|}
  source
}

///|
fn basic_release_reference() -> String {
  let source =
    #|import {"moonbitlang/async@0.21.3","moonbitlang/async@0.21.3/fs","moonbitlang/async@0.21.3/shell","moonbitlang/core/json"}
    #|async fn hash(path : String) -> String {let r=@shell.Cmd("fixture-worker",["hash",path]).output();assert_eq(r.exit_code(),0);r.stdout().trim().to_owned()}
    #|async fn main {
    #| let plan=@json.parse(@fs.read_file("release-plan.json").text())
    #| guard plan is {"artifacts":Array(artifacts),"version":String(version),"obsolete":Array(obsolete),..} else {fail("plan")}
    #| let files : Array[Json]=[];let repaired : Array[String]=[]
    #| for artifact in artifacts {
    #|  guard artifact is {"target":String(path),"sha256":String(expected),..} else {fail("artifact")}
    #|  let input : Json={"artifact":artifact,"version":version.to_json()}
    #|  let order=@shell.Cmd("jq",["-c",". as $i | .artifact.candidates|map(select(.version==$i.version))|sort_by(-.priority,.path)|map(.path)"],stdin=@shell.Text(input.stringify())).output()
    #|  assert_eq(order.exit_code(),0);guard @json.parse(order.stdout()) is Array(candidates) else {fail("candidates")}
    #|  let mut selected=""
    #|  for candidate in candidates {guard candidate is String(candidate) else {fail("path")};if selected=="" && hash(candidate)==expected {selected=candidate}}
    #|  assert_true(selected!="")
    #|  let valid=if @fs.exists(path) {hash(path)==expected} else {false}
    #|  if !valid {@fs.write_file(path,@fs.read_file(selected));repaired.push(path)}
    #|  assert_eq(hash(path),expected);files.push({"path":path.to_json(),"sha256":expected.to_json(),"source":selected.to_json()})
    #| }
    #| for path in obsolete {guard path is String(path) else {fail("obsolete")};if @fs.exists(path) {@fs.remove(path)}}
    #| let input : Json={"version":version.to_json(),"files":files.to_json(),"removed":obsolete.to_json(),"repaired":repaired.to_json()}
    #| let output=@shell.Cmd("jq",["-c",".files|=sort_by(.path)|.removed|=sort|.repaired|=sort"],stdin=@shell.Text(input.stringify())).output()
    #| assert_eq(output.exit_code(),0);@fs.write_file("result.json",output.stdout());@fs.write_file("manifest.json",output.stdout())
    #|}
  source
}

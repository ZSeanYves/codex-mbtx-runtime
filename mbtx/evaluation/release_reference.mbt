///|
fn release_selection_filter() -> String {
  let filter =
    #| . as $doc |
    #| def chain($id): $doc.profiles[]|select(.id==$id)|. as $p|(if .parent==null then [] else chain(.parent) end)+[$p];
    #| (reduce chain(.selected)[] as $p ({enabled:{},version:null};
    #| ($p.values | if $p.schema==1 then .enabled=.active | .version=.release | del(.active,.release) else . end) as $v |
    #| reduce ($v.clear // [])[] as $key (.;del(.[$key])) | .enabled=((.enabled // {})+($v.enabled // {})) | if $v.version!=null then .version=$v.version else . end)) as $cfg |
    #| ($doc.modules) as $nodes |
    #| (if $doc.tier==0 then [$nodes[].id] else [$doc.changes[]|(.path,.old_path // empty)|. as $p|$nodes[]|select(.prefix as $prefix|$p|startswith($prefix))|.id]|unique end) as $direct |
    #| (reduce range(0;($nodes|length)) as $i ($direct; . as $seen | (. + [$nodes[] | select(any((.deps+.compat_deps)[];. as $d|$seen|index($d)))|.id])|unique)) as $all |
    #| (reduce range(0;($nodes|length)) as $i (($doc.deleted // []); . as $gone | (. + [$nodes[] | select(any((.deps+.compat_deps)[];. as $d|$gone|index($d)))|.id])|unique)) as $removed |
    #| {version:$cfg.version,modules:([$nodes[]|select(.id as $id|($all|index($id)) and ($removed|index($id)|not) and (if $cfg.enabled|has($id) then $cfg.enabled[$id] else true end))]|sort_by(.id)),all_modules:$nodes,obsolete:($doc.obsolete+[$removed[]|"release/"+.+".dat","release/"+.+".refs.json"]),tier:$doc.tier}
  filter
}

///|
fn release_reference_mbtx() -> String {
  let start =
    #|import {"moonbitlang/async@0.21.3","moonbitlang/async@0.21.3/fs","moonbitlang/async@0.21.3/shell","moonbitlang/core/json"}
    #|async fn jq(filter : String, data : Json) -> Json {
    #| let r=@shell.Cmd("jq",["-c",filter],stdin=@shell.Text(data.stringify())).output(max_output_bytes=1000000)
    #| assert_eq(r.exit_code(),0); @json.parse(r.stdout())
    #|}
    #|async fn hash(path : String) -> String {
    #| let r=@shell.Cmd("fixture-worker",["hash",path]).output();assert_eq(r.exit_code(),0);r.stdout().trim().to_owned()
    #|}
    #|async fn main {
    #| let catalog=@json.parse(@fs.read_file("catalog.json").text())
  let body =
    #| guard plan is {"modules":Array(modules),"version":String(version),"obsolete":Array(obsolete),"tier":Number(tier,..),..} else {fail("release plan")}
    #| if !@fs.exists("release") && @fs.exists("release-backup") {@fs.rename("release-backup","release")}
    #| if @fs.exists("release-stage") {@fs.rmdir("release-stage",recursive=true)}
    #| @fs.mkdir("release-stage")
    #| for name in @fs.readdir("release") {@fs.write_file("release-stage/"+name,@fs.read_file("release/"+name))}
    #| let chosen : Array[Json]=[];let executed : Array[String]=[]
    #| for module in modules {
    #|  guard module is {"id":String(id),"sha256":String(expected),"binary":binary,..} else {fail("module")}
    #|  let ordered=jq(". as $i | .module.candidates|map(select(.version==$i.version and (.path|startswith(\"candidates/\")) and (.path|split(\"/\")|all(.!=\"..\" and .!=\".\"))))|sort_by(-.priority,.path)",{"module":module,"version":version.to_json()})
    #|  guard ordered is Array(candidates) else {fail("candidates")}
    #|  let mut source=""
    #|  for candidate in candidates {
    #|   guard candidate is {"path":String(path),"sha256":String(raw),..} else {fail("candidate")}
    #|   if source=="" && @fs.exists(path) && hash(path)==raw {
    #|    let bytes=@fs.read_file(path).text()
    #|    @fs.write_file("candidate-normalized",if binary==true.to_json() {bytes} else {bytes.replace_all(old="\r\n",new="\n")})
    #|    if hash("candidate-normalized")==expected {source=path}
    #|   }
    #|  }
    #|  assert_true(source!="")
    #|  let target="release/"+id+".dat"
    #|  let valid=if @fs.exists(target) {hash(target)==expected} else {false}
    #|  if !valid {
    #|   let bytes=@fs.read_file(source).text()
    #|   @fs.write_file("release-stage/"+id+".dat",if binary==true.to_json() {bytes} else {bytes.replace_all(old="\r\n",new="\n")})
    #|   executed.push(id)
    #|  }
    #|  assert_eq(hash("release-stage/"+id+".dat"),expected)
    #|  chosen.push({"id":id.to_json(),"path":target.to_json(),"sha256":expected.to_json(),"source":source.to_json()})
    #| }
    #| for path in obsolete {guard path is String(path) else {fail("obsolete")};let name=path.replace(old="release/",new="");if @fs.exists("release-stage/"+name) {@fs.remove("release-stage/"+name)}}
    #| let entries : Array[Json]=[]
    #| for row in chosen {
    #|  guard row is {"id":String(id),..} else {fail("row")}
    #|  let deps=jq(". as $i|.modules[]|select(.id==$i.id)|(.deps+.compat_deps)|unique",{"modules":modules.to_json(),"id":id.to_json()})
    #|  guard deps is Array(deps) else {fail("deps")}
    #|  let refs : Array[Json]=[]
    #|  for dep in deps {guard dep is String(dep) else {fail("dep")};refs.push({"id":dep.to_json(),"sha256":hash("release-stage/"+dep+".dat").to_json()})}
    #|  guard row is Object(fields) else {fail("row object")};fields["refs"]=refs.to_json();entries.push(Json::object(fields))
    #|  if tier.to_int()==2 {@fs.write_file("release-stage/"+id+".refs.json",refs.to_json().stringify())}
    #| }
    #| if @fs.exists("release-backup") {@fs.rmdir("release-backup",recursive=true)}
    #| @fs.rename("release","release-backup");@fs.rename("release-stage","release");@fs.rmdir("release-backup",recursive=true)
    #| for row in entries {guard row is {"path":String(path),"sha256":String(expected),..} else {fail("entry")};assert_eq(hash(path),expected)}
    #| let result=jq("{version:.version,affected:[.files[].id],files:.files}",{"version":version.to_json(),"files":entries.to_json()})
    #| @fs.write_file("manifest.json",result.stringify())
    #| guard result is Object(fields) else {fail("manifest")};fields["executed"]=executed.to_json()
    #| @fs.write_file("result.json",Json::object(fields).stringify())
    #|}
  start +
  "\n let plan=jq(" +
  release_selection_filter().to_json().stringify() +
  ",catalog)\n" +
  body
}

///|
fn release_reference_shell() -> String {
  let body =
    #|if ! test -d release && test -d release-backup; then mv release-backup release; fi
    #|if test -d release-stage; then rm -r -- release-stage; fi
    #|mkdir release-stage
    #|cp -a release/. release-stage/
    #|: > chosen.jsonl
    #|: > executed.txt
    #|jq -r '.modules[].id' plan.json > modules.txt
    #|while IFS= read -r id; do
    #| expected=$(jq -r --arg id "$id" '.modules[]|select(.id==$id)|.sha256' plan.json)
    #| jq -r --arg id "$id" '. as $p|.modules[]|select(.id==$id)|.candidates|map(select(.version==$p.version and (.path|startswith("candidates/")) and (.path|split("/")|all(.!=".." and .!="."))))|sort_by(-.priority,.path)[]|[.path,.sha256]|@tsv' plan.json > candidates.tsv
    #| source=''
    #| while IFS="$(printf '\t')" read -r path raw; do
    #|  if test -f "$path" && test "$(fixture-worker hash "$path")" = "$raw"; then
    #|   if jq -e --arg id "$id" '.modules[]|select(.id==$id)|.binary' plan.json >/dev/null; then cp "$path" candidate-normalized;
    #|   else jq -Rjs 'gsub("\r\n";"\n")' "$path" > candidate-normalized; fi
    #|   if test "$(fixture-worker hash candidate-normalized)" = "$expected"; then source=$path; break; fi
    #|  fi
    #| done < candidates.tsv
    #| test -n "$source"
    #| target="release/$id.dat"
    #| if ! test -f "$target" || test "$(fixture-worker hash "$target")" != "$expected"; then
    #|   if jq -e --arg id "$id" '.modules[]|select(.id==$id)|.binary' plan.json >/dev/null; then cp "$source" "release-stage/$id.dat";
    #|   else jq -Rjs 'gsub("\r\n";"\n")' "$source" > "release-stage/$id.dat"; fi
    #|   printf '%s\n' "$id" >> executed.txt
    #| fi
    #| test "$(fixture-worker hash "release-stage/$id.dat")" = "$expected"
    #| jq -n --arg id "$id" --arg path "$target" --arg sha256 "$expected" --arg source "$source" '{id:$id,path:$path,sha256:$sha256,source:$source}' >> chosen.jsonl
    #|done < modules.txt
    #|jq -r '.obsolete[]' plan.json > obsolete.txt
    #|while IFS= read -r path; do target="release-stage/${path#release/}"; if test -f "$target"; then rm -- "$target"; fi; done < obsolete.txt
    #|: > entries.jsonl
    #|while IFS= read -r id; do
    #| jq -r --arg id "$id" '.modules[]|select(.id==$id)|(.deps+.compat_deps)|unique[]' plan.json > deps.txt
    #| : > refs.jsonl
    #| while IFS= read -r dep; do actual=$(fixture-worker hash "release-stage/$dep.dat"); jq -n --arg id "$dep" --arg sha256 "$actual" '{id:$id,sha256:$sha256}' >> refs.jsonl; done < deps.txt
    #| jq -s . refs.jsonl > refs.json
    #| jq -c --arg id "$id" --slurpfile refs refs.json 'select(.id==$id)|.refs=$refs[0]' chosen.jsonl >> entries.jsonl
    #| if test "$(jq .tier plan.json)" = 2; then cp refs.json "release-stage/$id.refs.json"; fi
    #|done < modules.txt
    #|if test -d release-backup; then rm -r -- release-backup; fi
    #|mv release release-backup
    #|mv release-stage release
    #|rm -r -- release-backup
    #|while IFS= read -r id; do expected=$(jq -r --arg id "$id" 'select(.id==$id)|.sha256' entries.jsonl); test "$(fixture-worker hash "release/$id.dat")" = "$expected"; done < modules.txt
    #|jq -s --slurpfile p plan.json '{version:$p[0].version,affected:[.[].id],files:.}' entries.jsonl > manifest.json
    #|jq -Rn '[inputs]' executed.txt > executed.json
    #|jq --slurpfile e executed.json '.executed=$e[0]' manifest.json > result.json
  "set -eu\njq '" +
  release_selection_filter() +
  "' catalog.json > plan.json\n" +
  body +
  "\n"
}

///|
fn long_impact(extended : Bool, variant : Int) -> LongCase {
  let count = if extended { 7 } else { 4 }
  let components : Array[Json] = []
  let checks : Array[Json] = []
  let affected : Array[String] = []
  let blocked : Array[String] = []
  let check_results : Array[Json] = []
  for i in 0..<count {
    let id = "component-\{i}"
    components.push({
      "id": id.to_json(),
      "prefix": ("src/" + id + "/").to_json(),
      "deps": if i == 0 {
        []
      } else {
        ["component-\{i - 1}".to_json()]
      },
    })
    let status = if extended && i == count - 2 { "failed" } else { "passed" }
    checks.push({
      "component": id.to_json(),
      "status": status.to_json(),
      "evidence": "checks/\{i}:\{variant + 10}".to_json(),
    })
    affected.push(id)
    check_results.push({
      "component": id.to_json(),
      "status": status.to_json(),
      "evidence": "checks/\{i}:\{variant + 10}".to_json(),
    })
    if status == "failed" {
      blocked.push(id)
    }
  }
  let changes : Array[Json] = [
    { "path": "src/component-0/value.mbt", "status": "modified" },
    { "path": "docs/ignored.md", "status": "modified" },
  ]
  if extended {
    changes.push({
      "path": "generated/value.mbt",
      "old_path": "src/component-0/old.mbt",
      "status": "renamed",
    })
  }
  let expected : Json = {
    "direct": ["component-0"],
    "affected": affected.to_json(),
    "checks": check_results.to_json(),
    "blocked": blocked.to_json(),
  }
  long_plain({
    goal: "Investigate the change set using components.json, changes.json and checks.json. Map each new and old path to its owning prefix, ignoring paths under excluded prefixes. Find directly changed components, then all transitive reverse dependencies. Produce deterministic dependency-first order (lexical ID ties). Attach each affected component's validation status and evidence locator, and list failed checks as blocked. For renames inspect both paths, even when the new path is excluded. Unaffected components and documentation-only changes must not be included.",
    schema: "{direct:string[],affected:string[],checks:[{component:string,status:string,evidence:string}],blocked:string[]}",
    files: {
      "components.json": (
        {
          "components": components.to_json(),
          "excluded": ["docs/", "generated/"],
        } : Json)
      .stringify()
      .to_json(),
      "changes.json": changes.to_json().stringify().to_json(),
      "checks.json": checks.to_json().stringify().to_json(),
    },
    expected,
    reference: (
      #|jq -n --slurpfile c components.json --slurpfile d changes.json --slurpfile t checks.json '
      #| $c[0] as $cfg | $cfg.components as $nodes |
      #| [$d[0][] | (.path,.old_path // empty) | . as $p | select(all($cfg.excluded[]; . as $e | $p|startswith($e)|not)) | $nodes[] | select(.prefix as $q | $p|startswith($q)) | .id] | unique as $direct |
      #| (reduce range(0;($nodes|length)) as $i ($direct; . as $seen | (. + [$nodes[] | select(any(.deps[]; . as $d | $seen|index($d))) | .id]) | unique)) as $all |
      #| (reduce range(0;($nodes|length)) as $i ([]; . as $done | . + ([$nodes[] | select(.id as $id | ($all|index($id)) and ($done|index($id)|not)) | select(all(.deps[]; . as $dep | $done|index($dep))) | .id]|sort))) as $ordered |
      #| {direct:$direct,affected:$ordered,checks:[$ordered[] as $id | $t[0][] | select(.component==$id)],blocked:[$ordered[] as $id | $t[0][] | select(.component==$id and .status=="failed")|.component]}' > result.json
      #|
    ),
  })
}

///|
fn long_migration(extended : Bool, variant : Int) -> LongCase {
  let defaults : Json = {
    "timeout_s": 5,
    "retries": 2,
    "enabled": "true",
    "labels": { "team": "core", "note": "keep 雪" },
  }
  let layers : Array[Json] = [
    { "id": "base", "parent": Json::null(), "values": defaults },
    {
      "id": "region",
      "parent": "base",
      "values": { "timeout_s": (variant + 7).to_json() },
    },
    {
      "id": "service",
      "parent": "region",
      "values": { "retries": (variant % 4 + 1).to_json() },
    },
  ]
  if extended {
    layers.push({
      "id": "damaged",
      "parent": "service",
      "values": { "timeout_s": -1, "enabled": "perhaps" },
    })
    layers.push({
      "id": "final",
      "parent": "damaged",
      "values": { "retries": 4 },
    })
  }
  let rejected : Json = if extended {
    [
      { "layer": "damaged", "key": "enabled", "value": "perhaps" },
      { "layer": "damaged", "key": "timeout_s", "value": -1 },
    ]
  } else {
    []
  }
  let config : Json = {
    "schema_version": 2,
    "timeout_ms": ((variant + 7) * 1000).to_json(),
    "retries": (if extended { 4 } else { variant % 4 + 1 }).to_json(),
    "enabled": true,
    "labels": { "team": "core", "note": "keep 雪" },
  }
  let expected : Json = {
    "config": config,
    "rejected": rejected,
    "applied_layers": if extended {
      ["base", "region", "service", "damaged", "final"]
    } else {
      ["base", "region", "service"]
    },
  }
  let base = long_plain({
    goal: "Migrate the selected inherited configuration in layers.json to schema version 2. Traverse parents before children; shallow-merge accepted keys while preserving unrelated keys and labels. timeout_s must be a nonnegative integer; convert it to timeout_ms and remove timeout_s. enabled accepts only booleans or strings true/false. retries must be a nonnegative integer. Reject each invalid assignment and keep the inherited value, then continue applying valid assignments. Sort rejections by layer and key. Write migrated.json, rejected.json and result.json. Verify that applying the v2 normalization again would make no change. Do not alter layers.json.",
    schema: "{config:object,rejected:[{layer:string,key:string,value:any}],applied_layers:string[]}",
    files: {
      "layers.json": (
        {
          "selected": if extended {
            "final"
          } else {
            "service"
          },
          "layers": layers.to_json(),
        } : Json)
      .stringify()
      .to_json(),
    },
    expected,
    reference: (
      #|jq 'def chain($id): . as $doc | $doc.layers[] | select(.id==$id) | . as $n | (if .parent==null then [] else $doc|chain($n.parent) end)+[$n];
      #| chain(.selected) as $layers | reduce $layers[] as $l ({config:{},rejected:[],applied_layers:[]}; .applied_layers += [$l.id] | reduce ($l.values|to_entries[]) as $e (.; if (($e.key=="timeout_s" or $e.key=="retries") and ($e.value|type!="number" or .<0 or floor!=.)) or ($e.key=="enabled" and ([$e.value]|inside([true,false,"true","false"])|not)) then .rejected += [{layer:$l.id,key:$e.key,value:$e.value}] else .config[$e.key]=$e.value end)) |
      #| .config |= (.schema_version=2 | .timeout_ms=(.timeout_s*1000) | del(.timeout_s) | .enabled=(.enabled==true or .enabled=="true")) | .rejected |= sort_by(.layer,.key)' layers.json > result.json
      #|jq '.config' result.json > migrated.json
      #|jq '.rejected' result.json > rejected.json
      #|
    ),
  })
  { ..base, outputs: { "migrated.json": config, "rejected.json": rejected }, }
}

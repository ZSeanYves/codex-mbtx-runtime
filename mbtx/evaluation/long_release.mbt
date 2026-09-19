///|
fn long_release(
  extended : Bool,
  variant : Int,
  basic? : Bool = false,
) -> LongCase {
  let contents = ["alpha\n", "beta 雪\n"]
  let hashes = [
    "b6a98d9ce9a2d9149288fa3df42d377c3e42737afdcdaf714e33c0a100b51060", "b1ee4947020ae104c19c55222e3141d61dbbfa4d3dfd932af986506849f6dada",
  ]
  let version = "v\{variant + 2}"
  let n = if extended { 5 } else { 3 }
  let files : Map[String, Json] = { "release/obsolete.txt": "old artifact\n" }
  if basic {
    files["release/unrelated 雪.dat"] = "preserve unrelated\r\n"
  }
  let outputs : Map[String, Json] = Map([])
  let worker_events : Array[Json] = []
  let specs : Array[Json] = []
  let manifest : Array[Json] = []
  let repaired : Array[String] = []
  let mutable : Array[String] = ["release/obsolete.txt"]
  for i in 0..<n {
    let target = "release/component-\{i}.dat"
    let source = "candidates/current component-\{i}.dat"
    let old = "candidates/old-\{i}.dat"
    files[source] = contents[i % 2].to_json()
    files[old] = "obsolete version\n".to_json()
    outputs[target] = contents[i % 2].to_json()
    mutable.push(target)
    if i == 0 {
      files[target] = contents[i % 2].to_json()
    } else {
      repaired.push(target)
      if i % 2 == 1 {
        files[target] = "corrupt\n".to_json()
      }
    }
    let candidates : Array[Json] = [
      { "path": old.to_json(), "version": "v0", "priority": 99 },
      { "path": source.to_json(), "version": version.to_json(), "priority": 2 },
    ]
    if extended {
      let invalid = "candidates/broken-\{i}.dat"
      files[invalid] = "wrong hash\n".to_json()
      candidates.push({
        "path": invalid.to_json(),
        "version": version.to_json(),
        "priority": 9,
      })
    }
    if basic {
      let tie = "candidates/tied component-\{i}.dat"
      files[tie] = contents[i % 2].to_json()
      candidates.push({
        "path": tie.to_json(),
        "version": version.to_json(),
        "priority": 2,
      })
      for path in [source, target] {
        worker_events.push({
          "action": "hash",
          "id": Json::null(),
          "exit_code": 0,
          "args": ["hash", path.to_json()],
        })
      }
    }
    specs.push({
      "target": target.to_json(),
      "sha256": hashes[i % 2].to_json(),
      "candidates": candidates.to_json(),
    })
    manifest.push({
      "path": target.to_json(),
      "sha256": hashes[i % 2].to_json(),
      "source": source.to_json(),
    })
  }
  files["release-plan.json"] = (
      {
        "version": version.to_json(),
        "artifacts": specs.to_json(),
        "obsolete": ["release/obsolete.txt"],
      } : Json)
    .stringify()
    .to_json()
  let expected : Json = {
    "version": version.to_json(),
    "files": manifest.to_json(),
    "removed": ["release/obsolete.txt"],
    "repaired": repaired.to_json(),
  }
  outputs["manifest.json"] = expected
  let base = long_plain({
    goal: "Reconcile the release directory with release-plan.json. For each target, select candidates matching the requested version, verify their actual SHA-256 bytes using ./fixture-worker hash PATH, then choose greatest priority among hash-valid candidates (lexically smallest path breaks ties). Priority never overrides invalid content or a wrong version. Keep targets whose actual hash is already correct, repair missing/corrupt targets by copying the selected candidate, remove only the explicitly listed obsolete paths, and preserve candidate inputs. Rehash all final targets. Write identical manifest.json and result.json containing version, files sorted by target path with actual sha256 and selected source, removed paths, and repaired paths. This is a final-state oracle plus content verification; a self-reported success flag is insufficient.",
    schema: "{version:string,files:[{path:string,sha256:string,source:string}],removed:string[],repaired:string[]}",
    files,
    expected,
    reference: (
      #|: > release-results.jsonl
      #|: > repaired.txt
      #|jq -c '.artifacts|sort_by(.target)[]' release-plan.json | while IFS= read -r row; do
      #| target=$(printf '%s' "$row" | jq -r .target); expected=$(printf '%s' "$row" | jq -r .sha256); version=$(jq -r .version release-plan.json)
      #| printf '%s' "$row" | jq -r --arg version "$version" '.candidates|map(select(.version==$version))|sort_by(-.priority,.path)[]|.path' > candidates.txt
      #| selected=''; while IFS= read -r candidate; do actual=$(./fixture-worker hash "$candidate"); if test "$actual" = "$expected"; then selected=$candidate; break; fi; done < candidates.txt
      #| test -n "$selected"
      #| if test -f "$target" && test "$(./fixture-worker hash "$target")" = "$expected"; then :; else cp "$selected" "$target"; printf '%s\n' "$target" >> repaired.txt; fi
      #| actual=$(./fixture-worker hash "$target"); test "$actual" = "$expected"
      #| jq -n --arg path "$target" --arg sha256 "$actual" --arg source "$selected" '{path:$path,sha256:$sha256,source:$source}' >> release-results.jsonl
      #|done
      #|jq -r '.obsolete[]' release-plan.json | while IFS= read -r path; do if test -f "$path"; then rm -- "$path"; fi; done
      #|jq -Rn '[inputs]' repaired.txt > repaired.json
      #|jq -s --slurpfile p release-plan.json --slurpfile r repaired.json '{version:$p[0].version,files:sort_by(.path),removed:($p[0].obsolete|sort),repaired:($r[0]|sort)}' release-results.jsonl > result.json
      #|cp result.json manifest.json
      #|
    ),
  })
  {
    ..base,
    outputs,
    mutable_inputs: mutable,
    worker_events,
    absent: ["release/obsolete.txt"],
  }
}

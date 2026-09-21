///|
fn release_id(i : Int, variant : Int) -> String {
  "m\{100+(i*7+variant)%97}"
}

///|
fn release_case(tier : Int, variant : Int, epoch : Int) -> Json {
  let n = [4, 12, 24][tier]
  let profile_count = [1, 3, 6][tier]
  let files : Map[String, Json] = {
    "release/keep 雪.txt": "unrelated release\r\n",
    "source/preserve.bin": "BIN\u0000keep\r\n",
  }
  let previous = if epoch > 0 {
    release_case(tier, variant, epoch - 1)
  } else {
    Json::null()
  }
  if epoch > 0 {
    for path, value in v4_fields(previous, "files") {
      if path.has_prefix("release/") {
        files[path] = value
      }
    }
    for path, value in v4_fields(previous, "expected_outputs") {
      if !path.has_suffix(".refs.json") {
        files[path] = (match value {
          String(s) => s
          _ => value.stringify()
        }).to_json()
      }
    }
    let absent : Array[String] = @json.from_json(
      field(previous, "absent_outputs"),
    ) catch {
      _ => []
    }
    for path in absent {
      files.remove(path)
    }
  } else {
    files["release/obsolete.dat"] = "obsolete generation"
    files["release-stage/orphan.dat"] = "interrupted staging"
    files["release-backup/interrupted.dat"] = "abandoned backup"
    files["manifest.json"] = "{\"version\":\"stale\",\"files\":[{\"source\":\"invented\"}]}"
  }
  let version = "v\{variant+epoch+1}"
  let modules : Array[Json] = []
  let payloads : Array[Json] = []
  let selected_sources : Array[String] = []
  let dependencies : Array[Array[Int]] = []
  for i in 0..<n {
    let id = release_id(i, variant)
    let payload = release_payload(variant + i * 3 + epoch * 11)
    payloads.push(payload)
    let deps : Array[Int] = if tier == 0 || i == 0 || i == n - 2 {
      []
    } else {
      [(i - 1) / (if variant % 2 == 0 { 3 } else { 2 })]
    }
    let compatibility : Array[Int] = if tier == 2 &&
      i > 4 &&
      (i + variant) % 3 == 0 {
      [i - 3]
    } else {
      []
    }
    let all = deps.copy()
    for dep in compatibility {
      if !all.contains(dep) {
        all.push(dep)
      }
    }
    dependencies.push(all)
    let count = if tier == 0 {
      if i < 2 {
        2
      } else {
        1
      }
    } else if tier == 1 {
      if i < 6 {
        3
      } else {
        2
      }
    } else {
      3
    }
    let candidates : Array[Json] = []
    let mut source = ""
    for candidate in 0..<count {
      let prefix = if candidate == 0 {
        if variant % 2 == 0 {
          "a"
        } else {
          "z"
        }
      } else if candidate == 1 {
        "bad"
      } else {
        "b"
      }
      let path = if tier == 2 && candidate == 2 && i % 4 == 0 {
        "candidates/../source/preserve.bin"
      } else {
        "candidates/\{id} \{prefix} 雪.dat"
      }
      let corrupt = candidate == 1
      let raw = if candidate % 2 == 0 {
        v4_text(payload, "crlf")
      } else {
        v4_text(payload, "text")
      }
      let incorrect = release_payload(variant + i * 3 + epoch * 11 + 1)
      let plausible = tier == 2 && corrupt && i % 2 == 0
      if !path.contains("../") {
        files[path] = (if plausible {
          v4_text(incorrect, "text")
        } else if corrupt {
          "invalid candidate payload"
        } else {
          raw
        }).to_json()
      }
      let current = candidate != 2 || variant % 2 == 0
      candidates.push({
        "path": path.to_json(),
        "version": (if current { version } else { "old" }).to_json(),
        "priority": (if corrupt { 99 } else { 3 }).to_json(),
        "sha256": if plausible {
          field(incorrect, "sha256")
        } else {
          field(
            payload,
            if candidate % 2 == 0 {
              "crlf_sha256"
            } else {
              "sha256"
            },
          )
        },
      })
      if !corrupt &&
        current &&
        !path.contains("../") &&
        (source == "" || codepoint_compare(path, source) < 0) {
        source = path
      }
    }
    selected_sources.push(source)
    let binary = (variant + i * 3 + epoch * 11) % 96 % 4 == 0
    modules.push({
      "id": id.to_json(),
      "prefix": "src/\{id}/".to_json(),
      "deps": deps.map(j => release_id(j, variant)).to_json(),
      "compat_deps": compatibility.map(j => release_id(j, variant)).to_json(),
      "binary": binary.to_json(),
      "sha256": field(payload, "sha256"),
      "candidates": candidates.to_json(),
    })
    let target = "release/\{id}.dat"
    if epoch == 0 {
      if i == 0 || i == n - 2 || i == n - 1 {
        files[target] = field(payload, "text")
      } else if (i + variant) % 2 == 0 {
        files[target] = "damaged target"
      }
    }
  }
  let enabled : Map[String, Json] = Map([])
  for i in 0..<n {
    enabled[release_id(i, variant)] = (tier == 0 || i != n - 1).to_json()
  }
  let profiles : Array[Json] = []
  for level in 0..<profile_count {
    let schema = if tier == 2 && level % 2 == 0 { 1 } else { 2 }
    let values : Map[String, Json] = Map([])
    values[if schema == 1 { "active" } else { "enabled" }] = enabled.to_json()
    values[if schema == 1 { "release" } else { "version" }] = (if level ==
      profile_count - 1 {
      version
    } else {
      "old"
    }).to_json()
    if tier == 2 && level == profile_count - 2 {
      values["clear"] = ["version"]
    }
    profiles.push({
      "id": "profile-\{level}".to_json(),
      "parent": if level == 0 {
        Json::null()
      } else {
        "profile-\{level-1}".to_json()
      },
      "schema": schema.to_json(),
      "values": values.to_json(),
    })
  }
  let changed = if epoch == 2 { [3, 4] } else { [1, 2] }
  let changes : Array[Json] = []
  for i in changed {
    changes.push({
      "path": "generated/renamed-\{i}".to_json(),
      "old_path": "src/\{release_id(i,variant)}/old.dat".to_json(),
      "status": "renamed",
    })
  }
  changes.push({ "path": "docs/unrelated.txt", "status": "modified" })
  let affected : Array[Int] = if tier == 0 {
    [0, 1, 2, 3]
  } else {
    changed.copy()
  }
  for _ in 0..<n {
    for i, deps in dependencies {
      if !affected.contains(i) && deps.any(j => affected.contains(j)) {
        affected.push(i)
      }
    }
  }
  let removed_roots : Array[Int] = if tier == 2 && epoch == 2 {
    [3]
  } else {
    []
  }
  let removed = removed_roots.copy()
  for _ in 0..<n {
    for i, deps in dependencies {
      if !removed.contains(i) && deps.any(j => removed.contains(j)) {
        removed.push(i)
      }
    }
  }
  let removed_paths : Array[String] = []
  for i in removed {
    removed_paths.append([
      "release/\{release_id(i,variant)}.dat",
      "release/\{release_id(i,variant)}.refs.json",
    ])
  }
  let selected = affected.filter(i => {
    (tier == 0 || i != n - 1) && !removed.contains(i)
  })
  if epoch == 0 {
    for i in 0..<n {
      if !selected.contains(i) {
        files["release/\{release_id(i,variant)}.dat"] = field(
          payloads[i],
          "text",
        )
      }
    }
  }
  selected.sort_by((a, b) => {
    codepoint_compare(release_id(a, variant), release_id(b, variant))
  })
  let outputs : Map[String, Json] = Map([])
  let mutable : Array[String] = [
    "manifest.json", "release/obsolete.dat", "release-stage/orphan.dat", "release-backup/interrupted.dat",
  ]
  mutable.append(removed_paths[:])
  let executed : Array[String] = []
  for i in selected {
    let id = release_id(i, variant)
    let target = "release/\{id}.dat"
    if files.get(target) != Some(field(payloads[i], "text")) {
      executed.push(id)
    }
    outputs[target] = field(payloads[i], "text")
    mutable.push(target)
    if tier == 2 {
      mutable.push("release/\{id}.refs.json")
    }
  }
  let preserve_previous : Array[String] = []
  if epoch > 0 {
    for path, value in v4_fields(previous, "expected_outputs") {
      if path.has_suffix(".refs.json") && !mutable.contains(path) {
        outputs[path] = value
        preserve_previous.push(path)
      }
    }
  }
  let entries : Array[Json] = []
  let receipts : Array[Json] = []
  for i in selected {
    let id = release_id(i, variant)
    let refs : Array[Json] = []
    let deps = dependencies[i].copy()
    deps.sort_by((a, b) => {
      codepoint_compare(release_id(a, variant), release_id(b, variant))
    })
    for j in deps {
      let dep = release_id(j, variant)
      let digest = if selected.contains(j) {
        field(payloads[j], "sha256")
      } else {
        let path = "release/\{dep}.dat"
        let content = files.get(path).unwrap()
        let mut found = Json::null()
        for candidate in 0..<96 {
          let p = release_payload(candidate)
          if field(p, "text") == content {
            found = field(p, "sha256")
          }
        }
        found
      }
      refs.push({ "id": dep.to_json(), "sha256": digest })
    }
    entries.push({
      "id": id.to_json(),
      "path": "release/\{id}.dat".to_json(),
      "sha256": field(payloads[i], "sha256"),
      "source": selected_sources[i].to_json(),
      "refs": refs.to_json(),
    })
    if tier == 2 {
      outputs["release/\{id}.refs.json"] = refs.to_json()
    }
    for path in [selected_sources[i], "release/\{id}.dat"] {
      receipts.push({
        "action": "hash",
        "id": Json::null(),
        "args": ["hash", path.to_json()],
        "exit_code": 0,
      })
    }
  }
  let manifest : Json = {
    "version": version.to_json(),
    "affected": selected.map(i => release_id(i, variant)).to_json(),
    "files": entries.to_json(),
  }
  outputs["manifest.json"] = manifest
  let expected = match manifest {
    Object(o) => o.copy()
    _ => abort("manifest")
  }
  expected["executed"] = executed.to_json()
  let absent : Array[String] = [
    "release/obsolete.dat", "release-stage/orphan.dat", "release-backup/interrupted.dat",
  ]
  absent.append(removed_paths[:])
  if tier == 2 && variant % 2 == 0 && epoch == 0 {
    let original = files.copy()
    for path, value in original {
      if path.has_prefix("release/") {
        let backup = path.replace(old="release/", new="release-backup/")
        files.remove(path)
        files[backup] = value
        mutable.push(backup)
        absent.push(backup)
        if !mutable.contains(path) {
          outputs[path] = value
        }
      }
    }
  }

  files["catalog.json"] = (
      {
        "tier": tier.to_json(),
        "modules": modules.to_json(),
        "profiles": profiles.to_json(),
        "selected": "profile-\{profile_count-1}".to_json(),
        "changes": changes.to_json(),
        "deleted": removed_roots.map(i => release_id(i, variant)).to_json(),
        "obsolete": ["release/obsolete.dat"],
      } : Json)
    .stringify()
    .to_json()
  {
    "files": files.to_json(),
    "expected": Json::object(expected),
    "expected_outputs": outputs.to_json(),
    "preserve_previous": preserve_previous.to_json(),
    "mutable_inputs": mutable.to_json(),
    "absent_outputs": absent.to_json(),
    "worker_events": receipts.to_json(),
    "output": "result.json",
    "actions": ["hash"],
    "strict_worker_receipts": true,
    "operation_receipts": true,
    "exact_worker_actions": [],
    "goal": "Repair a reproducible release from catalog.json. Resolve selected profile parent-first. Schema 1 active/release fields mean enabled/version; schema 2 uses enabled/version. Apply clear deletions first, merge enabled maps, then apply non-null version. Quick selects all modules; other tiers map both rename paths to module prefixes and expand reverse dependencies across deps plus compat_deps, then exclude disabled modules. Ignore unrelated paths. For each affected module choose matching-version candidates by descending priority then Unicode code-point path order. Reject unsafe paths outside candidates/ or containing dot/dot-dot segments. Verify actual candidate SHA-256 using fixture-worker hash PATH; priority cannot override incorrect raw or normalized content; continue to the next candidate if either hash is wrong. Binary payloads retain all bytes; text normalizes CRLF to LF. Final target release/ID.dat must match module.sha256. Preserve correct targets and all unaffected bytes. Expand catalog.deleted transitively through reverse deps and compat_deps; exclude this deleted closure from the affected build and remove its release/ID.dat and release/ID.refs.json. Also remove catalog.obsolete. Stage a complete release directory, including preserved files, recover an interrupted backup if release is absent, replace the directory through release-stage/release-backup, and remove abandoned staging/backup data. For each affected module, manifest entries contain its actual final hash, chosen source and unique sorted refs [{id,sha256}] for all deps and compat_deps using their actual final bytes. Deep also writes release/ID.refs.json equal to that refs array. Verify published files with the worker. For each affected module, every invocation including an unchanged repeat must obtain successful worker hashes for its chosen candidate and published release/ID.dat. These required receipt paths use the exact workspace-relative candidate path from catalog.json and release/ID.dat, without a leading ./ or an absolute prefix. Additional hashes of temporary normalized files, staged files, dependencies and rejected candidates are allowed. manifest.json contains {version,affected,files}. affected contains exactly the affected, enabled, non-deleted module IDs in ascending Unicode code-point order; files contains one entry per affected ID in that same order. Each refs array is deduplicated and sorted by dependency ID. result.json adds sorted executed IDs whose target bytes were repaired in this invocation. An unchanged second invocation must preserve all outputs and return executed:[]. Deep is additionally re-run at later catalog/candidate generations while retaining the preceding release. Inputs, source/preserve.bin and unrelated release files must remain unchanged.",
    "output_schema": "{version:string,affected:string[],files:[{id,path,sha256,source,refs:[{id,sha256}]}],executed:string[]}",
    "reference_shell": release_reference_shell().to_json(),
    "reference_mbtx": release_reference_mbtx().to_json(),
  }
}

///|
fn v4_release(tier : Int, variant : Int) -> Json {
  let base = release_case(tier, variant, 0)
  let object = match base {
    Object(o) => o.copy()
    _ => abort("case")
  }
  let phases : Array[Json] = []
  for epoch in 0..<(if tier == 2 { 3 } else { 1 }) {
    let current = release_case(tier, variant, epoch)
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
          if path == "catalog.json" || path.has_prefix("candidates/") {
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

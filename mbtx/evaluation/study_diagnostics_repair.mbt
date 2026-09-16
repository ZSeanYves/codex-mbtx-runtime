///|
fn diagnostics_repair_case(index : Int, v : Int) -> StudyCase {
  let n = 23 + v * 3
  match index {
    0 => {
      let log = StringBuilder()
      let counts : Map[String, Int] = { "ERROR": 0, "WARN": 0, "INFO": 0 }
      for i in 0..<(n * 7) {
        let level = ["INFO", "ERROR", "WARN", "INFO", "INFO"][(i + v) % 5]
        counts[level] = counts.get(level).unwrap_or(0) + 1
        log.write_string(
          "\{100000+i} [" + level + "] message mentions ERROR \{i}\n",
        )
      }
      {
        goal: "Count actual severities in service.log using only the bracketed level after the timestamp. Words such as ERROR inside the message are not a severity. Include ERROR, INFO, WARN keys even if zero.",
        schema: "{ERROR:integer,INFO:integer,WARN:integer}",
        files: { "service.log": log.to_string().to_json() },
        expected: counts.to_json(),
        reference: (
          #|jq -Rs 'split("\n")|map(select(length>0)|capture("^[0-9]+ \\[(?<level>ERROR|WARN|INFO)\\]").level)|reduce .[] as $level ({ERROR:0,INFO:0,WARN:0}; .[$level]+=1)' service.log > result.json
          #|
        ),
      }
    }
    1 => {
      let rows : Array[Json] = []
      let expected : Array[Json] = []
      for i in 0..<n {
        let id = "req\{1000+i+v}"
        rows.push({ "request": id.to_json(), "event": "start" })
        let errors = (i + v) % 4
        for _ in 0..<errors {
          rows.push({ "request": id.to_json(), "event": "error" })
        }
        rows.push({ "request": id.to_json(), "event": "finish" })
        if errors > 0 {
          expected.push({ "request": id.to_json(), "errors": errors.to_json() })
        }
      }
      rows.rev_in_place()
      study_case(
        "Correlate events by request in input.json regardless of arrival order. Return only requests with one or more error events, with their error counts, sorted lexically by request. Start and finish events do not count as errors.",
        "[{request:string,errors:integer}]",
        rows.to_json(),
        expected.to_json(),
        "map(select(.event==\"error\"))|group_by(.request)|map({request:.[0].request,errors:length})|sort_by(.request)",
      )
    }
    2 => {
      let rows : Array[Json] = []
      let failed : Array[String] = []
      for i in 0..<n {
        let name = "test\{1000+i+v}"
        rows.push({ "name": name.to_json(), "attempt": 1, "passed": false })
        let passed = (i + v) % 3 != 0
        rows.push({
          "name": name.to_json(),
          "attempt": 2,
          "passed": passed.to_json(),
        })
        if !passed {
          failed.push(name)
        }
      }
      rows.rev_in_place()
      study_case(
        "Summarize test results in input.json using only the largest numeric attempt per test name (unique name/attempt pairs). Return the number of distinct tests, and lexically sorted names whose final attempt failed. Earlier failures followed by a pass are recovered.",
        "{tests:integer,failed:string[]}",
        rows.to_json(),
        { "tests": n.to_json(), "failed": failed.to_json() },
        "group_by(.name)|map(max_by(.attempt))|{tests:length,failed:(map(select(.passed==false)|.name)|sort)}",
      )
    }
    3 => {
      let rows : Array[Json] = []
      let expected : Array[Json] = []
      for i in 0..<n {
        let retries = (i + v) % 6
        let port = 8000 + i + v
        let row : Map[String, Json] = {
          "name": "service\{i}".to_json(),
          "port": "\{port}".to_json(),
          "debug": true.to_json(),
        }
        if i % 2 == 0 {
          row["retries"] = "\{retries}".to_json()
        }
        rows.push(row.to_json())
        expected.push({
          "name": "service\{i}".to_json(),
          "port": port.to_json(),
          "retries": (if i % 2 == 0 { retries } else { 2 }).to_json(),
        })
      }
      study_case(
        "Migrate each object in input.json in array order. Preserve name, convert decimal-string port and retries to integers, default absent retries to 2, and discard all other keys. Port and provided retries always contain valid decimal integers. Write a new file; preserve input.json.",
        "[{name:string,port:integer,retries:integer}]",
        rows.to_json(),
        expected.to_json(),
        "map({name,port:(.port|tonumber),retries:((.retries//\"2\")|tonumber)})",
      )
    }
    4 => {
      let rows : Array[Json] = []
      let expected : Array[Json] = []
      for i in 0..<n {
        let active = (i + v) % 3 != 0
        let text = "endpoint=https://old.example/\{v+i}; old.example remains literal in notes"
        let new = if active {
          text.replace(
            old="endpoint=https://old.example/",
            new="endpoint=https://new.example/",
          )
        } else {
          text
        }
        rows.push({
          "id": i.to_json(),
          "active": active.to_json(),
          "text": text.to_json(),
        })
        expected.push({ "id": i.to_json(), "text": new.to_json() })
      }
      study_case(
        "For each record in input.json preserve id and text, but only if active=true replace the exact literal 'endpoint=https://old.example/' with 'endpoint=https://new.example/'. Leave every other character untouched, including old.example in notes. Return objects with only id and text in input order.",
        "[{id:integer,text:string}]",
        rows.to_json(),
        expected.to_json(),
        "map({id,text:(if .active then .text|split(\"endpoint=https://old.example/\")|join(\"endpoint=https://new.example/\") else .text end)})",
      )
    }
    _ => {
      let base : Map[String, Json] = Map([])
      let site : Map[String, Json] = Map([])
      let overrides : Map[String, Json] = Map([])
      let expected : Map[String, Json] = Map([])
      for i in 0..<n {
        let key = "key\{i+v}"
        base[key] = i.to_json()
        if i % 3 == 0 {
          site[key] = (i + v + 10).to_json()
        }
        if i % 4 == 0 {
          overrides[key] = Json::null()
        } else if i % 4 == 1 {
          overrides[key] = (0).to_json()
        }
        if i % 4 != 0 {
          expected[key] = (if i % 4 == 1 {
            0
          } else if i % 3 == 0 {
            i + v + 10
          } else {
            i
          }).to_json()
        }
      }
      study_case(
        "Merge configuration objects in input.json in precedence order base, site, local. A present key replaces the lower layer even if its value is zero. A null deletes a key. Absent keys inherit. Return the resulting flat integer-valued object.",
        "{key:integer}",
        {
          "base": base.to_json(),
          "site": site.to_json(),
          "local": overrides.to_json(),
        },
        expected.to_json(),
        "[.base,.site,.local]|reduce .[] as $layer ({}; .+$layer)|with_entries(select(.value!=null))",
      )
    }
  }
}

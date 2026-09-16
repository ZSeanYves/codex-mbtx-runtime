///|
fn bounded_recovery_case(index : Int, v : Int) -> StudyCase {
  let n = 31 + v * 2
  match index {
    0 => {
      let log = StringBuilder()
      let mut errors = 0
      for i in 0..<(2047 + v * 128) {
        let error = (i + v) % 97 == 0
        if error {
          errors += 1
        }
        log.write_string(
          (if error { "ERROR " } else { "INFO " }) +
          "record \{i}; payload 雪 \{v}\n",
        )
      }
      let last = "tail 雪 \{v}"
      log.write_string(last + "\n")
      {
        goal: "Summarize large.log without dumping it into the model conversation. Count total newline-terminated records and records beginning with the exact prefix 'ERROR ', and return the last record without its newline. Preserve UTF-8.",
        schema: "{lines:integer,errors:integer,last:string}",
        files: { "large.log": log.to_string().to_json() },
        expected: {
          "lines": (2048 + v * 128).to_json(),
          "errors": errors.to_json(),
          "last": last.to_json(),
        },
        reference: (
          #|jq -Rs 'split("\n")|.[0:-1]|{lines:length,errors:(map(select(startswith("ERROR ")))|length),last:.[-1]}' large.log > result.json
          #|
        ),
      }
    }
    1 => {
      let log = StringBuilder()
      let counts : Map[String, Int] = Map([])
      for i in 0..<(n * 17) {
        let word = "word\{1000+(i*i+v)%13}"
        counts[word] = counts.get(word).unwrap_or(0) + 1
        log.write_string(word + "\n")
      }
      let pairs = counts.to_array()
      pairs.sort_by((a, b) => {
        if a.1 != b.1 {
          b.1.compare(a.1)
        } else {
          codepoint_compare(a.0, b.0)
        }
      })
      let expected = pairs[:Int::min(5, pairs.length())].map(p => {
        "word": p.0.to_json(),
        "count": p.1.to_json(),
      })
      {
        goal: "Compute the five most frequent whole-line words in words.txt without dumping the file. Sort by descending frequency, breaking ties by ascending word; return fewer than five if fewer unique words exist. Every line is nonempty and newline-terminated.",
        schema: "[{word:string,count:integer}]",
        files: { "words.txt": log.to_string().to_json() },
        expected: expected.to_json(),
        reference: (
          #|jq -Rs 'split("\n")|.[0:-1]|group_by(.)|map({word:.[0],count:length})|sort_by([-.count,.word])|.[0:5]' words.txt > result.json
          #|
        ),
      }
    }
    2 => {
      let files : Map[String, Json] = Map([])
      let mut count = 0
      let mut total = 0
      let mut minimum = 2147483647
      let mut maximum = -2147483647
      for shard in 0..<7 {
        let rows : Array[Int] = []
        for i in 0..<(n + shard) {
          let value = (i * 19 + v * 3 + shard) % 401 - 200
          rows.push(value)
          count += 1
          total += value
          minimum = Int::min(minimum, value)
          maximum = Int::max(maximum, value)
        }
        files["data/shard\{shard}.json"] = (rows.to_json().stringify() + "\n").to_json()
      }
      {
        goal: "Aggregate all JSON arrays in data/*.json without dumping the inputs. Return the count, signed sum, minimum and maximum of all integers. Every matching file contains an array; at least one value exists in total.",
        schema: "{count:integer,sum:integer,min:integer,max:integer}",
        files,
        expected: {
          "count": count.to_json(),
          "sum": total.to_json(),
          "min": minimum.to_json(),
          "max": maximum.to_json(),
        },
        reference: (
          #|jq -s 'add|{count:length,sum:add,min:min,max:max}' data/*.json > result.json
          #|
        ),
      }
    }
    3 => {
      let a : Array[Json] = []
      let b : Array[Json] = []
      let ids : Array[Int] = []
      let mut total = 0
      for i in 0..<n {
        let id = i + v
        let value = i * 7 - v
        let row : Json = { "id": id.to_json(), "value": value.to_json() }
        if i % 3 != 0 {
          a.push(row)
        }
        b.push(row)
        ids.push(id)
        total += value
      }
      a.rev_in_place()
      {
        goal: "Reconstruct from both batch-a.json and batch-b.json. checkpoint.json is untrusted stale state and must not affect the answer. Deduplicate records by id (duplicates have equal value), sort numeric ids, and sum value once per distinct id.",
        schema: "{ids:integer[],total:integer}",
        files: {
          "batch-a.json": a.to_json().stringify().to_json(),
          "batch-b.json": b.to_json().stringify().to_json(),
          "checkpoint.json": "{\"processed\":[9999],\"total\":999999}".to_json(),
        },
        expected: { "ids": ids.to_json(), "total": total.to_json() },
        reference: (
          #|jq -s 'add|unique_by(.id)|sort_by(.id)|{ids:map(.id),total:(map(.value)|add//0)}' batch-a.json batch-b.json > result.json
          #|
        ),
      }
    }
    4 => {
      let events : Array[Json] = []
      let balances : Map[String, Int] = Map([])
      for i in 0..<n {
        let account = "account\{(i+v)%5}"
        let delta = i * 3 - v
        let row : Json = {
          "event": i.to_json(),
          "account": account.to_json(),
          "delta": delta.to_json(),
        }
        events.push(row)
        if i % 2 == 0 {
          events.push(row)
        }
        balances[account] = balances.get(account).unwrap_or(0) + delta
      }
      events.rev_in_place()
      study_case(
        "Apply events in input.json exactly once per event id. Duplicated event ids have identical payloads. Start every encountered account at zero and sum signed deltas per account, including zero balances. Arrival order is irrelevant.",
        "{account:integer}",
        events.to_json(),
        balances.to_json(),
        "unique_by(.event)|reduce .[] as $e ({}; .[$e.account]=((.[$e.account]//0)+$e.delta))",
      )
    }
    _ => {
      let events : Array[Json] = []
      let committed : Array[Int] = []
      let mut total = 0
      for i in 0..<n {
        let id = i + v
        events.push({
          "transaction": id.to_json(),
          "type": "prepare",
          "amount": (i * 7 - v).to_json(),
        })
        let commit = (i + v) % 3 != 0
        events.push({
          "transaction": id.to_json(),
          "type": (if commit { "commit" } else { "abort" }).to_json(),
        })
        if commit {
          committed.push(id)
          total += i * 7 - v
        }
      }
      events.rev_in_place()
      study_case(
        "Reconcile transaction events in input.json regardless of arrival order. Each transaction has one prepare with integer amount and exactly one commit or abort. Include only committed transaction ids in numeric order and sum their prepared amounts. Aborted transactions contribute nothing.",
        "{committed:integer[],total:integer}",
        events.to_json(),
        { "committed": committed.to_json(), "total": total.to_json() },
        "group_by(.transaction)|map(select(any(.[];.type==\"commit\"))|map(select(.type==\"prepare\"))[0])|sort_by(.transaction)|{committed:map(.transaction),total:(map(.amount)|add//0)}",
      )
    }
  }
}

///|
fn text_process_case(index : Int, v : Int) -> StudyCase {
  let n = 13 + v * 2
  match index {
    0 => {
      let lines = StringBuilder()
      let expected : Array[String] = []
      for i in 0..<n {
        let text = ["雪", "café", "alpha", "é", "𝄞"][i % 5] +
          "-\{1000+i+v}"
        lines.write_string(" \t" + text + " \r\n\n" + text + "\n")
        expected.push(text)
      }
      expected.sort_by(codepoint_compare)
      {
        goal: "Read records.txt, trim ASCII space/tab/CR at each line's edges, drop empty records, deduplicate exact strings, and sort by Unicode code point. Preserve interior whitespace and Unicode bytes; do not perform NFC normalization.",
        schema: "string[]",
        files: { "records.txt": lines.to_string().to_json() },
        expected: expected.to_json(),
        reference: (
          #|jq -Rs 'split("\n")|map(gsub("^[ \\t\\r]+|[ \\t\\r]+$";""))|map(select(length>0))|unique' records.txt > result.json
          #|
        ),
      }
    }
    1 => {
      let lines = StringBuilder()
      let expected : Array[Json] = []
      for i in 0..<n {
        let name = "row,\{i+v}"
        let note = if i % 3 == 0 { "" } else { "say \"雪\" \{v}" }
        let value = i - v
        lines.write_string(
          "\"" +
          name +
          "\",\"" +
          note.replace_all(old="\"", new="\"\"") +
          "\",\{value}\r\n",
        )
        expected.push({
          "name": name.to_json(),
          "note": note.to_json(),
          "amount": value.to_json(),
        })
      }
      {
        goal: "Parse records.csv (no header) into objects with name, note and integer amount, preserving row order. The first two fields are quoted CSV strings that may be empty or contain commas and doubled quotes. The last field is a signed decimal integer. CRLF terminates each row; fields contain no embedded newline.",
        schema: "[{name:string,note:string,amount:integer}]",
        files: { "records.csv": lines.to_string().to_json() },
        expected: expected.to_json(),
        reference: (
          #|jq -Rs 'split("\n")|map(select(length>0)|rtrimstr("\r")|[scan("\"(?:[^\"]|\"\")*\"|[^,]+")]|map(if startswith("\"") then .[1:-1]|gsub("\"\"";"\"") else . end)|{name:.[0],note:.[1],amount:(.[2]|tonumber)})' records.csv > result.json
          #|
        ),
      }
    }
    2 => {
      let lines = StringBuilder()
      let expected : Array[Json] = []
      for i in 0..<n {
        let id = "record\{1000+i+v}"
        let body = "detail \{i}\n\n雪: \{v-i}"
        lines.write_string("@@ " + id + "\n" + body + "\n")
        expected.push({ "id": id.to_json(), "body": body.to_json() })
      }
      {
        goal: "Parse records.log into groups beginning with '@@ ID'. Every group contains one or more body lines until the next header or EOF. Preserve blank body lines and join body lines with newline, without a trailing newline. Return id and body in file order. Body lines never begin '@@ '.",
        schema: "[{id:string,body:string}]",
        files: { "records.log": lines.to_string().to_json() },
        expected: expected.to_json(),
        reference: (
          #|jq -Rs 'split("@@ ")[1:]|map(rtrimstr("\n")|split("\n")|{id:.[0],body:(.[1:]|join("\n"))})' records.log > result.json
          #|
        ),
      }
    }
    3 | 5 => {
      let values : Array[Int] = []
      let results : Array[Json] = []
      for i in 0..<n {
        let value = i * 3 - v
        values.push(value)
        results.push(
          if index == 5 && value % 5 == 0 {
            { "value": value.to_json(), "status": "rejected", "exit_code": 7 }
          } else if index == 5 {
            {
              "value": value.to_json(),
              "status": "ok",
              "square": (value * value).to_json(),
            }
          } else {
            { "value": value.to_json(), "square": (value * value).to_json() }
          },
        )
      }
      if index == 3 {
        study_case(
          "For every integer in input.json, invoke ./fixture-worker square N and check the child exit status. Collect its JSON output preserving input order. The worker accepts signed decimal integers; batching the orchestration is allowed.",
          "[{value:integer,square:integer}]",
          values.to_json(),
          results.to_json(),
          "[]",
        )
        |> fn(c) {
          {
            ..c,
            reference: (
              #|jq -r '.[]' input.json | while IFS= read -r value; do ./fixture-worker square "$value" || exit; done > children.jsonl
              #|jq -s '.' children.jsonl > result.json
              #|
            ),
          }
        }
      } else {
        study_case(
          "Invoke ./fixture-worker checked-square N for each integer in input.json in order. Multiples of five return exit 7 and must produce {value,status:\"rejected\",exit_code:7}; other values exit zero with {value,square}, which must produce {value,status:\"ok\",square}. Continue after expected rejections, but propagate any unexpected failure.",
          "[{value:integer,status:string,square?:integer,exit_code?:integer}]",
          values.to_json(),
          results.to_json(),
          "[]",
        )
        |> fn(c) {
          {
            ..c,
            reference: (
              #|jq -r '.[]' input.json | while IFS= read -r value; do if ./fixture-worker checked-square "$value" > child.json; then jq '.+{status:"ok"}' child.json; else code=$?; test "$code" = 7 || exit "$code"; jq -n --argjson v "$value" '{value:$v,status:"rejected",exit_code:7}'; fi; done > children.jsonl
              #|jq -s '.' children.jsonl > result.json
              #|
            ),
          }
        }
      }
    }
    _ => {
      let args = ["", "space \{v}", "雪", "'\"$();*", "line\nbreak"]
      let text = "input 雪 \{v}\n".repeat(n)
      // The byte count is a Unicode boundary, not a character count.
      let expected : Json = {
        "args": args.to_json(),
        "stdin": text.to_json(),
        "stdin_bytes": @utf8.encode(text).length().to_json(),
      }
      {
        goal: "Read payload.txt and argv.json. Invoke ./fixture-worker echo with exactly the array's arguments and pipe the exact UTF-8 payload bytes to stdin, then close stdin. Save the worker's JSON object (args, stdin, stdin_bytes), preserving empty arguments, quotes and newlines. Check the child exit status. The argument array always has five strings.",
        schema: "{args:string[],stdin:string,stdin_bytes:integer}",
        files: {
          "payload.txt": text.to_json(),
          "argv.json": (args.to_json().stringify() + "\n").to_json(),
        },
        expected,
        reference: (
          #|./fixture-worker echo "$(jq -r '.[0]' argv.json)" "$(jq -r '.[1]' argv.json)" "$(jq -r '.[2]' argv.json)" "$(jq -r '.[3]' argv.json)" "$(jq -r '.[4]' argv.json)" < payload.txt > result.json
          #|
        ),
      }
    }
  }
}

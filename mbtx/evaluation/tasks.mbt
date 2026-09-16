///|
fn task(
  id : String,
  goal : String,
  schema : String,
  files : Json,
  expected : Json,
  output? : String = "result.json",
) -> Json {
  {
    "id": id.to_json(),
    "goal": (goal +
    " Write the result to " +
    output +
    ". Output schema (use exactly these field names and types): " +
    schema +
    ". Work autonomously; complete scripts and programs are allowed. Do not modify other input files.").to_json(),
    "output_schema": schema.to_json(),
    "files": files,
    "output": output.to_json(),
    "expected": expected,
  }
}

///|
/// Versioned pilot inputs and independent output oracles. Expected outputs are
/// evaluator-private; only goals and input files enter the task workspace.
pub fn pilot_protocol() -> Json {
  let log = StringBuilder()
  for i in 0..<2047 {
    log.write_string(
      if i % 128 == 0 {
        "ERROR record\n"
      } else {
        "INFO record\n"
      },
    )
  }
  log.write_string("tail 雪\n")
  {
    "schema_version": 1,
    "protocol_id": "programmable-steps-pilot-v3",
    "population": "pilot",
    "observation": {
      "native_trace": true,
      "native_otel": true,
      "billing_lookup": "local 404; recorded and not forwarded; cost unknown",
    },
    "arms": ["shell_tool", "mbtx_program"],
    "seed": 20260916,
    "repeats": 1,
    "budgets": {
      "steps": 12,
      "tool_calls": 24,
      "requests": 16,
      "wall_seconds": 600,
      "tool_output_tokens": 4096,
    },
    "pacing": {
      "concurrency": 1,
      "min_interval_ms": 15000,
      "fallback_cooldown_ms": 30000,
    },
    "instructions": "Solve the supplied task in the current workspace. Inspect the input files and verify the actual result file. Both interfaces have rg, jq, sh and git available; login shell initialization is disabled. The MBTX dependency cache includes moonbitlang/async@0.21.3 (fs, shell, stdio) and the installed core library. Follow the MBTX tool's import and API example rather than guessing package names. Common file-editing tools are allowed and their use is counted separately. Do not access the network, delegate, or inspect files outside the workspace. Return a short final answer. A successful final answer alone is not the correctness oracle.",
    "tasks": [
      task(
        "repository-index",
        "Inspect the toy repository under src. Return an object with files (lexically sorted relative paths) and exported (lexically sorted function names marked pub).",
        "{\"files\": string[], \"exported\": string[]}",
        {
          "src/math.mbt": "pub fn add(a : Int, b : Int) -> Int { a + b }\nfn private_value() -> Int { 1 }\n",
          "src/text.mbt": "pub fn greet() -> String { \"hello\" }\n",
          "src/main.mbt": "fn main { println(\"sample\") }\n",
        },
        {
          "files": ["src/main.mbt", "src/math.mbt", "src/text.mbt"],
          "exported": ["add", "greet"],
        },
      ),
      task(
        "structured-summary",
        "Read orders.json. Sum quantity times unit_price by customer and overall, using integer arithmetic. Return customers and total.",
        "{\"customers\": {customer_name: integer}, \"total\": integer}",
        {
          "orders.json": "[{\"customer\":\"alpha\",\"quantity\":2,\"unit_price\":7},{\"customer\":\"beta\",\"quantity\":3,\"unit_price\":5},{\"customer\":\"alpha\",\"quantity\":0,\"unit_price\":9}]\n",
        },
        { "customers": { "alpha": 14, "beta": 15 }, "total": 29 },
      ),
      task(
        "file-normalization",
        "Read records.txt. Trim surrounding whitespace, remove blank and duplicate records, and return a lexically sorted JSON array. Preserve Unicode.",
        "string[]",
        { "records.txt": "雪\n alpha \n\n雪\nbeta\nalpha\n" },
        ["alpha", "beta", "雪"],
      ),
      task(
        "process-orchestration",
        "Invoke ./fixture-worker square N separately for N=1,2,3. It returns JSON. Return the three result objects in numeric order, checking every child exit status.",
        "[{\"value\": integer, \"square\": integer}]",
        {},
        [
          { "value": 1, "square": 1 },
          { "value": 2, "square": 4 },
          { "value": 3, "square": 9 },
        ],
      ),
      task(
        "log-diagnostics",
        "Inspect service.log. Count ERROR records and return errors and codes (unique error codes, sorted). WARN and INFO are not errors.",
        "{\"errors\": integer, \"codes\": string[]}",
        {
          "service.log": "INFO starting\nERROR E_CONN disconnected\nWARN E_CONN retry scheduled\nERROR E_PARSE invalid record\nINFO finished\n",
        },
        { "errors": 2, "codes": ["E_CONN", "E_PARSE"] },
      ),
      task(
        "configuration-repair",
        "Repair settings.json to valid JSON containing only integer port=8080 and retries=2. The existing syntax and types are broken. Verify the repaired file.",
        "{\"port\": integer, \"retries\": integer}",
        { "settings.json": "{\"port\":\"8080\",\"retries\":2,}\n" },
        { "port": 8080, "retries": 2 },
        output="settings.json",
      ),
      task(
        "bounded-output",
        "Inspect large.log without dumping the entire file into the conversation. Return lines (total records), errors (records beginning ERROR), and last (last record without its newline).",
        "{\"lines\": integer, \"errors\": integer, \"last\": string}",
        { "large.log": log.to_string().to_json() },
        { "lines": 2048, "errors": 16, "last": "tail 雪" },
      ),
      task(
        "recovery-merge",
        "Recover a partially processed dataset. Read both batch files; checkpoint.json may be stale. Deduplicate by id, sort ids, and return ids and the sum of value for unique records. Repeated ids have identical values.",
        "{\"ids\": integer[], \"total\": integer}",
        {
          "batch-a.json": "[{\"id\":1,\"value\":10},{\"id\":2,\"value\":20}]\n",
          "batch-b.json": "[{\"id\":2,\"value\":20},{\"id\":3,\"value\":30},{\"id\":4,\"value\":40}]\n",
          "checkpoint.json": "{\"processed\":[1],\"total\":10}\n",
        },
        { "ids": [1, 2, 3, 4], "total": 100 },
      ),
    ],
    "limitations": [
      "Pilot only; freeze formal sample size after reviewing pilot cost and difficulty.",
      "This compares configured interfaces, including their available libraries and utilities.",
      "Token usage and process-spawn totals are unknown unless native evidence covers them.",
    ],
  }
}

///|
/// Validate captured output bytes and input preservation independently of model
/// claims. Snapshot errors are unknown, not a zero-step task success.
pub fn task_oracle(task : Json, snapshot : Json) -> Json raise {
  let output = string_field(task, "output")
  if field(snapshot, "errors") is Array(errors) && !errors.is_empty() {
    return {
      "success": Json::null(),
      "reason": "Output snapshot incomplete",
      "confidence": "unknown",
    }
  }
  let files = object_field(snapshot, "files")
  for path, content in object_field(task, "files") {
    if path != output && files.get(path) != Some(content) {
      return {
        "success": false,
        "reason": "An input file was modified or removed",
        "confidence": "observed",
      }
    }
  }
  let parsed = match files.get(output) {
    Some(String(content)) => @json.parse(content) catch { _ => Json::null() }
    _ => Json::null()
  }
  {
    "success": (parsed == field(task, "expected")).to_json(),
    "reason": "Independent structured output comparison",
    "confidence": "observed",
  }
}

///|
/// Public, answer-free delivery requirements. The prompt and collector's
/// contract preflight consume this same object; expected answers stay private.
fn natural_output_contract(id : String) -> Json {
  let text : Json = { "type": "string" }
  let integer : Json = { "type": "integer" }
  let files : Map[String, Json] = Map([])
  let rules : Array[String] = []
  match id {
    "natural-data-reconciliation" => {
      let ledger = natural_array_schema(
        natural_object_schema({
          "id": text,
          "account": text,
          "base_amount": integer,
          "kind": text,
        }),
      )
      let rejected = natural_array_schema(
        natural_object_schema({ "source_key": text, "reason": text }),
      )
      files["ledger.json"] = ledger
      files["rejected.json"] = rejected
      files["result.json"] = natural_object_schema({
        "ledger": ledger,
        "totals": { "type": "object", "additionalProperties": integer },
        "total": integer,
        "rejected": rejected,
      })
      rules.append([
        "ledger.json is an array of accepted records sorted by id in Unicode code-point lexical order. Each entry has exactly id, account, base_amount and kind; do not copy other source fields into this output.",
        "base_amount is the normalized signed integer amount multiplied by accounts.json.rates[currency], including the negative sign for refunds. Preserve original records by leaving source.jsonl, source.csv and accounts.json unchanged.",
        "rejected.json is an array sorted by source_key in Unicode code-point lexical order. Each entry has exactly source_key and reason; reason is invalid, superseded or missing_invoice.",
        "result.json has exactly ledger, totals, total and rejected. result.ledger must equal the entire ledger.json array and result.rejected must equal the entire rejected.json array; duplicate these arrays in result.json even though they are also separate required files.",
        "result.totals has exactly one key per account declared in accounts.json.accounts, including accounts with zero total, and each value is the sum of that account's accepted base_amount values. result.total is the sum of all accepted base_amount values.",
      ])
    }
    "natural-repository-impact" => {
      files["result.json"] = natural_object_schema({
        "direct": natural_array_schema(text),
        "affected": natural_array_schema(text),
        "checks": natural_array_schema(
          natural_object_schema({
            "component": text,
            "status": text,
            "evidence": text,
          }),
        ),
        "blocked": natural_array_schema(text),
      })
      rules.append([
        "Write the required output to result.json in cwd. Its only keys are direct, affected, checks and blocked; an alternate filename such as impact.json does not replace result.json.",
        "direct contains unique directly changed component ID strings sorted in Unicode code-point lexical order.",
        "affected contains component ID strings in dependency-first topological order: repeatedly choose the lexically smallest currently available affected component whose dependencies are already emitted. Do not put check objects in affected.",
        "checks contains exactly one object per affected component, in the same order as affected. Each object has exactly component, status and evidence, copied from that component's entry in checks.json.",
        "blocked contains the component ID strings whose check status is failed, preserving their order in affected. Unaffected components are excluded from all four arrays.",
      ])
    }
    _ => abort("unknown natural task output contract: " + id)
  }
  {
    "version": "natural-output-v2",
    "comparison": "exact_json",
    "files": files.to_json(),
    "rules": rules.to_json(),
  }
}

///|
fn natural_object_schema(properties : Map[String, Json]) -> Json {
  let required = properties.keys().to_array()
  required.sort()
  {
    "type": "object",
    "properties": properties.to_json(),
    "required": required.to_json(),
    "additionalProperties": false,
  }
}

///|
fn natural_array_schema(items : Json) -> Json {
  { "type": "array", "items": items }
}

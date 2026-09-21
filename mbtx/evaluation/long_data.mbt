///|
fn long_reconciliation(
  extended : Bool,
  variant : Int,
  basic? : Bool = false,
) -> LongCase {
  let n = if extended { 6 } else { 3 }
  let json_lines : Array[String] = []
  let csv = StringBuilder()
  csv.write_string(
    "source_key,id,account,amount,currency,revision,kind,reference\n",
  )
  let ledger : Array[Json] = []
  let rejected : Array[Json] = []
  let totals : Map[String, Int] = { "alpha": 0, "beta": 0 }
  let mut total = 0
  for i in 0..<n {
    let id = "invoice-\{i}"
    let account = if i % 2 == 0 { "alpha" } else { "beta" }
    let amount = variant + i + 11
    let currency = if i % 2 == 0 { "USD" } else { "EUR" }
    json_lines.push(
      (
        {
          "source_key": "json-\{i}".to_json(),
          "id": id.to_json(),
          "account": account.to_json(),
          "amount": amount.to_json(),
          "currency": currency.to_json(),
          "revision": 1,
          "kind": "invoice",
          "reference": "",
        } : Json).stringify(),
    )
    let winning = if i % 2 == 0 { amount + 2 } else { amount }
    if i % 2 == 0 {
      csv.write_string(
        "csv-\{i},\{id},\{account},\{winning},\{currency},2,invoice,\n",
      )
      rejected.push({
        "source_key": "json-\{i}".to_json(),
        "reason": "superseded",
      })
    }
    let converted = winning * (if currency == "EUR" { 2 } else { 1 })
    total += converted
    totals[account] = totals[account] + converted
    ledger.push({
      "id": id.to_json(),
      "account": account.to_json(),
      "base_amount": converted.to_json(),
      "kind": "invoice",
    })
  }
  csv.write_string(
    "invalid-account,bad-a,missing,10,USD,1,invoice,\ninvalid-amount,bad-b,alpha,abc,USD,1,invoice,\n",
  )
  rejected.push({ "source_key": "invalid-account", "reason": "invalid" })
  rejected.push({ "source_key": "invalid-amount", "reason": "invalid" })
  if extended {
    csv.write_string(
      "refund-good,refund-z,alpha,-3,USD,1,refund,invoice-0\nrefund-bad,bad-c,alpha,-9,USD,1,refund,absent\n",
    )
    ledger.push({
      "id": "refund-z",
      "account": "alpha",
      "base_amount": -3,
      "kind": "refund",
    })
    totals["alpha"] = totals["alpha"] - 3
    total -= 3
    rejected.push({ "source_key": "refund-bad", "reason": "missing_invoice" })
  }
  if basic {
    // Physical line locators identify records that cannot provide a source key.
    json_lines.push("")
    for text in ["{broken", "7", "{}"] {
      json_lines.push(text)
      rejected.push({
        "source_key": "source.jsonl:\{json_lines.length()}".to_json(),
        "reason": "invalid",
      })
    }
    let line = csv.to_string().split("\n").count()
    csv.write_string("bad-width,too,few\n")
    rejected.push({
      "source_key": "source.csv:\{line}".to_json(),
      "reason": "invalid",
    })
    csv.write_string(
      "invalid-zero,bad-zero,alpha,0,USD,1,invoice,\ninvalid-fraction,bad-fraction,alpha,1.5,USD,1,invoice,\ninvalid-currency,bad-currency,alpha,10,XXX,1,invoice,\n",
    )
    for key in ["invalid-zero", "invalid-fraction", "invalid-currency"] {
      rejected.push({ "source_key": key.to_json(), "reason": "invalid" })
    }
    // A same-revision conflict exercises the declared Unicode tie breaker.
    json_lines.push(
      (
        {
          "source_key": "tie-雪",
          "id": "invoice-1",
          "account": "beta",
          "amount": (variant + 12).to_json(),
          "currency": "EUR",
          "revision": 1,
          "kind": "invoice",
          "reference": "",
        } : Json).stringify(),
    )
    rejected.push({ "source_key": "json-1", "reason": "superseded" })
  }
  rejected.sort_by((a, b) => {
    codepoint_compare(
      string_field(a, "source_key") catch {
        _ => ""
      },
      string_field(b, "source_key") catch {
        _ => ""
      },
    )
  })
  let expected : Json = {
    "ledger": ledger.to_json(),
    "totals": totals.to_json(),
    "total": total.to_json(),
    "rejected": rejected.to_json(),
  }
  let base = long_plain({
    goal: "Reconcile transactions from source.jsonl and source.csv using accounts.json. CSV is unquoted UTF-8 with the supplied header and no embedded commas/newlines. For both source.jsonl and source.csv, accept LF or CRLF and skip blank physical data lines; do not add blank lines to rejected.json. Keep counting skipped blank lines when assigning physical 1-based line numbers to later invalid records. A CSV row with the wrong column count, or a malformed/non-object JSONL row or one lacking a nonempty string source_key, is rejected as invalid with source_key SOURCE_FILE:LINE (for example source.jsonl:8 for a malformed record on physical line 8). Other rows retain their supplied unique source_key. Normalize amount and revision from integer JSON numbers or signed decimal integer strings (no fractional or exponent strings). Reject malformed records, unknown accounts/currencies, nonpositive invoice amounts and nonnegative refunds. Select one valid record per id by greatest revision, breaking ties by source_key in Unicode code-point lexical order (greatest wins); mark the other valid records superseded. After selection, accept a refund only if its reference identifies a selected invoice for the same account; otherwise reject it as missing_invoice. Convert accepted amounts using the integer base-currency rates. Write ledger.json sorted by id, rejected.json sorted by source_key with reason invalid/superseded/missing_invoice, and result.json including per-account totals (including zero) and overall total. Preserve original records and verify reconciliation totals.",
    schema: "{ledger:[{id:string,account:string,base_amount:integer,kind:string}],totals:{account:integer},total:integer,rejected:[{source_key:string,reason:string}]}",
    files: {
      "source.jsonl": (json_lines.join(if basic { "\r\n" } else { "\n" }) + "\n").to_json(),
      "source.csv": (if basic {
        csv.to_string().replace_all(old="\n", new="\r\n")
      } else {
        csv.to_string()
      }).to_json(),
      "accounts.json": (
        { "accounts": ["alpha", "beta"], "rates": { "USD": 1, "EUR": 2 } } :
        Json)
      .stringify()
      .to_json(),
    },
    expected,
    reference: (
      #|jq -Rn '[inputs|rtrimstr("\r")] | .[0]|split(",")' source.csv > header.json
      #|jq -Rn --slurpfile h header.json '[inputs|rtrimstr("\r")]|to_entries|map(select(.key>0 and .value!="")|. as $line|(.value|split(",")) as $v|if ($v|length)!=($h[0]|length) then {source_key:("source.csv:"+($line.key+1|tostring)),invalid:true} else reduce range(0;($h[0]|length)) as $i ({}; .[$h[0][$i]]=$v[$i]) end)' source.csv > csv-normalized.json
      #|jq -Rn '[inputs|rtrimstr("\r")]|to_entries|map(select(.value!="")|. as $line|(try (.value|fromjson) catch null) as $row|if ($row|type)=="object" and ($row.source_key|type)=="string" and $row.source_key!="" then $row else {source_key:("source.jsonl:"+($line.key+1|tostring)),invalid:true} end)' source.jsonl > json-normalized.json
      #|jq -n --slurpfile c accounts.json --slurpfile a csv-normalized.json --slurpfile b json-normalized.json '
      #| def integer: if type=="number" then . elif type=="string" and test("^-?[0-9]+$") then tonumber else null end;
      #| $c[0] as $cfg | ($a[0]+$b[0] | map(.amount=(try (.amount|integer) catch null) | .revision=(try (.revision|integer) catch null))) as $all |
      #| def valid: try (. as $r | ($r.id|type=="string") and ($r.id!="") and ($cfg.accounts|index($r.account)!=null) and ($cfg.rates[$r.currency]!=null) and ($r.amount|type=="number") and ($r.revision|type=="number") and ($r.amount|floor==.) and ($r.revision|floor==.) and (($r.kind=="invoice" and $r.amount>0) or ($r.kind=="refund" and $r.amount<0))) catch false;
      #| ($all|map(select(valid))) as $valid | ($valid|sort_by(.id,.revision,.source_key)|group_by(.id)|map(last)) as $selected |
      #| def accepted: . as $r | .kind=="invoice" or any($selected[]; .kind=="invoice" and .id==$r.reference and .account==$r.account);
      #| ($selected|map(select(accepted))|sort_by(.id)|map({id,account,base_amount:(.amount*$cfg.rates[.currency]),kind})) as $ledger |
      #| {ledger:$ledger,totals:(reduce $cfg.accounts[] as $a ({}; .[$a]=([$ledger[]|select(.account==$a)|.base_amount]|add // 0))),total:([$ledger[].base_amount]|add // 0),rejected:(([$all[]|select(valid|not)|{source_key,reason:"invalid"}]+[$valid[]|. as $r|select(all($selected[];.source_key!=$r.source_key))|{source_key,reason:"superseded"}]+[$selected[]|select(accepted|not)|{source_key,reason:"missing_invoice"}])|sort_by(.source_key))}' > result.json
      #|jq '.ledger' result.json > ledger.json
      #|jq '.rejected' result.json > rejected.json
      #|
    ),
  })
  {
    ..base,
    outputs: {
      "ledger.json": ledger.to_json(),
      "rejected.json": rejected.to_json(),
    },
  }
}

///|
fn long_incident(
  extended : Bool,
  variant : Int,
  basic? : Bool = false,
) -> LongCase {
  let events : Array[Json] = []
  let requests : Array[Json] = []
  let incidents : Array[Json] = []
  let n = if extended { 4 } else { 2 }
  for i in 0..<n {
    let id = "request-\{variant}-\{i}"
    let recovered = i % 2 == 0
    let code = if i % 3 == 0 { "timeout" } else { "upstream_502" }
    let seq = i * 10 + 1
    let config = if i % 3 == 0 { "short" } else { "normal" }
    let end_code = if recovered { "ok" } else { code }
    events.push({
      "request": id.to_json(),
      "seq": seq.to_json(),
      "attempt": 1,
      "code": code.to_json(),
      "config": config.to_json(),
    })
    events.push({
      "request": id.to_json(),
      "seq": (seq + 1).to_json(),
      "attempt": 2,
      "code": end_code.to_json(),
      "config": config.to_json(),
    })
    requests.push({ "id": id.to_json(), "budget_ms": 1000, "service": "api" })
    incidents.push({
      "request": id.to_json(),
      "status": (if recovered { "recovered" } else { "failed" }).to_json(),
      "attempts": 2,
      "cause": (if code == "timeout" {
        "configuration_timeout"
      } else {
        "upstream"
      }).to_json(),
      "evidence": if basic {
        [
          {
            "seq": seq.to_json(),
            "file": "app.jsonl",
            "line": (i + 2).to_json(),
          },
          {
            "seq": (seq + 1).to_json(),
            "file": "retries.jsonl",
            "line": (n - i + 1).to_json(),
          },
        ]
      } else {
        [seq.to_json(), (seq + 1).to_json()]
      },
    })
  }
  // Deliberately reverse file order and include a duplicate and an untracked request.
  let app = events.filter(e => field(e, "attempt") == (1).to_json())
  let retry = events.filter(e => field(e, "attempt") == (2).to_json())
  retry.rev_in_place()
  if extended {
    app.push(events[0])
  }
  app.push({
    "request": "noise",
    "seq": 999,
    "attempt": 1,
    "code": "upstream_502",
    "config": "normal",
  })
  let base = long_plain({
    goal: "Investigate tracked requests in requests.json by correlating app.jsonl and retries.jsonl with configurations.json. Ignore untracked requests. Deduplicate identical request/seq events, sort by sequence and retain evidence sequence numbers. Count distinct attempt numbers, classify a request recovered if the last event is ok and failed otherwise. Attribute its first error to configuration_timeout when code is timeout and the event's referenced config timeout_ms is less than that request's budget_ms; other timeout is deadline, upstream_502 is upstream, all other errors are unknown. Do not use file order as event order. Emit one incident per tracked request in lexical request order, plus failed and recovered counts; verify counts against the tracked population.",
    schema: "{incidents:[{request:string,status:string,attempts:integer,cause:string,evidence:integer[]}],failed:integer,recovered:integer}",
    files: {
      "requests.json": requests.to_json().stringify().to_json(),
      "configurations.json": (
        { "short": { "timeout_ms": 200 }, "normal": { "timeout_ms": 2000 } } :
        Json)
      .stringify()
      .to_json(),
      "app.jsonl": (app.map(e => e.stringify()).join("\n") + "\n").to_json(),
      "retries.jsonl": (retry.map(e => e.stringify()).join("\n") + "\n").to_json(),
    },
    expected: {
      "incidents": incidents.to_json(),
      "failed": (n / 2).to_json(),
      "recovered": (n / 2).to_json(),
    },
    reference: (
      #|jq -s 'unique_by(.request,.seq)' app.jsonl retries.jsonl > events-normalized.json
      #|jq -n --slurpfile r requests.json --slurpfile c configurations.json --slurpfile e events-normalized.json '
      #| [$r[0][] as $r | [$e[0][]|select(.request==$r.id)]|sort_by(.seq) as $events | ($events|map(select(.code!="ok"))|first) as $first | {request:$r.id,status:(if $events[-1].code=="ok" then "recovered" else "failed" end),attempts:($events|map(.attempt)|unique|length),cause:(if $first.code=="timeout" then (if $c[0][$first.config].timeout_ms<$r.budget_ms then "configuration_timeout" else "deadline" end) elif $first.code=="upstream_502" then "upstream" else "unknown" end),evidence:[$events[].seq]}] | sort_by(.request) | {incidents:.,failed:([.[]|select(.status=="failed")]|length),recovered:([.[]|select(.status=="recovered")]|length)}' > result.json
      #|
    ),
  })
  if !basic {
    return base
  }
  let files = base.instance.files.copy()
  files["app.jsonl"] = ("\n" +
  (match files["app.jsonl"] {
    String(text) => text
    _ => abort("log")
  })).to_json()
  files["retries.jsonl"] = ("{malformed\r\n" +
  (match files["retries.jsonl"] {
    String(text) => text
    _ => abort("log")
  }).replace_all(old="\n", new="\r\n")).to_json()
  {
    ..base,
    instance: {
      ..base.instance,
      files,
      goal: base.instance.goal +
      " Skip blank or malformed JSONL lines. Evidence entries are {seq,file,line}, where file is app.jsonl or retries.jsonl and line is the physical 1-based line number including skipped lines. If an identical event has multiple locations, keep the lexically first file and then smallest line. Do not invent locators from event sequence numbers.",
      schema: "{incidents:[{request:string,status:string,attempts:integer,cause:string,evidence:[{seq:integer,file:string,line:integer}]}],failed:integer,recovered:integer}",
      reference: base.instance.reference
      .replace_all(
        old="jq -s 'unique_by(.request,.seq)'",
        new="jq -Rn '[inputs | input_filename as $file | input_line_number as $line | try (fromjson + {file:$file,line:$line}) catch empty] | sort_by(.file,.line) | unique_by(.request,.seq)'",
      )
      .replace_all(
        old="evidence:[$events[].seq]",
        new="evidence:[$events[]|{seq,file,line}]",
      ),
    },
  }
}

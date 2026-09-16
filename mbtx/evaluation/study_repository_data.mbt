///|
fn repository_data_case(index : Int, v : Int) -> StudyCase {
  let n = 19 + v * 3
  match index {
    0 | 2 => {
      let files : Map[String, Json] = Map([])
      let paths : Array[String] = []
      let symbols : Array[String] = []
      for i in 0..<n {
        let path = "src/group\{i % 4}/file\{1000 + i}.mbt"
        let symbol = "export_\{v}_\{1000 + i}"
        files[path] = ("pub fn " +
        symbol +
        "() -> Int { \{i} }\nfn hidden() -> Int { 0 }\n").to_json()
        paths.push(path)
        symbols.push(symbol)
      }
      files["src/generated/ignored.mbt"] = "pub fn ignored() -> Int { 0 }\n".to_json()
      files["src/notes.txt"] = "pub fn not_code()\n".to_json()
      paths.sort_by(codepoint_compare)
      symbols.sort_by(codepoint_compare)
      if index == 0 {
        {
          goal: "Index all .mbt files recursively under src except src/generated. Return lexically sorted relative file paths and unique function names on lines beginning 'pub fn '. Ignore other extensions and private functions.",
          schema: "{files:string[], exported:string[]}",
          files,
          expected: { "files": paths.to_json(), "exported": symbols.to_json() },
          reference: (
            #|rg --files src -g '*.mbt' -g '!**/generated/**' | jq -Rs 'split("\n")|map(select(length>0 and (startswith("src/generated/")|not)))|sort' > paths.json
            #|rg --no-filename --no-heading '^pub fn ' src -g '*.mbt' -g '!**/generated/**' | jq -Rs 'split("\n")|map(select(startswith("pub fn "))|capture("pub fn (?<name>[^ (]+)").name)|unique' > symbols.json
            #|jq -n --slurpfile p paths.json --slurpfile s symbols.json '{files:$p[0],exported:$s[0]}' > result.json
            #|
          ),
        }
      } else {
        let selected = paths.filter(p => {
          p.contains("group1/") || p.contains("group3/")
        })
        {
          goal: "List relative paths of .mbt files recursively under src/group1 and src/group3, excluding all other directories and extensions. Return lexically sorted paths and their count. Missing selected directories contribute no files.",
          schema: "{files:string[], count:integer}",
          files,
          expected: {
            "files": selected.to_json(),
            "count": selected.length().to_json(),
          },
          reference: (
            #|rg --files src | jq -Rs 'split("\n")|map(select(test("^src/group[13]/.*\\.mbt$")))|sort|{files:.,count:length}' > result.json
            #|
          ),
        }
      }
    }
    1 => {
      let nodes : Array[Json] = []
      let ordered : Array[String] = []
      for i in 0..<n {
        let name = "pkg\{1000 + i}"
        ordered.push(name)
        let deps : Array[String] = if i == 0 {
          []
        } else if i > 1 && i % 3 == 0 {
          ["pkg\{999 + i}", "pkg\{998 + i}"]
        } else {
          ["pkg\{999 + i}"]
        }
        nodes.push({ "name": name.to_json(), "deps": deps.to_json() })
      }
      nodes.rev_in_place()
      study_case(
        "Read the acyclic package graph in input.json. Produce a topological ordering: dependencies before dependents; when several packages are ready select the lexically smallest name. Each name occurs once and dependencies reference existing names.",
        "string[]",
        nodes.to_json(),
        ordered.to_json(),
        "def topo($done): if length==0 then $done else . as $nodes | (map(select((.deps-$done)|length==0))|sort_by(.name)|.[0].name) as $next | ($nodes|map(select(.name!=$next)))|topo($done+[$next]) end; topo([])",
      )
    }
    3 => {
      let records : Array[Json] = []
      let totals : Map[String, Int] = Map([])
      let mut total = 0
      for i in 0..<(n * 5) {
        let customer = ["alpha", "beta", "雪", "empty"][i % 4]
        let q = (i + v) % 7
        let price = (i * 3 + v) % 19 - 3
        records.push({
          "customer": customer.to_json(),
          "quantity": q.to_json(),
          "price": price.to_json(),
        })
        totals[customer] = totals.get(customer).unwrap_or(0) + q * price
        total += q * price
      }
      study_case(
        "Sum quantity*price by customer and overall from input.json using signed integer arithmetic. Retain customers whose total is zero; negative prices are credits.",
        "{customers:{name:integer},total:integer}",
        records.to_json(),
        { "customers": totals.to_json(), "total": total.to_json() },
        "reduce .[] as $r ({customers:{},total:0}; ($r.quantity*$r.price) as $v | .customers[$r.customer]=((.customers[$r.customer]//0)+$v) | .total+=$v)",
      )
    }
    4 => {
      let users : Array[Json] = []
      let orders : Array[Json] = []
      let expected : Array[Json] = []
      for i in 0..<n {
        users.push({ "id": i.to_json(), "name": "user\{v}_\{i}".to_json() })
      }
      for i in 0..<(n * 2) {
        let id = (i * 7 + v) % (n + 3)
        let value = i * 3 - v
        orders.push({
          "id": i.to_json(),
          "user_id": id.to_json(),
          "amount": value.to_json(),
        })
        expected.push({
          "id": i.to_json(),
          "name": if id < n {
            "user\{v}_\{id}".to_json()
          } else {
            Json::null()
          },
          "amount": value.to_json(),
        })
      }
      orders.rev_in_place()
      study_case(
        "Left-join orders with users by user_id=id in input.json. Keep every order, preserve amount, use null for unknown users, and sort by numeric order id. User and order ids are unique within each table.",
        "[{id:integer,name:string|null,amount:integer}]",
        { "users": users.to_json(), "orders": orders.to_json() },
        expected.to_json(),
        ".users as $users | .orders | sort_by(.id)|map(. as $o|{id,amount,name:([$users[]|select(.id==$o.user_id)|.name][0]//null)})",
      )
    }
    _ => {
      let records : Array[Json] = []
      let sums : Array[Int] = [0, 0, 0, 0, 0]
      let counts : Array[Int] = [0, 0, 0, 0, 0]
      for i in 0..<(n * 4) {
        let time = (i * 17 + v) % 300
        let amount = (i + v) % 13 - 5
        records.push({ "second": time.to_json(), "value": amount.to_json() })
        sums[time / 60] += amount
        counts[time / 60] += 1
      }
      let expected : Array[Json] = []
      for i in 0..<5 {
        expected.push({
          "start": (i * 60).to_json(),
          "count": counts[i].to_json(),
          "sum": sums[i].to_json(),
        })
      }
      study_case(
        "Bucket integer timestamps in input.json into five half-open windows [0,60), [60,120), ..., [240,300). Return every window even if empty, ordered by start; count records and sum signed values.",
        "[{start:integer,count:integer,sum:integer}]",
        records.to_json(),
        expected.to_json(),
        ". as $rows | [range(0;5) as $i|[$rows[]|select(.second >= $i*60 and .second < ($i+1)*60)]|{start:($i*60),count:length,sum:(map(.value)|add//0)}]",
      )
    }
  }
}

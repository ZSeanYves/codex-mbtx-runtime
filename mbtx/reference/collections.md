# Strings, collections and JSON

A `String` is owned. `split()` returns an iterator of `StringView`; materialize
with `.to_array()` for indexing or reuse, and use `.to_owned()` to store a view
as `String`. `trim()` returns a view too. Use `text.iter()` for characters;
UTF-8 byte offsets are not character indices. `text.has_prefix(prefix)` tests
prefixes. Preserve CRLF or normalize it only according to the task contract.
`text.replace(old=..., new=...)` replaces the first occurrence; use
`text.replace_all(old="\r\n", new="\n")` to normalize every CRLF.

Create maps as `let counts : Map[String, Int] = Map([])`. Read with
`counts.get(key)` (`Int?`), default with `.unwrap_or(0)`, set with
`counts[key] = value`, remove with `counts.remove(key)`. Iteration is
`for key, value in counts { ... }`. Map iteration is not a sorted output order.

Arrays use `items.push(value)`, `items.length()`, `items[i]`, and `items.sort()`
(in place). Use `sort_by(fn(a, b) { ... })` for an explicit order. MoonBit String
comparison is shortlex: tasks requiring Unicode code-point lexicographic order
must compare `a.iter().to_array()` and `b.iter().to_array()` character by
character, then lengths. The complete tested comparator is in
`reference:examples`; jq sorting is also useful when it matches the task.

Import `moonbitlang/core/json`. Parse with `@json.parse(text)` (raises), serialize
with `value.stringify()`. Annotate mixed JSON objects as `Json`, for example
`let row : Json = { "name": name.to_json(), "count": count.to_json() }`.
Pattern-match with `{ "name": String(name), "count": Number(count, ..), .. }`;
validate missing fields/type errors in a fallback. JSON `null` matches `Null`.
The executable reference shows typed construction and exhaustive matching.

For complex JSON filtering, grouping or sorting, an allowed direct jq call can
avoid a custom parser and many API probes. Use `--arg` for strings, `--argjson`
for JSON, stdin for documents, and inspect the exit status before parsing output.

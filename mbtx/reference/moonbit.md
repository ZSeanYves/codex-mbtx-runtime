# MoonBit execution reference

This reference documents the installed compiler and `moonbitlang/async@0.21.3`,
using the Wasm target with `moonrun`. It contains general APIs, not task solutions.
The short-tool-description / local-reference / executable-example organization
follows OpenSeek commit `6d4a35b4eabf71dd10b87d003d7b31fef2960161`.
OpenSeek's different permissions and tool contracts do not apply here.

Pass exactly one of `source` or `filename` to `mbtx`. The latter reads an existing
UTF-8 file relative to `cwd`; edit it with the ordinary file editing tool. Each
execution starts a fresh process. Dependencies and successful compilations may
be reused within this session. `cache` and `build_reused_from` describe reuse.

## Imports and entry points

Use versioned single-file imports, for example:

```moonbit
import {
  "moonbitlang/async@0.21.3",
  "moonbitlang/async@0.21.3/fs",
  "moonbitlang/async@0.21.3/shell",
  "moonbitlang/core/json",
}
async fn main {
  let data = @json.parse(@fs.read_file("input.json").text())
  @fs.write_file("output.json", data.stringify() + "\n")
}
```

IO functions suspend and raise errors: call them from `async fn`. A synchronous
throwing entry point is `fn main raise { ... }`. `try { ... } catch { error =>
... }` handles an error; `fail("message")` raises one. Read compiler diagnostics
before changing APIs or imports. No network dependency installation is allowed.

## JSON, strings and collections

Annotate heterogeneous JSON as `Json`: `let row : Json = { "name": name.to_json(),
"count": count.to_json() }`. Use `@json.parse(text)` and `value.stringify()`.
Pattern matching validates shape and binds values:

```moonbit
let value : Json = { "name": "sample", "count": 3 }
match value {
  { "name": String(name), "count": Number(count, ..), .. } => {
    println(name)
    println(count.to_int())
  }
  _ => fail("expected name and count")
}
```

A `String` is already owned. `split` returns an iterator of `StringView`; call
`.to_array()` before indexing or iterating repeatedly, and `.to_owned()` when a
stored `String` is needed. `trim()` also returns a view. Preserve UTF-8; byte
offsets are not character indices. Avoid inventing JavaScript or Rust methods.

`let counts : Map[String, Int] = Map([])` creates a map; use `counts.get(key)` for an
`Int?`, `counts.get(key).unwrap_or(0)` for a default, and `counts[key] = value`
to update it. `for key, value in counts { ... }` iterates entries. Use arrays
for ordered output; map iteration does not define a sorted output order.
`array.sort()` sorts in place. MoonBit String comparison uses shortlex; tasks
requiring Unicode code-point lexicographic order need an explicit comparator
over `string.iter().to_array()`, comparing characters, then lengths.

## Files and child processes

`@fs.read_file(path).text()` reads UTF-8, `@fs.write_file(path, text)` writes,
`@fs.exists(path)` checks existence, and `@fs.mkdir(path, recursive=true)` creates
directories. Paths are relative to the actual working directory.

`@shell.Cmd(program, argv).output()` returns a child result. Check
`.exit_code()`, then read `.stdout()` and `.stderr()`. Arguments are literal;
shell expansion requires explicitly running `sh -c`. `.run()` raises on a
nonzero exit. Use `stdin=@shell.Text(text)` to supply text and EOF.
`output(max_output_bytes=N)` explicitly bounds the library's capture; this is
different from the host's model-context preview. For large child output, choose
a sufficient library limit or redirect it to files for streaming inspection.
The runtime cannot recover bytes a user program itself discarded.

## Diagnostics and output

Build and run results are separate. A nonzero compiler exit never starts the
program. A successful program may still have an incorrect task result.
The host preserves full streams separately from bounded previews. Read each
issued output ID with `read_resource`, following `next_offset` until `eof`.
The page reports byte ranges; use the returned offset rather than adding a
character count. `complete=false` means the underlying evidence is incomplete.
Reference reads do not compile MoonBit and count as ordinary tool calls.

Build and run timeouts are per tool invocation. They do not finish the task;
the model decides whether and how to recover. No oracle feedback is supplied.
`examples.mbtx` in this frozen reference bundle exercises the APIs above; its
successful build and execution are prerequisites for publishing the bundle.

# Syntax and checked errors

Standalone files declare their imports, for example the runnable programs in
`reference:examples` and `reference:tool-example`. Import `moonbitlang/async@0.21.3`
and each used subpackage (`.../fs`, `.../shell`); core packages such as
`moonbitlang/core/string` and `moonbitlang/core/json` are unversioned.

- `async fn main { ... }` may call async IO and throwing functions.
- A synchronous throwing function declares `fn parse(text : String) -> Int raise`.
  Its synchronous entry point is `fn main raise { ... }`.
- Use `try { expression } catch { error => ... }` to recover, or
  `fail("message")` to raise an error. `guard condition else { fail("reason") }`
  validates an invariant; propagate the actual child stderr on failure.
- `let mut n = 0` permits rebinding; `n = n + 1`. Arrays and maps can be updated
  without declaring their bindings mutable.
- After importing `moonbitlang/core/string`, use `@string.parse_int(text.trim())`
  or `let n : Int = @string.from_str(text.trim())`. Parsing raises on invalid
  text or overflow. `@string.parse_int64`, `parse_uint`, and `parse_double`
  are available. There is no `String.to_int()` or `Int.parse()`.
- A `Double` from JSON can use `.to_int()` only after the program has checked
  that truncation and range are acceptable. Parse exact large integers from
  text with the required width, or use an allowed utility suited to the task.
- Match a tuple as `match (a, b) { ... }`; write exhaustive fallbacks when
  validating external input. Do not guess JavaScript/Rust method names.

`reference:examples` compiles and executes these parsing and error patterns.

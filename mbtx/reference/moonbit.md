# Frozen MoonBit API index

The Wasm runtime uses `moonbitlang/async@0.21.3`. These references and executable
examples are checked with the bundled compiler. They contain no task answers.
The compact prompt/reference/example organization draws on OpenSeek commit
`be29e280ae3e75ec907a93fcd248923e054f6728`; its permissions do not apply here.

Read the relevant topic before guessing an unfamiliar API. Request a sufficient
page size (up to 65536 bytes); related APIs are grouped so one read usually
answers several questions. The host applies the shared context budget.

| Resource | Contents |
|---|---|
| `reference:syntax` | Versioned imports, async/raise, typed integer parsing, checked errors |
| `reference:files` | Read/write, directory listing, removal, paths and binary data |
| `reference:collections` | StringView, maps, sorting, JSON construction and patterns |
| `reference:processes` | Direct exec, stdin, literal argv, exit status, safe rg and jq |
| `reference:tools` | Utility conventions; the task determines the allowed programs |
| `reference:examples` | Complete runnable language/file/JSON example |
| `reference:process-examples` | Complete runnable rg and jq orchestration examples |
| `reference:tool-example` | The same short program included in the mbtx tool description |
| `reference:shell` | Shell-arm documentation; it does not authorize MBTX interpreters |

Pass exactly one of inline `source` or existing `filename` to `mbtx`. The source
limit is 65536 UTF-8 bytes. Each run has a fresh heap and process; dependencies
and identical compiled programs can be cached. `cache` and `build_reused_from`
identify reuse. Preserve a delivery source only when the task asks for one.

Use MoonBit for control flow, combination and error handling. When allowed,
use jq for substantial JSON transformations and rg for searches instead of
reimplementing their parsers. Never invoke a shell or another script host.

`compiler_preview` is a derived, error-first diagnostic. Exact source paths are
shortened to `source:line:column`; warnings may be summarized. `scan_complete`
and `display_complete` say whether scanning or presentation is incomplete.
Raw build/run stream resources remain unchanged: read their IDs with
`read_resource` and follow `next_offset` until `eof`. Offsets are UTF-8 bytes.
An underlying resource receipt with `complete=false` is incomplete evidence.

Compilation failure never starts the program. Successful execution does not
prove task correctness. Evaluation calls share the attempt deadline, including
delivery verification; no oracle feedback is exposed to the model.

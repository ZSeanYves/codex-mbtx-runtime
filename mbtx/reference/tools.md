# Installed public utilities

Both arms may use the same installed `sh`, `jq`, `rg` and `git`. Paths and binary
hashes are frozen in the run manifest. Network access and delegation are disabled.
No task-specific answer or oracle API is exposed by these references.

- `jq` parses JSON. `jq -s` reads several JSON inputs into an array. `jq -r`
  emits unquoted strings; `jq -n --arg name "$value" '$name'` constructs JSON
  safely. `--argjson` expects valid JSON. Inspect each command's exit status.
- `rg -n PATTERN PATH` searches text with line numbers. `rg --files PATH`
  enumerates files under the ordinary ignore rules. Use explicit paths/options
  when a task requires a different file population. A no-match exit is 1.
- `git status --porcelain`, `git diff`, and `git ls-files` inspect the isolated
  task repository. The initial baseline is identical for both arms. Do not
  inspect parent repositories, personal configuration or other attempts.
- `sh -c SCRIPT` interprets Shell syntax; direct argv execution does not.
- `./fixture-worker` is a fixed evaluation utility for tasks explicitly asking
  for controlled work. Its task-visible command contract accompanies the input.
  It contains no evaluation oracle or private answers.

File permissions, approval, sandboxing and cancellation remain Codex's
responsibility. Utility stderr is diagnostic evidence, not a task oracle.

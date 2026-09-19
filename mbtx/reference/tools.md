# Task-declared direct utilities

The current task lists allowed programs and argument forms. Policy and this
list come from the same declaration; installed tools are not automatically
allowed. Paths/hashes are frozen in the bundle. No oracle or private answer
utility is available. Network access and delegation are disabled.

- jq handles JSON parsing, filtering, grouping and sorting. `-s` combines JSON
  inputs; `-r` emits strings; `-n` constructs values without input. Use literal
  `--arg name value` and `--argjson name valid_json`; check exit status.
- rg uses `--no-config --json -- PATTERN PATH` or `--no-config --files -- PATH`
  in MBTX. A no-match status is 1. Ordinary ignore rules apply.
- Git is available only where explicitly declared, restricted to specified
  read operations. Do not inspect parent repositories or personal config.
- fixture-worker supports only task-declared actions. Its public contract
  accompanies the input; it does not return evaluation answers.
- Shell execution and Shell-specific documentation belong to the Shell arm.

Use MoonBit for control flow and error handling around these tools. Substantial
JSON processing need not be reimplemented by hand. See `reference:processes`
and `reference:process-examples` for exact direct-call patterns.

Do not use preprocessors, hooks, aliases, pagers, external diff or generic
forwarders to launch programs indirectly. Process stderr is diagnostic evidence,
not an oracle. Codex retains filesystem, cancellation and OS sandbox authority.

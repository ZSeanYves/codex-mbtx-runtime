# Shell execution reference

The Shell arm uses Codex `exec_command` and `write_stdin`. Login initialization
is disabled. Complete POSIX sh scripts are allowed; no prescribed number of
commands, processes or model steps applies. Common file editing tools are
available to both arms. Read tasks carefully and verify produced files.

Quote variables and paths: `printf '%s\n' "$value"`, `sh ./program.sh`.
Use `set -eu` when unhandled failures should stop a script, but handle expected
failures explicitly (`if command; then ...; else ...; fi`). A pipeline's last
status is not evidence that every stage succeeded. POSIX sh does not guarantee
`pipefail`. Preserve stdout and stderr separately with `>out.txt 2>err.txt`.

A long-running `exec_command` may return a session ID. Poll with `write_stdin`;
send EOF or interruption only when appropriate. A poll is a tool call, not
automatically a new model decision. Child completion, output drain and an
accepted final model answer are separate events.

The context preview has a bounded token allowance; reaching it does not stop
the process. Full stream resource IDs are included in execution responses.
Read them with `read_resource(resource_id, offset, max_bytes)` and follow
`next_offset` until `eof`. Only this session's registered resources are readable.
PTY output is one combined stream; do not claim stdout/stderr separation there.

The available public utility reference is `reference:tools`. MoonBit's language
reference is also available as `reference:moonbit`; reading it is optional.

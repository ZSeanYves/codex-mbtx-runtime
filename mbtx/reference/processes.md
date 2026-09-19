# Direct process orchestration

Import `moonbitlang/async@0.21.3/shell`. `@shell.Cmd(program, args)` directly
executes the named program with literal argv. The package name does not invoke
Shell; quotes, pipes, variables, redirects and wildcards in arguments have no
Shell meaning. Use only current-task programs and argument prefixes.

`Cmd(...).output()` captures stdout/stderr; check `.exit_code()` before using
`.stdout()` or `.stderr()`. `.run()` raises on nonzero exit. Use
`stdin=@shell.Text(text)` to supply text and EOF; `@shell.Binary(bytes)` preserves
bytes. `stdout=@shell.ToFile(path)` redirects without Shell. Capture has a library
limit: set `output(max_output_bytes=N)` for known larger outputs. The host's
resource archive cannot recover output discarded inside the user program.

Safe search forms are literal arrays:
- `Cmd("rg", ["--no-config", "--json", "--", pattern, path])`
- `Cmd("rg", ["--no-config", "--files", "--", path])`
Treat rg exit 0 as matches, 1 as no matches, other statuses as errors. `--` ends
options, so patterns or paths cannot enable preprocessors. Parse JSON result
lines if needed; no shell pipeline is required.

For jq, use `Cmd("jq", ["-c", "--arg", "label", label, "--argjson", "minimum",
minimum.stringify(), filter], stdin=@shell.Text(document))`. Each array element
is one argument. Do not insert Shell quotes around the filter or interpolate
untrusted values into its syntax. Check exit status, then parse stdout.

`reference:process-examples` contains the complete verified rg search and jq
composition programs. They demonstrate generic orchestration, not task answers.
Do not use `sh/bash/dash`, Python/Node, unregistered paths, worker `launch`, or
utility features that invoke another program. The official MoonRun policy
constrains direct requests; do not assume complete descendant-process auditing.

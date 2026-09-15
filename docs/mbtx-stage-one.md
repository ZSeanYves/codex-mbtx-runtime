# Programmable MBTX: Stage One

This document describes the first executable implementation of the
[architecture](mbtx-architecture.md). It validates a programming tool's execution
boundary. It does not measure agent-step savings or establish a performance
advantage over Shell.

## Scope and ownership

`codex-mbtx-extension` registers the optional `mbtx` function through the upstream
extension registry. Both `codex exec` and the TUI use the app-server registry.
The extension owns argument validation, source preparation, compiler invocation,
artifact selection, and a bounded structured result. It does not depend on
`codex-core`, an evaluator, or the old launcher repository.

The `codex-tools` process capability connects extensions to the host. Its core
implementation uses the existing exec-policy manager, `ToolOrchestrator`,
sandbox transformation, environment policy, and child-spawning primitive.
It does not call a model-visible Shell tool or create a second session manager.

The host adds a bounded one-shot capture policy. It observes the direct child's
wait status, retains separate streams, drains them to EOF, and cleans the owned
process group. Existing Shell capture behavior remains the default.

## Target decision

The selected target is **linear-memory Wasm (`wasm`) with official `moonrun`**.
No automatic target fallback is supported.

The capability fixture was compiled and executed using both `native` and `wasm`
on macOS ARM64. Both preserved empty and special-character arguments, UTF-8 file
contents, cwd, an environment value, separate stdout/stderr, JSON operations,
and a real child program. The Wasm artifact also executed inside Codex's macOS
sandbox. Wasm avoids a C compiler/linker in each submitted program's build path.
This is a deployment decision, not a speed comparison. `wasm-gc`, JavaScript,
LLVM, and Windows have not been accepted as interchangeable targets.

The prototype used:

| Component | Observed version |
| --- | --- |
| Upstream baseline | `31ffe2bc9adccfe5fd3d29208250f796a13aa7a0` |
| Rust | Repository-pinned `1.95.0`, `aarch64-apple-darwin` |
| moon / moonrun | `0.1.20260904`, revision `94521db` |
| moonc | `v0.10.12+1634b282e`, 2026-09-07 |
| Capability imports | `moonbitlang/async@0.21.3`, installed core |

The unmodified baseline CLI built successfully with `cargo build --locked -p
codex-cli --bin codex -j 4`, `CARGO_PROFILE_DEV_DEBUG=0`, and
`CARGO_INCREMENTAL=0`. The timed invocation reported 260.17 seconds elapsed;
dependencies already existed locally, so this is not an empty-machine cold-build
benchmark. The resulting binary reported `codex-cli 0.0.0`.

## Configuration and execution

Install the official MoonBit toolchain, the repository's Rust toolchain and
Python 3 (required by the workspace justfile). Ensure `moon`, `moonrun`, `cargo`,
`just`, `cargo-nextest`, and `python3` are on PATH. Install the test helpers once,
as described in the upstream [build instructions](install.md):

```console
source "$HOME/.cargo/env"
cargo install --locked just
cargo install --locked cargo-nextest
just --version
cargo nextest --version
python3 --version
```

Prepare the fixture's dependencies once, from the repository root:

```console
moon build --target wasm --target-dir _build/mbtx-capabilities mbtx/fixtures/capabilities.mbtx
```

Then configure the host using absolute paths:

```toml
[mbtx]
enabled = true
moon = "/absolute/path/to/moon"
moonrun = "/absolute/path/to/moonrun"
dependency_cache = "/absolute/path/to/.moon/cache/deps"
```

Use `command -v moon` and `command -v moonrun` to find the executables. The usual
dependency-source cache is `$HOME/.moon/cache/deps`; an installation using
`MOON_HOME` or `MOON_DEP_CACHE` may have a different location. The
[example](../mbtx/config/example.toml) contains placeholders, not machine-specific
credentials. Missing executables, non-executable files, missing cache paths and
unknown MBTX configuration fields fail explicitly. With `enabled` omitted or
false, upstream Shell behavior is unchanged.

The model calls `mbtx` with a complete `.mbtx` source string, for example:

```json
{
  "source": "fn main { println(\"hello from MoonBit\") }",
  "argv": [],
  "build_timeout_ms": 60000,
  "run_timeout_ms": 10000,
  "max_output_bytes": 1024
}
```

Use versioned imports for installed libraries, such as
`"moonbitlang/async@0.21.3"` and `"moonbitlang/async@0.21.3/fs"`. An async main
also requires the root async import. The capability fixture is a complete
example. Source bytes are preserved in a fresh `.codex-mbtx/<UUID>/program.mbtx`
under the selected cwd. The result links to that source and the generated Wasm
artifact. Each call receives a fresh program instance; files preserve task state.

The compiler command uses `moon build --frozen --target wasm --release`; the
runtime command uses `moonrun -- ARTIFACT ARGV...`. Argument strings are passed
as an argv vector, including empty strings, leading hyphens, spaces and metacharacters.
stdin is EOF. Interactive input and persistent background sessions are outside
this stage. Validation exercises direct tool mode (`features.code_mode = false`,
the inspected upstream default). Code Mode integration is not part of this
stage's acceptance evidence.

Moon's frozen build still takes a writable dependency-cache lock. Consequently,
the extension copies the configured preinstalled cache into invocation scratch
through the host filesystem capability and sets compiler-only `MOON_DEP_CACHE`
and `MOON_BUILD_CACHE` overrides. It does not grant write access to the user's
global cache. The program receives the original host environment policy.
The [Moon command manual](https://moonbitlang.github.io/moon/commands.html) and
[toolchain path implementation](https://github.com/moonbitlang/moon/blob/94521db/crates/moonutil/src/moon_dir.rs)
describe the underlying frozen/path controls.

Dependency snapshot copying is preparation work in this prototype. There is no
cross-invocation program cache or performance claim. Cache/bundle hashing,
immutable shared prepared dependencies and reuse belong to the later build
bundle stage. Source imports absent from the supplied snapshot fail instead of
being installed during a tool call.

## Authority and lifecycle

Source and dependency copying use the selected environment's filesystem
capability and sandbox context. Compilation and execution independently pass
through host execution policy and approval. Approval applies to the displayed
compiler/program invocation and its source file within the effective sandbox;
it is not per-statement approval or a claim of Shell-prefix policy equivalence.
A program can perform permitted file and child-process operations, just as
other approved interpreters can.

The extension does not request automatic escalation or retry a program after
failure. Such retries could duplicate file changes. Denied compilation produces
no runtime invocation. Both stages retain independent output and failure state.

On cancellation or timeout the host sends TERM to the owned group, permits up
to 500 milliseconds for the leader to terminate, and sends KILL when needed.
It waits for the direct child and drains stdout/stderr before returning a
successful capture. Reaping and drain each have a two-second bound. Outstanding
group members are killed on normal leader exit too: this tool is one-shot.
Drain failure is an explicit error, not a fabricated empty successful stream.

An extension may request a bounded dispatch cleanup grace. Whole-turn
interruption honors that request as well; otherwise the upstream 100-millisecond
abort would cut cleanup short. MBTX requests eight seconds and the host caps
extension requests at ten seconds. Only an actually dispatched tool can extend
that turn's grace; enabling the extension alone does not. This is cleanup time,
not extra program execution time.

`exit_code` is the observed numeric exit, or null on signal termination.
`signal` is the observed signal, or null for normal exit. Numeric 143 therefore
differs from SIGTERM. Timeout/cancellation classification is independent of
those fields. Exit observation is not a kernel exit timestamp. Process-group
cleanup does not prove that arbitrary descendants which create a new session
have been reaped; deliberately detached work remains an unsupported boundary.

Only local Linux/macOS execution is admitted. Remote or multiple selected
environments, Windows, and managed network proxies currently return explicit
unsupported errors. These restrictions never cause an unsandboxed fallback.

## Result bounds

The source limit is 65,536 UTF-8 bytes; argv allows 256 entries totaling 32,768
bytes, with NUL rejected. Build and run budgets default to 60 and 10 seconds,
clamped to 120 and 60 seconds. A call delivers at most 4,096 combined raw output
bytes across compilation and execution, default 1,024. Strings use UTF-8 lossy
decoding; this interface is not a binary output archive.

Serialization is separately bounded to 8,192 bytes and the host context budget.
Further shortening sets the relevant stream's truncation flag. Build output
consumes part of the combined delivery budget; programs should write larger
results to permitted files. A metadata-only result that cannot fit the host
budget fails explicitly. Review of this model-visible result includes escape
expansion, UTF-8 boundaries, and preservation of null observations.

## Reproducible validation

From the repository root, after dependency preparation:

```console
moon run mbtx/scripts/validate.mbtx
```

The entry checks the working directory and required commands before starting
any build. Each stage prints its command name; failures preserve the original
error and include setup guidance where applicable. After a prerequisite is
installed, rerun the same entry: it retains the existing Cargo and MoonBit build
directories and does not force a clean rebuild.

The entry then builds the actual CLI, runs the configuration, shared tool and MBTX
unit suites plus the affected core configuration, exec, policy, dispatch and
extension-adapter tests, then executes the opt-in MBTX fixed-SSE integration
tests sequentially. It needs no
provider/API credentials. Tests which require MoonBit are ignored by normal
upstream test runs and explicitly selected by this entry; absence of the
toolchain in the opt-in run is an error.

The host lifecycle tests use deterministic real OS processes. The integration
tests use the upstream Codex test harness, actual MoonBit compilation and
runtime execution, and a local Responses server. They test execution semantics;
their prescribed model trajectory cannot demonstrate a reduction in agent steps.
The denied-write case covers both MoonBit file operations and a spawned native
child, so confinement is tested beyond the Wasm host's own file API.

Broader upstream tests are a separate regression check. The first complete
changed-library run executed 2,955 tests: 2,947 passed, seven failed and one
timed out. Review identified a new session-retention defect in the process
capability; it now stores a weak invocation reference and refuses calls after
the invocation ends. The shell credential-snapshot timeout passed when isolated.

Six other failures were reproduced without changes in a separate checkout of
the exact upstream baseline. They are the three agent-control input/spawn
tests, two multi-agent input tests, and
`interrupting_compaction_fallback_retains_last_known_step_context`. The same
user-message/compaction timeouts occur there with one test thread. This
establishes that those failures are not introduced by MBTX; it does not diagnose
their upstream cause or establish a completely green upstream suite.

A subsequent full-workspace `just test --test-threads=4 --retries=0` was
attempted. Compilation stopped in `glib-sys`, reached through the unchanged
`codex-voice-host` GStreamer dependency, because this Mac lacks `pkg-config`.
The full-workspace tests therefore did not execute; they are not recorded as
passed. This system dependency is not required by the MBTX tool or its CLI
build. No upstream test or feature was disabled to bypass the failure.

The app-server and four existing extension suites ran 1,929 tests: 1,890 passed,
35 failed and four timed out, with one additional test skipped. Building the
upstream MCP fixture binaries with `cargo build --locked -p codex-rmcp-client
--bins` resolved 25 missing-binary failures; all 25 passed on targeted recheck.
Ten Code Mode-related cases and four ZshFork timeout cases remain outside the
successful regression evidence. The latter have not been causally diagnosed.

An attempted build of `codex-code-mode-host` failed because the pinned
`v8 150.4.0` build downloaded
`librusty_v8_ptrcomp_sandbox_release_aarch64-apple-darwin.a.gz` from its upstream
GitHub release and received HTTP 404. No V8 dependency was repinned or built
from source for this stage. The directly invoked MBTX tool does not require
that companion executable. These observations are local build/test limitations,
not Linux validation or evidence of a programmable tool's benefit.

Review the change in three parts: the shared process capability and host
lifecycle bridge; the optional config and product extension; then the real
integration fixtures, validation entry and documentation. The first part is
generic host infrastructure; the product crate depends only on lower-level
contracts and exports `install`. Evaluation and observability packages have not
been activated.

Local validation results and the second review are recorded after the final
implementation checks. Linux execution must be repeated on Linux before making
platform coverage claims. Step accounting, evaluator activation, live relay
collection, SigNoz ingestion and research reports remain later stages.

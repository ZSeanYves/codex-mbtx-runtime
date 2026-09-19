# Codex with programmable MBTX

An experimental fork of [OpenAI Codex](https://github.com/openai/codex), maintained at [ZSeanYves/codex-mbtx-runtime](https://github.com/ZSeanYves/codex-mbtx-runtime). Upstream copyright, attribution and the [Apache-2.0 license](LICENSE) are retained. Upstream npm packages do **not** contain this fork's MBTX integration; build this repository to reproduce the experiment.

We study whether **MoonBit programs can replace Shell programming and orchestration inside the same Codex workflow**, and how this affects model decision rounds, task completion and tool trajectories. This is a local research implementation, not a plugin distribution or a general security product.

## Comparison and execution boundary

The two arms receive the same task inputs, goals, model configuration, external utilities and common output budget. Shell uses Codex's execution tools. MBTX uses the `mbtx` extension, file editing and bounded `read_resource`; Shell and unified-exec tools are disabled in the MBTX arm and checked in captured model requests.

MBTX compiles MoonBit to Wasm and runs it with official MoonRun `--policy`, inside the existing Codex OS sandbox. `@shell.Cmd(program, args)` starts a program with literal arguments; it does not interpret Shell syntax. MoonBit provides control flow and error handling, and may use task-declared `jq`, safe `rg` forms and `fixture-worker` actions. Git is not registered by default. Shell, Python, Node, arbitrary paths and forwarding entries such as worker `launch` are denied. Tool declarations generate both policy and task guidance. The host PATH resolves to recorded, read-only utility entries.

Policy constrains **direct process requests**. We prohibit indirect execution features, but do not claim comprehensive control or auditing of every descendant process. Codex's filesystem/network sandbox remains active; ordinary IP networking is not opened. Worker receipts use the existing AF_UNIX channel. Native policy denial stderr is observed evidence; unreported argv, request identity and complete process totals remain unknown.

## Architecture

| Location | Responsibility |
|---|---|
| `codex-rs/ext/mbtx` | Tool schema, source/cache handling, compilation, MoonRun invocation, bounded resources and derived compiler previews |
| `codex-rs/core/src/tools/handlers/extension_process.rs` | Existing Codex execution/sandbox bridge, cancellation and remaining attempt budget |
| `codex-rs/config/src/mbtx.rs` | Host-configured MBTX paths and runtime policy |
| `codex-rs/mbtx-eval` | OS/HTTP adapters, frozen bundles, local pacing gate, worker receipts, exact-source validation and offline reports |
| `mbtx/evaluation` | Protocol, deterministic fixtures, independent oracles, decision accounting and descriptive analysis |
| `mbtx/reference` | Compact language index, topic references and executable examples |
| `mbtx/scripts` | Bundle preparation, one collection entry and offline validation |

Keep research rules in the MoonBit evaluator and OS/transport behavior in the Rust adapter. The evaluator does not depend on `codex-core`. Reference programs are private offline validation inputs; they are never supplied as task answers to a model. Full raw outputs remain archived even when the model receives a shortened, explicitly derived diagnostic preview.

## Build and offline validation

Linux is the supported measurement host. Install the repository-pinned Rust toolchain (`codex-rs/rust-toolchain.toml`, currently 1.95.0), `just`, `cargo-nextest`, the official MoonBit/MoonRun toolchain with `--policy` support, `bubblewrap`, `git`, `jq`, `rg`, Python 3 and Node.js. MoonBit dependencies are pinned in `mbtx/moon.mod`/`moon.lock`, including `moonbitlang/async@0.21.3`. Native dependencies for unrelated upstream crates are also needed for the full Rust workspace suite. Bundle metadata records exact compiler/runtime versions and hashes; a moving toolchain installation is not an interchangeable reproduction environment.

Run from the repository root:

```bash
unset MBTX_BUNDLE
moon run mbtx/scripts/validate-evaluation.mbtx --fast
moon run mbtx/scripts/validate-evaluation.mbtx
```

The fast path checks MoonBit and affected Rust crates. The full offline entry builds a fresh bundle, validates the exact reference sources, both-arm task references, local sandbox/policy/worker preflight, fixed-response replay, retry faults, interruption and deterministic report rebuilding. It does not contact a model provider. Replay uses a local Responses server and actual Codex/tool execution; prescribed solutions verify the harness, **not autonomous model performance**.

To prepare and inspect a bundle separately:

```bash
moon run mbtx/scripts/prepare-evaluation.mbtx
BUNDLE=$(cat _build/evaluation-bundle.txt)
"$BUNDLE/mbtx-eval" verify-bundle "$BUNDLE"
"$BUNDLE/codex" sandbox --help
```

The bundled Linux CLI must advertise `--allow-unix-socket`. Verify the executable in the selected bundle, not only workspace source. Preparation records the Git revision, dirty-tree/component fingerprints, compiled products, toolchain, reference and dependency hashes. Keep the matching source snapshot and external toolchain installation. A bundle alone is not a portable installation image.

Rust checks use `just test -p codex-mbtx-extension -p codex-mbtx-eval` from `codex-rs`. Shared core changes additionally require the relevant core integration cases and the complete `just test` suite. Finish Rust changes with scoped `just fix` and `just fmt`, following [AGENTS.md](AGENTS.md). Upstream test failures must be reported and reproduced separately; passing local MBTX checks does not imply the entire workspace passed.

## V4 task protocol

`programmable-shell-replacement-v4` has two independent tracks. Each fixed task has two repetitions; the second swaps arm order. Each attempt has a new workspace and session cache.

| Track | Tasks | Repeats | Pairs | Attempts |
|---|---:|---:|---:|---:|
| `basic` | 10 | 2 | 20 | 40 |
| `complexity` | 6 | 2 | 12 | 24 |
| `all` (default) | 16 | 2 | 32 | 64 |

The basic track covers repository impact, data reconciliation, incident investigation, configuration migration, dependent jobs, incremental recovery, output forensics, release verification, reusable data programs and reusable recovery programs. Each family has one representative task. Basic tasks have no difficulty label.

The complexity track has two workflows, each at three frozen tiers. All six require source delivery:

| Workflow | Quick | Standard | Deep |
|---|---|---|---|
| Event recovery | 12 logical records, 2 shards; duplicate/version selection, state repair and checkpoint | 24 records, 4 shards, 18 jobs in a 5-level branching DAG; transactions, source conflicts, cache validation and controlled retry | 48 records, 8 shards, 36 jobs in a 10-level DAG; schema/alias/tombstone handling, damaged tails, transitive invalidation and 3 checkpoint generations |
| Release repair | 4 modules, 6 candidates, 1 profile; actual hashes, repair and manifest | 12 modules, 30 candidates, 3 profile levels; rename, reverse dependencies, candidate precedence and normalization | 24 modules, 72 candidates, 6 profile levels; migration, compatibility dependencies, byte preservation, provenance, references and interrupted publication |

Approximate step targets are 5–20 / 20–50 / 50–100. These are **design targets**, not verified model results. Difficulty comes from interacting rules and state transitions. Offline checks cannot establish that a model needs a particular number of decisions.

Task design borrows executable delivery and recovery validation ideas from [Terminal-Bench](https://github.com/laude-institute/terminal-bench/tree/d28711d0da2675d0bb1d56de45ae5df6082438a3), and “fix failures while preserving correct behavior” from [SWE-bench grading](https://github.com/SWE-bench/SWE-bench/blob/02e7a74ffd0b707aab73d203fe87bdc7c76afc8e/swebench/harness/grading.py). Reference organization and examples draw on [OpenSeek](https://github.com/moonbitlang/openseek/tree/be29e280ae3e75ec907a93fcd248923e054f6728). We do not copy their tool stacks or claim comparable benchmark scores.

## Source delivery and measurement

Delivery requires exactly `solution.sh` or `solution.mbtx` for the assigned arm, at most 65,536 UTF-8 bytes, runnable without arguments from cwd and without auxiliary authored files. A wrong filename is `missing_source`; a correct workspace artifact cannot substitute for source delivery or a completed session.

MBTX delivery is compiled once, then run under the same policy generation logic on fresh visible and two withheld inputs. Withheld inputs change structure as well as values. Reusable recovery and all complexity cases actually run an unchanged second pass. Deep cases additionally receive evaluator-declared input updates while retaining prior state. Each phase keeps its own result, receipt and process evidence; an empty second-pass execution list cannot erase first-pass work.

One **step** is one observed model decision round. Tool calls, HTTP requests/retries, compilations, program starts, worker operations, recovery and errors are separate measurements. Validation has no model steps. Unfinished attempts retain all observed decisions and `steps_to_success = null`.

The 7,200-second total attempt deadline covers Codex startup through delivery validation. Compilation/runtime consume the remaining budget; no model-step, tool-call or request-count limit is imposed. Output/source/argv/path safety limits remain. Online collection is serial, with at least 15 seconds between requests, at most two HTTP and two stream retries. Raw 429 responses are retained; respect Retry-After, or wait at least 30 seconds if absent. Failed attempts are not silently replaced.

Reports include all assigned attempts, observed step lists/ranges/medians, within-task repeat differences and conditional step differences for successful pairs. They also show reference first reads/continuations, compilation failures, decisions before the first successful compile, runtime errors and external retries. Repetitions are not independent tasks. There is no pooled causal estimate across tracks or small-sample confidence claim. Attribution distinguishes sandbox/launcher, worker IPC, provider/relay, Codex scheduling, MBTX compilation/runtime, fixture/oracle, harness, timeout and censoring, with observed/inferred/unknown labels.

## Collection, resume and reports

V4 currently has **offline validation only**. No new online performance conclusion is claimed. Starting an online run is a separate, explicit action.

Credentials are supplied through the shell environment. To enter a key without placing it in shell history:

```bash
read -rsp 'OPENAI_API_KEY: ' OPENAI_API_KEY
export OPENAI_API_KEY
printf '\n'
```

Never put credentials in repository files, reports or command output. Select the provider/model in `mbtx/config/relay.toml` before freezing a run.

```bash
# Prescribed offline replay; no provider credential is needed.
moon run mbtx/scripts/collect-study.mbtx replay --suite all

# A future, separately authorized online run:
moon run mbtx/scripts/collect-study.mbtx relay --suite all
# Or use --suite basic / --suite complexity.
```

Collection prints the exact run directory and current task/arm/repetition/progress. Default output is `_build/programmable-study-v4/<platform>/<mode>/run-<timestamp>`. Online order is local sandbox/policy preflight, worker receipt, relay probe, then attempts. Preflight failure stops before provider access; relay failure stays external evidence. Do not disable preflight or loosen the sandbox to get past a failure.

Resume only with the original run, bundle, protocol, model/configuration, task selection, toolchain, utilities, pacing and cache conditions:

```bash
moon run mbtx/scripts/collect-study.mbtx relay --suite all \
  --output /absolute/path/to/original/run --resume
```

The manifest rejects mismatched resume conditions. Old protocols cannot be resumed with the new task generator. To rebuild an existing report, use **that run's frozen bundle**:

```bash
/path/to/frozen/bundle/mbtx-eval report /absolute/path/to/run \
  --bundle /path/to/frozen/bundle --format all
```

Reports are standalone offline HTML, JSON, Markdown and CSV under `RUN/reports/<id>/`. No SigNoz deployment is used. Raw evidence includes preflight stderr/process/result, policies, worker events, HTTP request/response bodies, Codex JSONL/stderr, traces, effective configuration, source/hash and submission phases. Authentication is excluded from captured HTTP metadata.

Reproducibility means verifiable frozen conditions, deterministic fixture/oracle replay and rebuildable reports. It does not promise identical online model decisions or success rates. V4 changes tasks, references and interaction together; differences from V3 cannot be attributed solely to policy or prompting.

## Historical results and retained evidence

The completed V3 pilot at `_build/programmable-pilot-v3/Linux/relay/run-1789785914915` used 12 tasks × 2 repeats × 2 arms. Shell succeeded in 22/24 attempts, MBTX in 24/24. All 48 sessions completed. Shell observed 269 decisions (median 10, range 5–25); MBTX observed 358 (median 15.5, range 4–35). MBTX had 78 compilation failures; four delivery attempts accounted for 77 of the 89 additional decisions. The trace supported API guessing, diagnostic pagination and tool strategy as overhead sources, without establishing a compiler defect. V3 deep Shell attempts used 13–21 decisions and did not reach the intended depth.

These are descriptive results from one small pilot, not a general claim of superiority. Its frozen bundle is `_build/evaluation-bundles/531b0f635a6cc507280defb931b4b73eb1bce47b`. Earlier runs, reports, bundles and archive branches remain intact.

Generated evidence stays under ignored `_build/`: run roots, `evaluation-bundles`, verification logs and source snapshots. Fork-specific research documents have been consolidated here. Historical raw files formerly in `docs/validation` were hash-verified and moved to `_build/verification-v4/retained-history/docs/validation/`; the cleanup inventory and pre-change snapshot are in `_build/verification-v4/`. Do not use `git clean` or delete a bundle referenced by retained evidence. Remove only verified, reproducible temporary build outputs.

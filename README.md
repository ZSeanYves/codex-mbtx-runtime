# Codex with programmable MBTX

An experimental fork of [OpenAI Codex](https://github.com/openai/codex), maintained at [ZSeanYves/codex-mbtx-runtime](https://github.com/ZSeanYves/codex-mbtx-runtime). Upstream copyright, attribution and the [Apache-2.0 license](LICENSE) are retained. Upstream npm packages do **not** contain this fork's MBTX integration; build this repository to reproduce the experiment.

We study whether **MoonBit programs can replace Shell programming and orchestration inside the same Codex workflow**, and how this affects model decision rounds, task completion and tool trajectories. This is a local research implementation, not a plugin distribution or a general security product.

## Comparison and execution boundary

The two arms receive the same task inputs, goals, model configuration, external utilities and common output budget. Shell uses Codex's execution tools. MBTX uses the `mbtx` extension, file editing and bounded `read_resource`; Shell and unified-exec tools are disabled in the MBTX arm and checked in captured model requests.

New collections enable upstream Code Mode in both arms (`features.code_mode = true` and `features.code_mode_host = true`), adding JavaScript `exec`/`wait` orchestration alongside each arm's direct tools. Bundle preparation includes `codex-code-mode-host` and lets these feature flags select the tool mode. This changes the collection conditions from the completed pilots below, which disabled Code Mode; prepare a new bundle and run directory before collecting again.

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

The final V4 collection bundle recorded Linux x86_64, `moon` and `moonrun` version `0.1.20260915 (2e1a46d)`, and Rust dev builds without debug info. Use `bundle.json` to check the exact components for each source run; version strings alone do not replace their hashes.

Start from a checkout of this fork, then run commands from its root:

```bash
git clone https://github.com/ZSeanYves/codex-mbtx-runtime.git
cd codex-mbtx-runtime
```

Validate offline before supplying a provider credential:

```bash
unset MBTX_BUNDLE
moon run mbtx/scripts/validate-evaluation.mbtx --fast
moon run mbtx/scripts/validate-evaluation.mbtx
```

The fast path checks references, MoonBit, report rendering and affected Rust crates. The full offline entry prepares a fingerprinted bundle, validates both-arm task references, local sandbox/policy/worker preflight, fixed-response replay, retry faults, interruption and deterministic report rebuilding. It does not contact a model provider. Replay uses a local Responses server and actual Codex/tool execution; prescribed solutions verify the harness, **not autonomous model performance**. Preparation can reuse verified components or an existing identical bundle; it never means that changed source may reuse an incompatible executable.

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

Approximate step targets are 5–20 / 20–50 / 50–100. These are **design targets**. The completed V4 pilot did not achieve this separation: Shell quick attempts took 25–30 steps, standard 13–27 and deep 21–43, including failed attempts. Keep the frozen labels when interpreting these results; improving calibration requires another protocol revision.

Task design borrows executable delivery and recovery validation ideas from [Terminal-Bench](https://github.com/laude-institute/terminal-bench/tree/d28711d0da2675d0bb1d56de45ae5df6082438a3), and “fix failures while preserving correct behavior” from [SWE-bench grading](https://github.com/SWE-bench/SWE-bench/blob/02e7a74ffd0b707aab73d203fe87bdc7c76afc8e/swebench/harness/grading.py). Reference organization and examples draw on [OpenSeek](https://github.com/moonbitlang/openseek/tree/be29e280ae3e75ec907a93fcd248923e054f6728). We do not copy their tool stacks or claim comparable benchmark scores.

## Source delivery and measurement

Delivery requires exactly `solution.sh` or `solution.mbtx` for the assigned arm, at most 65,536 UTF-8 bytes, runnable without arguments from cwd and without auxiliary authored files. A wrong filename is `missing_source`; a correct workspace artifact cannot substitute for source delivery or a completed session.

MBTX delivery is compiled once, then run under the same policy generation logic on fresh visible and two withheld inputs. Withheld inputs change structure as well as values. Reusable recovery and all complexity cases actually run an unchanged second pass. Deep cases additionally receive evaluator-declared input updates while retaining prior state. Each phase keeps its own result, receipt and process evidence; an empty second-pass execution list cannot erase first-pass work.

One **step** is one observed model decision round. Tool calls, HTTP requests/retries, compilations, program starts, worker operations, recovery and errors are separate measurements. Validation has no model steps. Unfinished attempts retain all observed decisions and `steps_to_success = null`.

The 7,200-second total attempt deadline covers Codex startup through delivery validation. Compilation/runtime consume the remaining budget; no model-step, tool-call or request-count limit is imposed. Output/source/argv/path safety limits remain. Online collection is serial, with at least 15 seconds between requests. HTTP request retries and stream retries are each capped at two at their respective retry layer, not across an entire attempt. Raw 429 responses are retained without automatic 429 retry; the gate delays later requests by Retry-After, or at least 30 seconds if absent. Failed attempts are not silently replaced.

Reports include all assigned attempts, observed step lists/ranges/medians, within-task repeat differences and conditional step differences for successful pairs. They also show reference first reads/continuations, compilation failures, decisions before the first successful compile, runtime errors and external retries. Repetitions are not independent tasks. There is no pooled causal estimate across tracks or small-sample confidence claim. Attribution distinguishes sandbox/launcher, worker IPC, provider/relay, Codex scheduling, MBTX compilation/runtime, fixture/oracle, harness, timeout and censoring, with observed/inferred/unknown labels.

## Collection, resume and reports

V4 has completed offline validation and an online pilot. The commands below create a **new** run: by default, 16 tasks × 2 repeats × 2 arms = 64 attempts. They do not reproduce the historical pilot's interruptions and separate correction cohorts automatically.

Credentials are supplied through the shell environment. To enter a key without placing it in shell history:

```bash
read -rsp 'OPENAI_API_KEY: ' OPENAI_API_KEY
export OPENAI_API_KEY
printf '\n'
```

Never put credentials in repository files, reports or command output. Review `mbtx/config/relay.toml` before freezing a run; the recorded pilot used `gpt-5.6-terra` with `xhigh` reasoning through its configured Responses relay. A different endpoint, model or configuration defines new collection conditions.

```bash
# Prescribed offline replay; no provider credential is needed.
moon run mbtx/scripts/collect-study.mbtx replay --suite all

# Online collection; requires the credential and a prepared, verified bundle.
moon run mbtx/scripts/collect-study.mbtx relay --suite all
# Or use --suite basic / --suite complexity.
```

Collection prints the exact run directory and current task/arm/repetition/progress. Default output is `_build/programmable-study-v4/<platform>/<mode>/run-<timestamp>`; the default seed is `20260919`. Online order is local sandbox/policy preflight, worker receipt, relay probe, then attempts. Preflight failure stops before provider access; relay failure stays external evidence. Do not disable preflight or loosen the sandbox to get past a failure. Keep the collector in a persistent terminal/session for this long-running study; losing its host session can censor an attempt.

In another terminal, follow the recorded progress without starting another collector:

```bash
/path/to/frozen/bundle/mbtx-eval log /absolute/path/to/run \
  --bundle /path/to/frozen/bundle --follow
```

Resume only with the original run, bundle, protocol, model/configuration, task selection, toolchain, utilities, pacing and cache conditions:

```bash
# Pin the original bundle explicitly; the default pointer may now refer to another build.
export MBTX_BUNDLE=/absolute/path/to/original/bundle
moon run mbtx/scripts/collect-study.mbtx relay --suite all \
  --output /absolute/path/to/original/run --resume
```

The manifest rejects mismatched resume conditions. Resume skips **every recorded attempt**, including failures, interrupted attempts and censored attempts; it starts only unassigned arms. It does not retry the interrupted task from its beginning. Old protocols cannot be resumed with the new task generator.

For planned batches, `--batch-pairs N` stops between pairs after at most N pairs with pending arms in that invocation. It does not shrink the frozen schedule: the report still includes unstarted slots in its assigned denominator. Resume the same output with identical conditions to start the next batch.

After a task-contract or harness correction, use a **new output directory** and a prospectively recorded correction scope. `--start-pair N` selects a zero-based suffix of the original complete schedule. Keep task selection, repetitions and seed identical so pair IDs, repetition indices and arm order are preserved. For example, after separately preparing and verifying the corrected bundle:

```bash
export MBTX_BUNDLE=/absolute/path/to/corrected/bundle
moon run mbtx/scripts/collect-study.mbtx relay --suite all \
  --repeats 2 --seed 20260919 --start-pair 10 --batch-pairs 3 \
  --output /absolute/path/to/new-correction-run
```

This starts at pair index 10 and attempts three pairs, retaining the rest of the suffix as unstarted. It does not import or overwrite earlier attempts. Pass the same `--start-pair` when resuming that run. Before collecting corrections, record affected tasks/repeats, both arms, reasons and source-run mappings, independently of their scores. Preserve originals and distinguish bundle/protocol versions when combining results. Ordinary model failures are not grounds for replacement.

For a correction that affects individual arms, use `--slots` with the exact frozen pair ID and arm. The complete schedule remains in the manifest, while unlisted arms and pairs are skipped:

```bash
/path/to/frozen/bundle/mbtx-eval run --mode relay \
  --bundle /absolute/path/to/corrected/bundle \
  --output /absolute/path/to/slot-correction-run \
  --slots pair-0016/mbtx_program,pair-0016/shell_tool,pair-0027/mbtx_program
```

Slot IDs are validated against the complete selected schedule and are recorded under `schedule_selection.slots`. They cannot be combined with a nonzero `--start-pair`; use `--resume` with the same slot list to continue an interrupted slot run.

To rebuild an existing report, use **that run's frozen bundle**:

```bash
/path/to/frozen/bundle/mbtx-eval report /absolute/path/to/run \
  --bundle /path/to/frozen/bundle --format all
```

Reports are standalone offline HTML, JSON, Markdown and CSV under `RUN/reports/<id>/`. No SigNoz deployment is used. Raw evidence includes preflight stderr/process/result, policies, worker events, HTTP request/response bodies, Codex JSONL/stderr, traces, effective configuration, source/hash and submission phases. Authentication is excluded from captured HTTP metadata. A corrected analyzer may be used for a historical reporting defect only with a separate compatibility record; do not alter its original run manifest or raw evidence.

To package a stopped individual run with its evidence and reports:

```bash
moon run mbtx/scripts/package-evidence.mbtx /absolute/path/to/run
```

This creates `RUN-evidence.tar.gz` and a SHA-256 sidecar, excluding disposable top-level workspaces and the collector lock. It refuses to overwrite an existing archive. It does not include the external toolchain or combine multiple runs.

Reproducibility means verifiable frozen conditions, deterministic fixture/oracle replay and rebuildable reports. It does not promise identical online model decisions or success rates. V4 changes tasks, references and interaction together; differences from V3 cannot be attributed solely to policy or prompting.

## Completed pilots and evidence exports

V3 (`run-1789785914915`) used 12 tasks × 2 repeats × 2 arms. Shell succeeded in 22/24 attempts, MBTX in 24/24. All 48 sessions completed. Shell observed 269 decisions (median 10, range 5–25); MBTX observed 358 (median 15.5, range 4–35). MBTX had 78 compilation failures; four delivery attempts accounted for 77 of the 89 additional decisions. The trace supported API guessing, diagnostic pagination and tool strategy as overhead sources, without establishing a compiler defect. V3 deep Shell attempts used 13–21 decisions and did not reach the intended depth.

The V4 online comparison completed on 21 September 2026. It selects **64 attempts from six explicitly mapped source runs**, retaining all **75 original attempts**, including 11 superseded or interrupted attempts. Task-contract clarifications, a Unicode report crash and a hidden 600-second gateway deadline required declared corrections; a later host interruption required a separate configuration-task correction. This is a documented multi-run comparison, not one uninterrupted homogeneous collection.

| V4 group | Shell success | MBTX success |
|---|---:|---:|
| All | 27/32 (84.4%) | 30/32 (93.8%) |
| Basic | 20/20 | 19/20 |
| Complexity | 7/12 | 11/12 |

The last pair ended in external connection failures; both remain in the denominator with unknown network/relay/provider root cause. Among the 26 pairs where both arms succeeded, MBTX used fewer steps in 5 and more in 21; median MBTX minus Shell was **+5.5 steps**. MBTX recorded 187 reference first reads, no reference continuation reads, and 104 compilation failures. Compiler execution totaled about 117 seconds, so model learning and correction rounds, rather than compiler wall time or repeated diagnostic pagination, remain the main inferred overhead. MBTX completed more complex attempts in this sample but has not demonstrated fewer decision rounds.

All source-run relay probes succeeded; local sandbox, policy and worker preflights passed. The audit covered 1,631 requests without a pacing, concurrency or retry-limit violation. The independent combined-report audit passed 395 checks. The repair validation passed MoonBit 52/52, rollout-trace 65/65 and mbtx-eval 56/56 with two existing bundle tests skipped; these scoped checks do not claim a passing full Rust workspace suite.

On the collection host, shareable exports live together in `~/Downloads/programmable-pilot-v3/` and `~/Downloads/programmable-study-v4/`, with `programmable-pilot-v3.tar.gz` and `programmable-study-v4.tar.gz` alongside them. Open each export's `index.html` or `README.md` first. V4 preserves the `_build/` layout for the combined report, raw source-run evidence, correction provenance and available frozen bundles. Its combined report is under `_build/programmable-study-v4/Linux/relay/combined-20260921-v4-online/` inside the export. These local artifacts are not checked into Git or automatically downloaded with a clone.

Exports support offline inspection and evidence/report verification. Frozen bundle metadata retains original absolute paths, hashes and external toolchain requirements; an evidence archive is not a portable executable installation. V3's original frozen bundle is unavailable, so its export supports evidence inspection but does not provide a complete report-rebuild environment. Use matching frozen conditions where available to rebuild historical reports, or prepare a new verified bundle for a fresh experiment. Never resume a historical run with newly built conditions. Research documentation is consolidated in this README; generated evidence, bundles, verification logs and source snapshots belong outside versioned source. Before removing local generated files, verify their exported copies and preserve the evidence and provenance required for any result you retain.

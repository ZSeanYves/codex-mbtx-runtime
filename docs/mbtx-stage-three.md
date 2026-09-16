# Stage Three: Programmable Tool Evaluation

Stage three activates the Rust collection adapter and shared MoonBit evaluation
package. The completed small pilot motivated the [expanded study protocol](mbtx-study-protocol.md),
which defines 24 scenarios, four input variants and two repetitions (192 pairs),
saved-source validation and task-aware uncertainty. Use `collect-study.mbtx` for
new research collection. The eight-task pilot remains a small diagnostic suite;
its outcomes are not pooled with the new study.

## Runtime and analysis ownership

`codex-rs/mbtx-eval` runs the built Codex executable, serializes HTTP traffic,
preserves native rollout traces and OTel batches, snapshots task outputs and
publishes immutable attempts. `mbtx/evaluation` owns task goals, output oracles,
step rules, failure classifications, populations and comparative analysis. A
single prebuilt Wasm worker handles a versioned JSONL interface for each run or
report. Collection never compiles its harness or prepares dependencies per arm.

Shell remains the default product interface. Evaluation selects `shell_tool`
or `mbtx_program` explicitly. Both use the same frozen model catalog with direct
tools, explicit workspace filesystem permissions, common instructions, configured Codex output allowance and available
system utilities. The MBTX arm disables Shell tool exposure; the Shell arm disables
MBTX. Code Mode, delegation, plugins and memories are disabled. Requests preserve
the actual tool schemas, and the local gate checks execution-tool exposure.

The treatment includes the interface description, installed utilities and MoonBit
libraries. It is not a claim about language syntax alone. A Shell arm may submit a
complete script. MBTX may invoke native utilities within the same permissions.
Native per-tool output contracts remain different (Shell tokens versus MBTX stream
bytes); the [study protocol](mbtx-study-protocol.md) specifies this attribution
boundary. The shared Codex allowance does not make those contracts identical.

## Pilot protocol

The `programmable-steps-pilot-v3` diagnostic protocol defines eight goals: repository indexing,
structured aggregation, Unicode record normalization, subprocess orchestration,
log diagnosis, configuration repair, bounded-output inspection, and recovery of a
partially processed dataset. Inputs and oracles are stored in the run manifest;
only inputs and goals enter each workspace. The independent oracle compares
structured output and verifies preservation of other task inputs. It does not
accept a model's assertion that the task succeeded.

Version two stated output field names and types in each model-visible goal. It
preserves tasks and expected values but does not pool new attempts with version-one
samples. Recovery merge now explicitly requires `total`; the earlier prompt did
not fix that key. Old manifests and reports retain their original verdicts.
Version three adds a real Git repository per attempt, isolated Git configuration,
limited filesystem reads, a frozen MoonBit registry index and an updated generic
tool example. These environmental changes are not retroactively applied to old
evidence. Reconstruct older runs with their original analysis bundle.

The MBTX tool includes a bounded, versioned file/JSON/child-process example, executed
by offline validation. Both arms receive the same utility and dependency reference.
Successful compiler diagnostics occupy at most half the combined stream budget,
preserving runtime output space; failed builds keep the full diagnostic budget.

These are small pilot tasks, not a representative coding benchmark. The process
task's oracle validates its result objects; it does not prove how the agent
produced them or count operating-system process starts. Actual process counts
remain unknown unless separately observed. Use the pilot to inspect floor effects,
tool adoption, task difficulty and request cost before freezing a formal suite.

The initial pilot uses one pair per task. Adjacent pairs alternate AB/BA; a fixed
seed permutes task order. Each arm receives its own workspace, HOME, Codex home,
session and mutable program build state. Fixed fixture files and dependencies are
prepared once; model-generated source is compiled during real tool work.

Project instruction loading and automatic skill instructions are disabled, project
config discovery stops at the workspace, and login-shell initialization is disabled.
Each arm starts from the same deterministic Git commit in its own repository.
The old empty `.git` marker did not stop Git from discovering the parent checkout;
the current setup verifies the actual repository root and clean baseline.
Task tools can read only platform runtime files, the recorded compiler/library
paths, frozen dependency/index data and their own work area. The user's MoonBit
publishing credentials and evaluator manifests are outside that read policy.
`agents.enabled=false` disables delegation. The gate checks top-level tools and
namespaced `additional_tools` before forwarding. It refuses unexpected tools or
inherited instruction fragments and preserves rejected requests as harness evidence.
Common editing tools remain available in both arms and their calls are reported:
file correctness alone does not establish production or verification through MBTX.

The pilot allows 12 logical steps, 24 emitted tool calls, 16 HTTP sends and 600
seconds per arm. The request boundary checks budgets and a 100 ms native-trace
monitor interrupts excessive step/tool work. Tool batches can cross the soft
tool boundary before interruption; every observed operation remains in evidence.
Token usage remains a recorded native field where available, not a fabricated
zero or a presently enforced total-token limit.

Every scheduled arm remains in intention-to-treat denominators. Oracle success,
execution status and counting coverage are separate. Paired step differences
require two successful arms with complete step evidence. Reports also preserve
the first observed step/tool divergence and inferred repair candidates. Pilot
means are descriptive. The expanded study freezes scenario-aware uncertainty and
repetitions separately. Fixed-response replay is a separate validation mode.

## Build and run

Use the fork's Rust toolchain and the latest installed MoonBit toolchain. Install
`just` and `cargo-nextest` using Cargo if they are absent. Both arms also require
`rg`, `jq`, `sh`, and `git` on PATH. On Debian/Ubuntu, install missing utilities with
`sudo apt-get install ripgrep jq git`; on macOS, use `brew install ripgrep jq git`.
The collector records resolved utility paths and SHA-256 hashes before any API
request. Resume refuses a changed PATH or required executable. From the repo root:

```bash
git pull --ff-only origin main
moon run mbtx/scripts/prepare-evaluation.mbtx
moon run mbtx/scripts/validate-evaluation.mbtx
```

The collector currently supports Linux and macOS; Windows collection is rejected
explicitly. Linux remains the primary online platform.

Preparation prints the bundle path and saves it in `_build/evaluation-bundle.txt`.
The key includes implementation sources, Cargo/toolchain inputs, MoonBit tools,
core libraries and dependency contents, platform and build profile. Documentation
does not enter the key. Repeating unchanged preparation verifies and reuses the
bundle. Installed compiler or core-library changes invalidate verification.
Each invocation writes `preparation-*.json` beside the bundle directories, with
the preparation cost and reuse flag. This wall-clock build/verification cost is
outside attempt measurements.

Run fixed replay without credentials:

```bash
moon run mbtx/scripts/collect-pilot.mbtx replay
```

For Linux relay collection, keep the existing private credential JSON outside Git:

```bash
moon run mbtx/scripts/collect-pilot.mbtx relay --credentials-file /private/path/credentials.json
```

The JSON has the form shown in `mbtx/config/credentials.json.example`. Alternatively,
export `OPENAI_API_KEY` before running. The reader never needs a global Codex login
or an `auth.json` copied into an experiment. The key is read by the collector and
held by its forwarding gate; Codex receives a local, attempt-scoped credential.
Do not put a real key in the checked-in example or provider TOML.

`mbtx/config/relay.toml` uses the same working provider contract as
[Codex-MBTX at c11cec2](https://github.com/ZSeanYves/Codex-MBTX/blob/c11cec22555a7e91a0c687eac8c6465d31a06e58/config/codex-relay-shell.toml.example):
`OpenrouterICU`, `https://tokenadvent.com/v1`, `gpt-5.6-terra`, `xhigh`, Responses,
`env_key = "OPENAI_API_KEY"`, and `requires_openai_auth = false`. This is a reference
to configuration behavior; no old measurements become new research evidence.

The generated per-arm config is checked by the fork's `--strict-config` loader.
Unsupported historical options are omitted. Codex's model traffic uses the local
gate while task subprocesses retain network-disabled workspace permissions.
Product analytics are disabled. Native OTel's optional
`analytics/codex/turn-costs` lookup is answered locally with an explicit 404 and
retained as `auxiliary-*.json` with `forwarded=false`. This equal observation
condition avoids an extra billing request to the relay; monetary cost stays
unknown. No model request or remote error is reclassified as that local response.

The script prints an ignored `_build/programmable-pilot/<platform>/<mode>/run-*`
directory. Linux and macOS datasets remain separate. To start with two goals:

```bash
moon run mbtx/scripts/collect-pilot.mbtx relay --tasks structured-summary,configuration-repair --credentials-file /private/path/credentials.json
```

Use a small diagnostic run when investigating setup, and the expanded study for
new comparative data. Its 192 pairs are programmable-task experiments, separate
from the old repository's transparent-launcher experiments.

After collection stops, package evidence without dependency copies and workspaces:

```bash
moon run mbtx/scripts/package-evidence.mbtx <RUN>
```

This creates `<RUN>-evidence.tar.gz` and its `.sha256` sidecar, including the run
manifest, fixtures, probes, attempts and reports. It verifies the compressed archive
before publication and refuses overwrite. Send both files; no task rerun is needed.
The checksum uses the archive's basename, so it can be verified after transfer with
`sha256sum -c <RUN>-evidence.tar.gz.sha256` on Linux or
`shasum -a 256 -c <RUN>-evidence.tar.gz.sha256` on macOS from the archive directory.

## Pacing and recovery

All model requests, probes and auxiliary Responses calls use one gate. It owns
the concurrency slot through the full response stream and records queue wait
separately. The minimum start interval is 15 seconds, about four starts per minute.
Automatic HTTP and stream retries are disabled. A 429 remains an external failure;
later sends respect `Retry-After` in seconds or HTTP-date form, with 30 seconds
when absent or invalid. A second evaluator using the same account on the same
host is refused. Other computers and applications using that account remain
outside the gate.

Initial qualification requires one success in at most three probes. Collection
pauses after five consecutive final infrastructure failures or eight among the
latest sixteen completed arms. Every failure keeps its pair assignment. A
timeout, cancellation or unfinished attempt does not become a zero-step success.

Each probe prints its HTTP status, error detail and evidence directory, and saves
`summary.json` next to the complete request/response capture. A successful probe
requires a parsed terminal `response.completed` event, not a matching phrase in
an incomplete stream. Probe input uses Codex's typed `input_text` representation.

The first Linux relay pilot (`run-1789537433934`) stopped before any task because
all three probes returned HTTP 400, `Unsupported content type`. Inspection found
that the gateway appended a second JSON `Content-Type` after copying the native
header. A loopback regression reproduced the duplicate header; forwarding now
sets the header once and records the normalized outbound metadata. This is an
adapter request defect, not evidence of invalid credentials or a backend task
failure. The supplied failure archive remains unchanged. After updating, prepare
a fresh bundle and start a new run; an existing bundle contains the previous code.

Use the printed bundle path for direct commands:

```bash
<BUNDLE>/mbtx-eval log <RUN> --follow
<BUNDLE>/mbtx-eval report <RUN> --format all
<BUNDLE>/mbtx-eval run --bundle <BUNDLE> --output <RUN> --mode relay --resume --credentials-file /private/path/credentials.json
<BUNDLE>/mbtx-eval replay <RUN> --bundle <BUNDLE> --output <NEW-RUN>
```

Resume requires the same protocol, bundle, configuration, seed, task selection
and repetitions; repeat nondefault options. It executes only unassigned arms.
Previously interrupted attempts stay censored and are never overwritten or
silently replaced. SIGKILL can prevent finalization; report reconstruction retains
the surviving prefix. After terminating a collector externally, ensure its
recorded Codex process has stopped before resuming. Cleanup never scans an entire UID.

Recorded replay executes saved Responses bytes in new workspaces. Truncated streams
and responses containing source-run absolute paths are rejected explicitly. It
does not rewrite model commands, mutate the source evidence or claim portable
reproduction of arbitrary environment-dependent programs.

## Reports and observability

Each terminal attempt has a `seal.json` with raw-file SHA-256 hashes. Sealed
attempts, shared fixtures and bundles have write permissions removed to prevent
background tools from appending build products. Retained
evidence includes assignment, effective config, request/response bodies, native
trace and payloads, native OTLP JSON, Codex JSONL/stderr, process outcome and output
snapshot. A report creates a fresh revision containing JSON, CSV, Markdown, a small
offline HTML summary and standard `steps.otlp.json`. Reconstruction needs the
analysis bundle, not credentials, a live provider or a viewer. Unsealed or altered
evidence is never eligible for the complete successful-pair subset.
The frozen run manifest and fixtures are hash checked too. Each report revision
records the analyzer bundle and seals its outputs; changing analysis rules never
replaces a previous report. HTTP success without a terminal Responses event is
an external stream failure. Local evidence write failures are harness failures.

Reports distinguish native request starts from observed upstream dispatches, and
invocation failures from MBTX compilation/execution and Shell command failures.
The latter categories may overlap with invocation failures and are not additive.
Missing result payloads remain unknown. A budget-exhausted attempt's accepted-step
count is not its steps-to-success value. Shared edit counts remain visible even
when the independent file oracle passes.

Use SigNoz for interactive timelines. The native OTel batches and derived step
spans share attempt identifiers. Start the official Collector once with
`mbtx/observability/collector.yaml`, setting `MBTX_OTEL_ARCHIVE` to a writable
archive directory and `MBTX_SIGNOZ_OTLP_ENDPOINT` to the local SigNoz OTLP HTTP
endpoint. The documented collector configuration targets contrib 0.139.0.

```bash
otelcol-contrib --config mbtx/observability/collector.yaml
<BUNDLE>/mbtx-eval import-otel <RUN>/attempts --endpoint http://127.0.0.1:4318
<BUNDLE>/mbtx-eval import-otel <RUN>/reports/<REVISION> --endpoint http://127.0.0.1:4318
```

Import native attempts once and choose one report revision to avoid duplicate
spans. The import command checks HTTP success and rejects partial ingestion.
`collector-validation.yaml` verifies OTLP ingestion locally without SigNoz.
See [SigNoz self-hosted ingestion](https://signoz.io/docs/ingestion/self-hosted/overview/)
and the [OTLP specification](https://opentelemetry.io/docs/specs/otlp/) for the
standard receiver contract.

Recommended SigNoz views:

| View | Selection and interpretation |
| --- | --- |
| Pair comparison | Filter `service.name=mbtx-evaluation`, then `mbtx.task` and `mbtx.arm`; inspect `mbtx.agent_steps` and `mbtx.oracle_success`. |
| Step timeline | Open `mbtx.attempt` and its `codex.agent_step` children; each step has its native ID, outcome and source sequence. |
| Native execution | Filter the matching `mbtx.attempt_id` in Codex's native trace service; inspect requests, scheduling, tools and compiler/runtime work. |
| Failure inspection | Select `mbtx.status`, follow the evidence reference, and distinguish external responses, tool failures and incomplete observation. |

Step spans use observed producer wall timestamps for display. Request durations
and queue waits use the collector's monotonic clock; cross-clock durations are
not subtracted. The current native process envelope includes shutdown and export
flush. It is not pure task execution time. Timing remains diagnostic until a
separate observation-calibration experiment supports performance claims.

## Validation and remaining acceptance

The validation entry checks pure MoonBit oracles and populations, native HTTP
stream serialization and cooldown, evidence sealing, HTML escaping, real Codex
fixed replay, deterministic report reconstruction and bundle reuse. All network
fault tests are local. Native request/step accounting continues to use the
stage-two implementation.

The macOS ARM64 implementation check on 2026-09-16 passed:

| Check | Observed result |
| --- | --- |
| Shared MoonBit model | 13 tests passed, including oracle, missing-evidence, failure classification and population rules. |
| Scoped Rust packages | 71 tests passed across the adapter, MBTX extension and native rollout trace packages. |
| Real Codex fixed replay | 16 successful arms, eight comparable pairs, two accepted logical steps per arm. These trajectories were prescribed. |
| Local HTTP faults | Eight arms covering 429, 500, disconnect and missing terminal SSE events; all retained as external failures. |
| Collector interruption | Actual SIGKILL preserved one censored attempt and produced a partial report with zero comparable pairs. |
| Recorded-response execution | All 16 arms succeeded in new workspaces using the saved response bodies and protocol headers. |
| Reconstruction and resume | Reconstructed JSON was byte-identical; completed resume added no attempts or model requests. |
| Build reuse | A second unchanged preparation verified the existing bundle without compilation. |
| Official OTLP receiver | Collector contrib 0.139.0 accepted 57 batches: 12,219 spans, including 16 attempt spans and 32 logical steps, plus 280 log records. |

The offline validation run was `run-1789533732520`, using bundle fingerprint
`460274bc969cca68434c4c093ed6086324f8353f`. The evidence remains in the local ignored
`_build/stage-three-validation` directory. Final lint/format cleanup and the
explicit collection-platform guard do not retroactively change that bundle.
The Collector check proves OTLP ingestion, not deployment or usability of the
SigNoz interface. SigNoz UI validation remains pending a local installation.

The protocol-v2 correction was checked separately on macOS ARM64 on 2026-09-16:

| Check | Observed result |
| --- | --- |
| Pure models and scoped Rust packages | 15 MoonBit tests and 75 Rust tests passed. |
| Real Codex execution | All 16 fixed-response arms and all 16 recorded-response arms passed their independent file oracles. |
| Tool and context isolation | Captured task requests exposed the assigned execution interface and common tools, without delegation or inherited repository/skill instructions. |
| Compiler diagnostic pressure | A real successful build with repeated warnings preserved the runtime marker and reported diagnostic truncation. The exact model-visible file/JSON/child-process example also executed successfully. |
| Faults, interruption and reproducibility | Eight external-fault arms, SIGKILL recovery, deterministic reconstruction, immutable resume and unchanged-bundle reuse passed again. |
| Archived pilot accounting | Reanalysis of the recovered Linux attempt records identified 79 MBTX compilation failures and eight Shell command failures. Original evidence and oracle verdicts were not changed. |
| Evidence transfer | Complete compressed archives passed checksum verification; an existing archive was refused without modifying its checksum. |

The full offline run was `run-1789544721282`, with bundle
`50c4111e7aa87005f507b342192a04bc3e3fb539`. Subsequent accounting checks covered
incomplete tool evidence and upstream sends that failed before response headers;
bundle `fc22681b764063601396d9f08594000b2e883ae9` reconstructed that run with
16 successful arms and eight comparable pairs. This correction used no live relay
requests. It validates the harness and runtime fixes; whether the model now solves
the tasks through MBTX remains a question for a new Linux pilot. Version-one and
version-two attempts must not be pooled as one treatment.

The stage-two Linux archive `run-1789527207143.zip` was reviewed on 2026-09-16:
eight captures reproduced their states and reports byte for byte, and all 134
extracted files remained unchanged. Its SHA-256 is
`a919938d5ce765842c3614eb0166434ef702d02cb270abcd724afb039ee0df55`.
The archive did not include the terminal unit-test transcript or a build manifest;
the acceptance statement is scoped to its fixed-replay evidence.

Stage-three Linux sandbox/runtime verification and live model behavior remain
user-operated. A successful fixed replay does not prove autonomous MoonBit
programming ability, step reduction, general task success or lower latency.

The `MBTX required` CI status aggregates path-aware fast checks and the optional
workflow-dispatched offline job. PR checks do not build the full Codex CLI or call
a provider. The offline job builds one bundle and uses only controlled loopback
responses, retaining artifacts even when a check fails. Existing upstream
workflows remain available.

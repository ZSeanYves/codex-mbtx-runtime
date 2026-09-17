# Programmable MBTX long-workflow study

Protocol: **`programmable-long-study-v1`**. This document is the current collection
and interpretation contract. Older pilot and expanded-study evidence remains
unchanged and must not be pooled with this protocol.

The [implementation validation record](mbtx-long-study-validation.md) separates
verified local behavior from remaining platform and online research acceptance.

## Research question

Under the same goals, inputs, model and permissions, does a programmable MoonBit
tool improve task completion or reduce accepted model decisions compared with
the Shell tool? Compilation, documentation lookup, errors and repair are genuine
costs of the interface. They are retained rather than subtracted after collection.

An agent step is one logical model decision in the native Codex loop. Multiple
tools emitted by one accepted response count as one decision. A normal final
answer also counts. Started decisions, accepted decisions, tool calls, HTTP
attempts, auxiliary requests, compilation and process observations are separate
quantities. They are never summed into a synthetic total step count.

The collector has no step, tool-call or request-count cutoff. Its default
attempt wall limit is 3,600 seconds, including pacing and external waiting.
At timeout, interruption or crash, all observed decisions remain in evidence;
steps to successful completion is null. Execution termination, session
termination, oracle correctness and evidence integrity remain separate fields.
The collector never stops because the oracle would pass, sends a continuation
prompt, edits a model call or discloses the private oracle to the model.

## Prespecified population

| Cohort           | Category                  | Required linked work                                                                                        |
| ---------------- | ------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Workflow         | Repository impact         | Change classification, dependency closure, exclusions and validation evidence                               |
| Workflow         | Data reconciliation       | CSV and JSONL normalization, duplicate/version resolution, currency conversion, rejected records and totals |
| Workflow         | Incident investigation    | Cross-file request correlation, retry deduplication, configuration evidence and incident chains             |
| Workflow         | Configuration migration   | Inherited configuration resolution, migration, quarantine and preservation of unrelated values              |
| Workflow         | Dependent jobs            | Actual worker execution, dependency order, controlled recovery, disabled branches and state summary         |
| Workflow         | Incremental recovery      | Checkpoint validation, generation selection, stale-state cleanup and execution of missing work only         |
| Workflow         | Output forensics          | Separate worker streams, anomaly context, Unicode, cross-file evidence and final tail checks                |
| Workflow         | Release verification      | Candidate version/content validation, hash checking, repair, stale-file removal and manifest generation     |
| Program delivery | Reusable data program     | A saved program that also passes independently prepared new inputs                                          |
| Program delivery | Reusable recovery program | A saved coordinator that correctly recovers independently prepared states                                   |

Each category has two complexity levels, two input variants and two repetitions:
eight pairs per category, **64 workflow pairs and 16 program-delivery pairs**.
The formal total is 80 assigned pairs, including failures, not 80 selected
successes. Standard tasks have approximately five linked semantic constraints;
extended tasks add branches, conflicting generations, exclusions, damaged state
or validation obligations. These constraints do not prescribe model steps.
Sleep and irrelevant repeated actions do not extend tasks.

The separate ten-pair pilot uses one new input per category. It is excluded from
formal statistics. Shell may save complete scripts; MBTX may save complete
programs. Workflow tasks require correct work and artifacts, not a submitted
program. Delivery tasks require `solution.sh` or `solution.mbtx`, respectively,
and independent execution on new inputs. Public native utilities are permitted
in both arms. Fixed replay uses reference implementations, including declared
Shell utilities within MoonBit; it validates execution and measurement only.

The seed is `20260916`. Adjacent pairs alternate AB/BA across repetition
boundaries. Each ten-pair formal batch contains all categories and balances the
two complexity levels. Both arms start from the same sealed fixture and Git
baseline, with independently writable copies, HOME and sessions. Private
expected outputs and hidden cases are outside the sandbox's readable boundary.
Controlled-worker receipts verify actual execution using the peer process and
executable hash; artifact text alone cannot substitute for required execution.
The job oracle also checks observed dependency completion before successful
dependent execution, required retryable failures, and prohibited disabled jobs.

## Frozen treatment and guidance

The bundle includes Codex, the adapter, MoonBit analyzer, fixture worker,
MoonBit toolchain, the approved async dependency closure, model catalog and
general reference documents. It excludes personal credentials and unrelated
dependency caches. Component receipts include source/toolchain/platform/profile
and artifact fingerprints. Reusing an identical bundle verifies hashes and does
not start a component compiler. Model-generated source still incurs compilation
when required by its effective inputs.

Both arms can use `read_resource` to read `reference:moonbit`, `reference:shell`,
`reference:tools` and `reference:examples`. These contain APIs and generic
examples, not benchmark solutions. The organization follows the pinned
[OpenSeek prompt](https://github.com/moonbitlang/openseek/blob/6d4a35b4eabf71dd10b87d003d7b31fef2960161/prompt/default_prompt.mbt.md),
adapted to this fork's permissions and tool interfaces. Examples are compiled
and executed by `validate-reference.mbtx`. Reference bytes are treatment inputs.

`mbtx` accepts exactly one of `source` or `filename`. Each execution snapshots
the source and starts a fresh process and heap. Dependency preparation is reused
within an attempt. A successful compiled artifact can be reused only for the
same source, effective compiler/dependency inputs, working directory, target,
profile and options. Build operations serialize on the session lock. Interrupted
preparation is not published as a cache entry. Runtime outputs are never cached.
A cache hit identifies the original build and its diagnostic resources.

Both arms use Codex's 4,096-token tool-output policy. Full stdout and stderr are
captured before preview truncation. A resource receipt records observed bytes,
EOF, completeness, hash and write cost. `read_resource(resource_id, offset,
max_bytes)` returns UTF-8-safe pages and the actual next offset. References and
the current session's registered resources are its complete access scope.
Capture failures are explicit; they do not silently relabel a preview as full
output or prevent the requested program from executing.

## Request pacing and recovery

All evaluator requests use one gate, including probes and auxiliary HTTP calls.
The permit is held until the upstream stream ends. Relay send starts are at
least 15 seconds apart. A 429 is retained as a failed request; the next request
waits for a valid `Retry-After`, or at least 30 seconds when it is absent or
invalid, and still respects normal pacing. Two independent native Codex limits
are frozen in `mbtx/config/relay.toml`: `request_max_retries = 2` for transient
HTTP/transport failures and `stream_max_retries = 2` for agent sampling stream
reconnections. Each counts retries after the initial try and accepts 0..5;
zero disables that layer. `unbounded_connection_retries` is explicitly false.
The gateway adds no hidden retries, and 429 is not retried. Every retry traverses
the same serialized, paced gate. Nested HTTP and stream recovery can send at most
nine HTTP attempts per logical sampling call with the defaults; this is not a
limit on successful task decisions, tool calls or total task requests.

Native evidence counts `http_transport_retries` and `agent_stream_retries`
separately. Failed requests remain immutable even when a later retry succeeds.
A normally completed session with correct artifacts can be successful with
`external_errors_recovered = true`; the report also counts recovered-error arms
and retains every external failure. Exhausted recovery remains an external
failure and completion steps stay null. The complete policy is stored in the
run manifest; changing it requires a new run, not resuming old conditions.
A host-level account lock prevents two collectors on this
host from sharing the account concurrently; other applications or hosts remain
outside this control.

The initial probe needs one success in up to three attempts. Collection pauses
after five consecutive infrastructure-failed arms, eight infrastructure failures
in the latest sixteen arms, or loss of a core collection component. Isolated
relay failures do not invalidate other evidence. Completed pairs and failed
attempts are never replaced. Resume runs only missing schedule arms; an existing
interrupted attempt remains censored. A separate rerun is an additional run,
never an overwrite of an earlier sample.

## Running on Linux

Install the standard build prerequisites and MoonBit toolchain described in the
stage-one guide. Install `git`, `jq` and `ripgrep`. Run from the checkout root:

```sh
git pull --ff-only origin main
moon run mbtx/scripts/validate-evaluation.mbtx
export OPENAI_API_KEY='YOUR_PRIVATE_KEY'
moon run mbtx/scripts/collect-pilot.mbtx relay
```

Keep the key in the environment or pass a private JSON credential file using
`--credentials-file`. Do not add it to Git. `mbtx/config/relay.toml` contains only
non-secret model/provider configuration. A model, guide, pacing, cache or task
change requires a new condition; it cannot resume an existing run.

After reviewing the independent pilot, start the fixed formal schedule:

```sh
moon run mbtx/scripts/collect-study.mbtx relay --output _build/long-study-linux
moon run mbtx/scripts/collect-study.mbtx relay --output _build/long-study-linux --resume
```

Each invocation attempts up to ten additional pairs. Repeat the resume command
until the schedule is finished or the infrastructure gate stops it. Explicitly
setting `--batch-pairs 80` runs the entire schedule in one invocation. The same
bundle and environment must remain available for resume.

```sh
MBTX_BUNDLE=$(cat _build/evaluation-bundle.txt)
"$MBTX_BUNDLE/mbtx-eval" log _build/long-study-linux --follow
"$MBTX_BUNDLE/mbtx-eval" report _build/long-study-linux --format all
moon run mbtx/scripts/package-evidence.mbtx _build/long-study-linux
```

Report reconstruction does not require API credentials. Original attempts are
sealed and hash-verified. Derived analysis can be cached outside those seals;
deleting the derivative cache forces reconstruction from raw evidence.

## Reading the report

`report.html` opens directly from a local file. ECharts 6.0.0 and all report data
are embedded; no CDN, subscription, server or account is involved. Each attempt
is compressed separately and decoded on demand. Every recorded step remains
available, including long or unfinished trajectories. Views cover completion,
filtered task comparison, paired trajectories, step details and methods/evidence.
The complete JSON, Markdown and CSV exports use the same analysis model.
CSV includes unstarted assignments. Original HTTP/SSE bytes, binary artifacts
and native OTLP batches remain in the accompanying evidence archive.

The first analysis includes every assigned arm, with unstarted arms separate.
The started population includes all failures. Conditional paired step analysis
requires both successes, equal starting baselines and complete step evidence.
Workflow and program-delivery cohorts are analyzed separately. Absolute paired
differences and MBTX/Shell step ratios use a fixed-seed, 2,000-draw percentile
bootstrap with category/complexity strata and nested input/repetition sampling.
An interval requires at least two distinct inputs in every observed category
and complexity stratum. Pilot estimates are
descriptive. Differential failure can select the successful subset; intervals
do not establish an unconditional advantage or generalize to all tasks.

Timing is diagnostic. Native trace timestamps are captured at writer entry,
before its lock and disk write. Gate timestamps use a separate monotonic epoch.
Only timestamps within the same clock domain may be subtracted. Process wait
is an observed boundary, not a kernel exit timestamp. Request windows contain
client transport and observation backpressure; relay internal waiting and
model computation cannot be separated without server evidence. Build, fixture,
pacing, MBTX preparation/compile/run, drain, snapshot and independent validation
costs are identified where observed. Overlapping intervals are not added into
a fabricated complete attribution, and unexplained time remains unknown.

`validate-observation.mbtx` runs a balanced local calibration with native OTLP
enabled and disabled, retaining required audit capture in both conditions. It
checks fixed-trajectory behavior and records the incremental cost; it does not
claim to measure all observation overhead or remove it from online results.

## Acceptance boundaries

Local fixed replay establishes functional coverage, measurement reconstruction
and fault handling. It does not show autonomous-model benefit. Linux/macOS data
is kept separate. Online pilot and formal outcomes must be reviewed after user
collection. Browser interaction checks must name the actual browsers tested;
syntax checks and successful HTML generation alone are not cross-browser QA.
Default upstream Shell behavior remains unchanged.

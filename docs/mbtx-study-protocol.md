# Programmable execution study

This protocol extends the eight-task pilot into a paired study of configured
Codex execution interfaces. It measures successful program delivery and model
decision rounds. It does not measure CPU instructions, prove that Shell cannot
batch commands, or treat end-to-end latency as pure runtime speed.

The [implementation validation record](mbtx-study-validation.md) separates local
offline checks from pending Linux online research acceptance.

## Treatments and acceptance

The primary `programs` cohort requires a saved, reusable `solution.sh` or
`solution.mbtx`. A fresh instance executes the submitted program on the visible
input and two withheld inputs without further model assistance. The evaluator
compares outputs and input preservation against independent frozen oracles.
Writing a correct literal answer to the visible result file is insufficient.

The current program-delivery protocol is `programmable-steps-programs-v2`.
Each arm receives an explicit instruction identifying its required source file
and language. The shared task goal, inputs and oracles remain identical. These
arm instructions are frozen in `run.json` and retained in the actual model
requests. Earlier v1 instructions presented both submission alternatives to both
arms, which allowed a consistent wrong-file interpretation in an interrupted
online run. See the [evidence diagnosis](mbtx-study-diagnosis-2026-09-17.md).
Do not pool v1 and v2 samples or resume a v1 run with the new bundle. Existing
v1 outcomes remain unchanged; offline replay cannot prove live-model compliance.

Both interfaces retain the same common editing tools and system utilities.
Editing program source through `apply_patch` is legitimate in both conditions.
An MBTX program may call native utilities, including Shell; source evidence and
explicit attribution labels must preserve that fact. A successful program is
not automatically a pure MoonBit implementation. Withheld validation has its
own process, build, output and timing evidence and does not add model steps.

The independent `workflow` cohort accepts the final task artifact, including
results produced through common file-editing tools. It is reported separately
and never pooled with program-delivery samples or historical pilot runs.

Each attempt has a fresh Git repository with a deterministic fixture commit,
isolated HOME and Git configuration, private temporary storage, and filesystem
permissions confined to its work area plus the recorded compiler and utilities.
Task processes cannot read evaluator manifests, other attempts, package-publishing
credentials or the parent checkout. The bundle freezes MoonBit's registry index
as well as dependency sources; compiler binaries and core libraries are hashed
and checked. Both arms use the same fixtures, library cache, model settings,
permissions and the configured Codex output allowance. Native tool response
contracts remain explicit: Shell accepts token-based output requests; MBTX
accepts a combined stream byte budget (default 1,024, maximum 4,096), plus its
structured build/run envelope. These are not identical byte/token limits. Both
tools' actual arguments, outputs and truncation evidence are retained. Any
interpretation must include these interface policies; fewer steps alone cannot
establish a MoonBit-language advantage independent of output delivery. No model
request is silently rewritten.

The permission profile also grants read access to the bundle's `codex` executable.
Linux bubblewrap re-enters that executable to install seccomp before it starts
the task. This is a file-level grant shared by both arms and submission validation;
it does not expose the enclosing bundle directory or evaluator manifests.

## Coverage and assignment

The study covers 24 scenarios across eight families. Each scenario has four
deterministic visible-input variants and independent withheld-input instances.
Two repetitions per visible variant give 192 planned pairs (384 arms). A
one-repetition run has 96 pairs and is explicitly identified in its manifest.

| Family | Scenarios |
| --- | --- |
| Repository inspection | `nested-symbols`; `dependency-order`; `filtered-inventory` |
| Structured data | `grouped-totals`; `relational-join`; `time-windows` |
| Text processing | `unicode-records`; `quoted-records`; `multiline-records` |
| Process orchestration | `batch-children`; `stdin-argv`; `child-failure-recovery` |
| Diagnostics | `severity-counts`; `request-correlation`; `test-diagnostics` |
| Repair and migration | `config-migration`; `constrained-repair`; `config-precedence` |
| Bounded output | `large-log`; `frequency-topk`; `multifile-aggregate` |
| Recovery | `checkpoint-replay`; `idempotent-updates`; `transaction-reconcile` |

Variants change content and size rather than merely renaming a task. The program
contract fixes file names, types, ordering, duplicate handling and error rules.
Expected outputs and reference replay programs remain evaluator-private.
The Unicode scenario specifies code-point sorting and exact deduplication, not
NFC normalization. Process-task output oracles do not by themselves establish
which subprocesses an autonomous program invoked: OS spawn counts remain unknown.
Saved source and native tool evidence must accompany any mechanism explanation.

A fixed seed determines pair order. Arm order alternates and reverses between
repetitions for the same input variant. A batch limit stops only between pairs.
Resume preserves completed attempts and executes only unassigned work. Failed
attempts, interrupted work and partial batches remain visible; no performance-
based replacement or successful-sample-only stopping is permitted.

## Analysis

The primary results are intention-to-treat success and success by decision-round
budget. Paired step differences are conditional on both arms succeeding with
complete evidence and equal recorded initial workspace commits. Unknown or
unequal baselines exclude a pair only from the strict comparison; its assigned
outcomes remain in ITT with the exclusion reason. Report program validation, common-tool use, compiler errors,
runtime errors and external failures independently. A condition with fewer steps
but lower success must not be declared unconditionally superior.

Aggregate by scenario and family as well as overall. Input variants and repeated
attempts are nested observations, not additional independent task families.
Use a fixed-seed hierarchical paired bootstrap for uncertainty, with paired
resampling that preserves the family/scenario/input/repetition structure.
Intervals describe this benchmark population and its sampling assumptions;
they do not establish universal task generalization. Partial runs explicitly
report missing coverage and unknown outcomes.
The implementation uses 2,000 percentile-bootstrap draws with seed 20260916.
Families are fixed strata; scenarios, input variants and repetitions are resampled
within their parent group. The point estimate uses equal weights at each level.
Only strata with comparable pairs contribute to the conditional estimate; missing
strata remain in the coverage and ITT tables. Fewer than two observed input
variants yields a null interval. Small and partially observed strata do not
support strong generalization or ranking claims.

## Pacing and delivery

Keep model-request concurrency at one and start intervals at least 15 seconds.
Retain 429 outcomes and honor Retry-After, using 30 seconds when absent or invalid.
Preserve the existing three-probe qualification and infrastructure-failure stop
rules. A build bundle is prepared once and reused across batches. Compilation
of model-authored programs and independent validation are recorded separately.

At the pilot's observed cost, 192 pairs would take approximately eight hours;
larger programs, repairs and provider delay can increase this substantially.
This is a planning estimate, not a collection deadline or a reason to drop slow
samples. Linux online collection is user-operated. Local implementation checks
use deterministic Responses replay and do not spend relay requests.

## Linux operation

Run from the fork's repository root. Reuse the working relay TOML and private
credentials described in [stage three](mbtx-stage-three.md). No system-wide
Codex login or new API account is needed.

```bash
git pull --ff-only origin main &&
moon run mbtx/scripts/validate-evaluation.mbtx &&
export MBTX_BUNDLE="$(cat _build/evaluation-bundle.txt)"
```

Validation prepares the bundle once, runs the offline checks, and leaves its path
in `_build/evaluation-bundle.txt`. It does not forward any requests to the relay.
The final export replaces any stale bundle selection left in the terminal.
Use `prepare-evaluation.mbtx` alone when only a fresh build bundle is required.

Continue to online collection only after validation succeeds. Study validation
first executes one fixed pair through the public collection script, checks the
retained schedule and successful outcomes separately, then resumes the other 23
pairs. A failing first pair stops validation and prints tool errors and evidence
paths. All standalone collection entry points are compiled in the fast checks.

For a retained failed run, inspect the first unsuccessful attempt from each arm
without rebuilding, editing its evidence or sending API requests:

```bash
moon run mbtx/scripts/diagnose-study.mbtx _build/study-validation/run-TIMESTAMP
```

The script writes a diagnostic text file beside the run and includes native
tool output, MBTX build/run details, submission stderr, and per-request queue,
response-header and terminal timings when available. Missing timing stays null.
An attempt timeout is an overall Codex deadline, including model requests and
local pacing. It is not a Shell or MBTX process timeout. Cancellation before
response headers records a local cancelled request with unknown upstream status;
it is not automatically classified as an external error. Linux
`bwrap: execvp .../codex: No such file or directory` is a sandbox bootstrap
failure before task execution, not evidence of a Shell/MBTX capability difference.
Keep failed evidence; after changing the bundle or permission profile, validate
and collect into a new run rather than resuming the old experimental condition.

The default study plans 192 pairs. The following command completes up to 24 new
pairs per invocation, retaining the entire frozen schedule:

```bash
export MBTX_RUN="_build/programmable-study/Linux/relay/study-$(date -u +%Y%m%dT%H%M%SZ)"
moon run mbtx/scripts/collect-study.mbtx relay --output "$MBTX_RUN" --batch-pairs 24 --credentials-file /private/path/credentials.json
```

Alternatively, supply `OPENAI_API_KEY` in the collector's environment. After a
normal batch stop, resume the same run with the original options:

```bash
moon run mbtx/scripts/collect-study.mbtx relay --output "$MBTX_RUN" --batch-pairs 24 --resume --credentials-file /private/path/credentials.json
```

Omit `--batch-pairs` to finish all remaining pairs in one invocation. Set
`--repeats 1` **at initial creation** for a 96-pair study, and repeat that option
on resume. `--scenarios grouped-totals,large-log --variants 1 --repeats 1` defines
a separate two-pair smoke run; never append it to the formal cohort. Variants
are selected with one-based CLI indices; raw metadata stores zero-based indices.
Use `--suite workflow` with a different output directory for the secondary
workflow condition. No suite, model, seed, bundle or selection may change on resume.

Batching never retries completed failures or rewrites an interrupted attempt.
After an external hard kill, stop the recorded owned Codex process before resume.
The existing infrastructure-failure gate can stop within a pair; the scheduled
other arm remains explicitly unstarted. Do not delete evidence to bypass it.

```bash
export MBTX_BUNDLE="$(cat _build/evaluation-bundle.txt)"
"$MBTX_BUNDLE/mbtx-eval" log "$MBTX_RUN" --follow
"$MBTX_BUNDLE/mbtx-eval" report "$MBTX_RUN" --format all
moon run mbtx/scripts/package-evidence.mbtx "$MBTX_RUN"
```

All four report formats, frozen manifests, native traces, source hashes and
validation outputs are retained. Submission validation has three fresh executions
per arm, plus one compilation for an MBTX submission. It has no model calls and
is excluded from agent-step and Codex elapsed measurements. Its process elapsed
values include the sandbox/capture boundary and are not launcher benchmarks.
All measured comparisons remain conditional on the recorded observation setup;
there is no claim of zero measurement perturbation. Inspect detailed trajectories
using the existing [SigNoz import workflow](mbtx-stage-three.md#reports-and-observability).

Historical pilot data, including the observed 43-versus-35 step result, remains
unchanged and separate from this protocol. Formal conclusions follow new data.

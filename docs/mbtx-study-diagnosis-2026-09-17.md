# Interrupted Linux program-delivery study: evidence diagnosis

The interrupted run identifies an ambiguous submission contract and substantial
external request delays. It does not support a comparative step-efficiency or
runtime-performance conclusion. No live API requests were made for this review.

## Evidence and observed outcomes

- Run: `run-1789608837051`, Linux, `programmable-steps-programs-v1`.
- Source revision: `515c70d3cb492876261c367d70bf65bc35605bb8`.
- Linux bundle: `e753fa1e85851f4d5677c616f16588e02ed817ea`.
- Original report: `63fa3251-f130-457b-a3e0-9ec25af3d658`.
- Input archive: `run-1789608837051-evidence.tar.gz`.
- Archive SHA-256:
  `c8f2806bca5512581cae5615e0afc7b0d953f9df81c085d0b0718407059f62be`.

The frozen schedule contains 192 pairs. Ten arms were started across five pairs:
Shell has three successes and two timeouts; MBTX has four task failures and one
censored attempt. The remaining 187 pairs were unstarted. There are **zero**
complete successful pairs eligible for conditional step comparison. The final
MBTX attempt has no terminal outcome or submission-validation evidence.

Reconstruction on a separate local copy produced a byte-identical report JSON.
The run/fixture integrity check and all nine sealed attempts passed; the unsealed
last attempt retained null integrity. The rebuilt report identifier is
`48a9db5e-1c92-4a1c-9032-d0e126cb712e`. The original archive and extracted run
were not changed or resealed.

| Task | Shell | MBTX | Observed explanation |
| --- | --- | --- | --- |
| `relational-join-v1` | Success, 7 steps | Task failure, 7 steps | MBTX saved `solution.sh`; required `solution.mbtx` absent |
| `batch-children-v1` | Success, 6 steps | Task failure, 5 steps | Same submission mismatch |
| `idempotent-updates-v1` | Timeout, 2 completed steps | Task failure, 6 steps | Shell awaiting a response; MBTX has the same submission mismatch |
| `test-diagnostics-v1` | Success, 5 steps | Task failure, 4 steps | Same submission mismatch |
| `time-windows-v1` | Timeout, 5 completed steps | Censored, 0 completed steps observed | Shell awaiting a response; MBTX interrupted before a completed step |

Completed-step counts on failed or censored attempts are not steps to success.
The preserved report's `task_failure` outcomes remain failures; the diagnosis
does not rename files, execute a substituted source, or retroactively pass them.

## Submission contract

All four completed MBTX attempts produced a visible `result.json` that passed
the independent oracle. Native edit events and final messages show that each
saved `solution.sh`. The evaluator correctly required `solution.mbtx` and
returned `missing_source`. Those programs were therefore not validated on fresh
visible or withheld inputs as MBTX submissions.

The actual user prompt offered “solution.sh for the Shell interface or
solution.mbtx for MBTX” without explicitly identifying the assigned delivery
contract. The assigned execution tool was MBTX, but it permitted native Shell
utilities; using that tool did not itself establish which saved file to deliver.
This ambiguity is an observed harness-design defect. Its contribution to future
model behavior must be tested; clarification cannot guarantee compliance.

The 14 observed MBTX tool invocations comprise 12 completed executions, one
generated-source compilation error, and one execution failure containing a
`jq` syntax diagnostic. The model subsequently produced correct visible output.
These intermediate errors remain part of the trajectory. They do not establish
a defect in MBTX's process launcher, nor does visible correctness establish
reusable-program correctness.

Protocol v2 puts the assigned filename and language in explicit arm instructions,
removes the two-way choice from the common goal, and retains the same independent
acceptance rules. It also states that temporary tool programs do not substitute
for the required saved source. Both arms receive the corresponding obligation.
Missing-source evidence now names the required file.

## Timeout attribution and request evidence

Both Shell timeouts reached the 600-second overall attempt limit while the next
request had started but no response headers had been observed. All dispatched
tools in those attempts had already returned. Model computation, relay queueing
and network delay cannot be separated without additional server evidence.

| Shell task | Codex elapsed | Sum of fully observed HTTP request intervals | Recorded pacing wait | Last request |
| --- | ---: | ---: | ---: | --- |
| `idempotent-updates-v1` | 600.058 s | 274.654 s across 2 responses | 0.176 s | No response headers or terminal record |
| `time-windows-v1` | 600.081 s | 538.868 s across 5 responses | 8.055 s | No response headers or terminal record |

HTTP intervals use `headers.send_ns` and `result.end_ns` from the same collector
clock. They are diagnostic boundary measurements that may include local proxy
processing; they are not pure provider-compute measurements. Pacing is reported
separately. Missing terminal intervals are not filled by subtracting totals, and
overlapping tool work is not added to manufacture exhaustive attribution.

The saved Shell program for `idempotent-updates-v1` passed all three later
submission checks, but the visible workspace had not been executed to completion
before timeout. For `time-windows-v1`, both the visible result and all three saved
program checks passed; final conversational completion still timed out. The
report appropriately keeps oracle success separate from attempt completion.

Across task attempts, 47 requests have recorded HTTP 200 responses and three
have unknown HTTP status. No observed task response is 429. This does not prove
the absent responses were healthy. The initial failed probe is separate from
task outcomes. Existing request serialization and the 15-second minimum start
interval remain unchanged; no retries or timeout increases were introduced.

The collector previously omitted `result.json` when an attempt was closed before
headers. The fix writes a local cancellation event with send/end timestamps,
phase `awaiting_headers`, and null HTTP status, transport-error status and
first-byte time before releasing the gate and sealing evidence. A hard-killed
collector can still leave missing evidence, which remains unknown.

## Revalidation and next collection

The original archive and imported evidence remain unchanged. Implementation
validation uses package tests, a controlled local HTTP server, and real Codex
with fixed local responses. Such replay verifies transport, instruction delivery
and program validation; it cannot establish autonomous live-model success or
step savings.

Validation on macOS ARM64 passed 20 MoonBit tests and 82 Rust tests across the
evaluator, MBTX extension and native trace packages. The new local-HTTP regression
verifies cancellation before response headers, gate drain, and an unchanged seal;
the analysis regression preserves local timeout/cancellation and unknown upstream
status. Real Codex replay passed 24 pairs, 144 fresh-input program executions,
immutable two-batch resume, and one separate workflow pair. The first pair checks
the actual outbound user/developer messages for the assigned submission contract.
The updated diagnostic script also completed against the supplied archive.

Local validation evidence:

- Bundle: `f3d178ed5359b310d0ff8302db3c3c56d1c80335`.
- Replay: `_build/study-validation/run-1789614310648`.
- Complete replay report: `c5003a65-1f46-4bce-a75c-c7e1f66e846b`.
- Workflow report: `3ca04189-56fc-452c-b557-b630b90cb18d`.
- Package and replay logs: `_build/study-diagnosis-fast.log` and
  `_build/study-diagnosis-replay.log`.

Linux execution of the repaired code and autonomous v2 collection remain
user-operated. The full legacy fault matrix was not repeated in this targeted
repair; existing package coverage and the new cancellation regression passed.

After offline validation on Linux, use a new, separately labelled two-pair smoke
run before committing to the full schedule:

```bash
git pull --ff-only origin main
moon run mbtx/scripts/validate-evaluation.mbtx
export MBTX_BUNDLE="$(cat _build/evaluation-bundle.txt)"
moon run mbtx/scripts/collect-study.mbtx relay \
  --scenarios relational-join,batch-children --variants 1 --repeats 1
```

Use the existing private credential setup. Retain both successful and failed
smoke outcomes. Inspect whether MBTX delivers an executable `solution.mbtx` and
passes fresh-input validation. A subsequent full study must use a different run
directory and the frozen v2 condition. No old outcome is replaced by the smoke.

# Linux programmable study: first 24-pair batch

This batch provides usable evidence about the configured model and execution
interfaces. It does not demonstrate an MBTX step advantage. Under the frozen
12-step completion budget, Shell completed 17 of 24 started tasks and MBTX
completed 10. The dominant observed MBTX difficulty was generated-program
compilation and repair, rather than process-launch latency.

## Provenance and scope

| Field | Value |
| --- | --- |
| Run | `run-1789622244695` |
| Platform | Linux x86_64 |
| Source revision | `b3878e7f42f98019d05bd579a580e2c4ec72ebce` |
| Collection bundle | `25036a24ae3803be23fd601f4eca4a7c726f7ba6` |
| Protocol / analysis | `programmable-steps-programs-v2` / `programmable-steps-v3` |
| Model / reasoning | `gpt-5.6-terra` / `xhigh` |
| Original report | `fb0618f4-2087-4b45-bc4d-265cfb6f0060` |
| Archive | `run-1789622244695.zip` |
| Archive SHA-256 | `0948290d86ef3907795ddf513d960c0261d1ba4dfc98b8ce7ed5515db7e2415b` |

The collector stopped normally at `batch_limit`, after 24 adjacent pairs and
48 finalized arms. The infrastructure gate remained open. The frozen plan still
contains 192 pairs; the remaining 168 pairs are unstarted. `partial=true` is
therefore expected and does not indicate a failed batch export. This prefix
covers 16 of 24 scenarios and seven of eight families, with no repair-family
samples. It contains no second repetitions yet.

All 48 attempt seals and the manifest/fixture integrity check passed on local
reconstruction. Rebuilt report `6638806b-cbb3-43e5-a7ca-c6e803f79405` is byte-for-byte
identical to the original JSON. Reconstruction used a separate copy; the archive
and imported raw evidence were not rewritten. Native traces, tool results,
submission sources and fresh-input validation are retained in the archive.
No live relay requests were made during this review.

The span from the first attempt's start to the last attempt's end was approximately
2.97 hours. This includes external requests, pacing and work between attempts;
it is not an MBTX runtime benchmark or a stable estimate for the full study.

## Completion and program correctness

The following denominators describe the 24 started pairs only. The original
intention-to-treat report retains all 192 planned assignments per arm, including
168 unstarted assignments; those unstarted assignments are not task failures.

| Outcome among started arms | Shell | MBTX |
| --- | ---: | ---: |
| Completed successfully | 17/24 (70.8%) | 10/24 (41.7%) |
| Step budget exhausted | 3 | 12 |
| Overall attempt timeout | 2 | 1 |
| Observed external failure | 2 | 1 |
| Harness error | 0 | 0 |
| Visible and saved-program oracles passed, regardless of completion | 19/24 | 16/24 |
| Oracle passed but step budget exhausted | 2 | 6 |

The last two rows overlap earlier rows. They separate a valid delivered program
from a completed Codex interaction. Six MBTX programs had already produced the
correct visible result and passed all three fresh-input checks, but no accepted
final completion arrived within 12 decision rounds. Two Shell attempts had the
same outcome. These remain `budget_exhausted` under the frozen protocol; they
must not be retroactively converted to primary successes. Work beyond the budget
was not observed, so eventual completion is unknown.

Nineteen MBTX attempts supplied the required `solution.mbtx`: 18 reached execution
validation and one failed submission compilation. Of those 18, 16 passed the
complete oracle. The other five attempts did not deliver the required source
before their budget, timeout or external termination. The previous systematic
delivery of `solution.sh` in the MBTX arm is no longer the common failure mode.

## Paired decision rounds

Seven pairs completed successfully on both arms with complete evidence and equal
initial workspace commits. These are the only pairs in the primary conditional
step comparison.

| Task | Shell steps | MBTX steps | MBTX minus Shell |
| --- | ---: | ---: | ---: |
| `batch-children-v1` | 9 | 9 | 0 |
| `test-diagnostics-v1` | 6 | 6 | 0 |
| `time-windows-v1` | 10 | 10 | 0 |
| `multiline-records-v1` | 4 | 11 | +7 |
| `time-windows-v3` | 7 | 11 | +4 |
| `checkpoint-replay-v3` | 6 | 7 | +1 |
| `request-correlation-v2` | 4 | 11 | +7 |

The frozen hierarchical analysis reports **+2.7 MBTX steps**, with a conditional
95% bootstrap interval of **+1.6 to +3.8**. It weights the observed hierarchy;
the unweighted seven-pair mean is 19/7, approximately +2.71. Three pairs tied,
four required more MBTX rounds, and none required fewer. These are conditional
results from a small, uneven prefix, with unequal success rates. The interval
does not establish a universal difference or account for unobserved task families
and success selection. Failed attempts' completed-round counts are not their
steps to success.

MBTX succeeded where Shell did not in `relational-join-v1`, `frequency-topk-v1`
and `child-failure-recovery-v3`. The corresponding Shell outcomes were timeout,
external failure and timeout. These show successful MBTX deliveries in those
instances; they do not establish an interface advantage under comparable external
conditions. Likewise, Shell-only successes do not imply lower execution latency.

## Observed sources of extra work

There were 194 MBTX tool calls: 65 compilation failures, seven execution failures,
and 122 completed executions. Thus 33.5% of MBTX calls failed compilation. All
12 MBTX attempts that exhausted the budget contained compilation failures,
ranging from two to five per attempt. These are observed failed tool calls,
not a counterfactual estimate of how many total steps a better interface would save.

Repeated compiler diagnostics include:

- Constructing read-only JSON variants directly instead of using constructors
  or `to_json`.
- Applying `to_owned` to an owned `String`, using unavailable `print`, `HashMap`
  or `Array.init` APIs, and invalid collection patterns.
- Missing `raise` or async imports, invalid escaping, and mutable-variable errors.

For example, `relational-join-v3` spent five calls on compilation errors and did
not save a program before the step limit. `multiline-records-v1` eventually passed,
but used 11 rounds versus Shell's four, including four compilation failures.
`idempotent-updates-v1` encountered four compilation failures and produced a
validated program before reaching the limit, but did not complete the session.

The seven execution failures contain program assertions, invalid `jq` indexing,
and an attempt to list a regular file as a directory. The available results do
not identify a common launcher or sandbox defect. Thirty-two of the 65 failing
compilations returned truncated diagnostics. That is an additional observed
interface limitation; its separate causal effect on subsequent rounds is not
identified by this batch. Tool-output allowances were disclosed as unequal byte
and token contracts in the protocol, so this is not a pure language comparison.

Some successful MoonBit submissions use `jq` or Shell utilities. For example,
the successful `relational-join-v1` program calls `jq` and writes its output through
MoonBit. Tool identities alone cannot establish a native-MoonBit algorithm benefit.

## External conditions and an observation defect

The report contains 400 task HTTP request records. Of these, 397 have HTTP 200
headers: 394 completed Responses streams and three `response.failed` streams
whose error is `upstream_error` / `Upstream service temporarily unavailable`.
The three other records have unknown status. No observed task response was 429.
Among requests with recorded send timestamps, the minimum start interval was
15.00259015 seconds and no recorded request intervals overlapped. Missing request
terminals prevent a universal statement about all intervals or server-side work.

The three timed-out attempts were waiting for external responses near the overall
600-second limit. The initial Shell `relational-join-v1` attempt completed no
decision round. MBTX `relational-join-v4` waited approximately 586 seconds for its
first response headers, leaving very little of the attempt budget for task work.
Model computation, relay-internal waiting and network delay are not separately
observable. These timeouts do not measure runtime speed.

Two requests reached the local gateway's send preparation but retained no final
request record, despite the previous cancellation fix. A local HTTP regression
reproduced the cause: when Codex disconnects before headers, Hyper drops the
handler future. That also dropped the in-flight exchange and its serialization
permit before the attempt's explicit cancellation handler could persist evidence.
The third incomplete request stopped earlier, before a recorded queue slot.

The repair gives the exchange a task lifetime independent of the client handler.
It retains the existing serialization slot until response completion or owned
attempt cancellation. Cancellation can then record its unknown upstream status
and terminal timestamp before draining and sealing. It introduces no retry,
parallel API call, altered task budget or source correction. The new regression
failed on the previous implementation and all 17 evaluator tests passed after
the repair, including connected cancellation, disconnected cancellation, stream
serialization, cooldown and evidence verification. This fixes future observation;
it does not fill missing timings in this batch or alter its outcomes.

A real-Codex fixed replay of `relational-join-v1` also passed locally on macOS
after the repair: both arms completed, all six fresh-input executions passed,
and both attempt seals and the manifest integrity check passed. Replay report
`3ec2df51-15ad-4887-a3cf-cb33df337166` used bundle
`6ee66c6e3b5c29c807d346e489e95a07cf5b6d21`. This validates the repaired collection
path with fixed responses; it is not an additional autonomous model sample or
Linux validation of the repair.

## Interpretation and next experiment

The supported conclusion is that this model, tool description, output policy
and 12-step budget currently produce less reliable completion and more rounds
for MBTX on the observed task prefix. The experiment has identified concrete
MoonBit authoring and diagnostic problems worth improving. It has not measured
an intrinsic MBTX runtime disadvantage or demonstrated a step reduction.

Before investing in the remaining 168 pairs, improve and validate generic
MoonBit API guidance and diagnostic access, then collect a separately identified
small live pilot. Do not supply task answers or quietly raise only one arm's
budget. Changes to tool guidance, output contracts or stopping rules create a
new experimental condition and must not be pooled with this frozen v2 batch.
The existing batch remains valuable baseline evidence and should be retained.
If the original baseline is continued instead, it must use its original bundle
and recorded settings; a newly prepared bundle cannot be substituted on resume.

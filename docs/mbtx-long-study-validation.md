# Long-workflow implementation validation

Date: 2026-09-18. Platform: macOS ARM64. Protocol:
`programmable-long-study-v1`. These checks concern implementation correctness
and evidence reconstruction. Model responses were local fixed Responses/SSE
fixtures. No live relay requests or autonomous online experiments were run.

## Verified behavior

| Check                       | Observed result                                                                                                                                                                                                                                                  |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Shared MoonBit model        | 25 tests passed, including incorrect artifacts, invented worker execution, disabled jobs, dependency ordering, external failures, unknown timing and insufficient within-stratum input coverage.                                                                 |
| Scoped Rust packages        | 193 tests passed across tools, MBTX extension, native rollout trace and evaluator. A subsequent evaluator check passed all 21 tests after the report memory optimization.                                                                                        |
| Shared configuration and IO | 356 tests passed across configuration and PTY/process utilities, including EOF, split streams, process-group cancellation and draining after observed exit.                                                                                                      |
| Actual Codex integration    | Nine tests passed: approval denial, sandbox denial, source/filename execution, cache reuse, changed-source invalidation, timeout and interrupt/reap behavior. Source and binary artifact receipts were verified.                                                 |
| Long task fixtures          | 70 fixture/reference executions passed, including pilot inputs and independent program-delivery cases.                                                                                                                                                           |
| Real Codex fixed replay     | All 20 category/complexity pairs completed: 40 successful arms with required artifacts and independent delivery validation.                                                                                                                                      |
| Removed count cutoffs       | Both arms completed 28 accepted decisions, 53 tool calls and 28 native request starts. Repeated MBTX source produced 25 compilation-cache hits while still executing fresh programs.                                                                             |
| Recorded replay             | Re-executing the saved long trajectory in new workspaces retained 28 decisions, 53 calls and successful outcomes for both arms.                                                                                                                                  |
| External failures           | 429, 500, disconnect and missing terminal SSE events remained external failures, not backend failures.                                                                                                                                                           |
| Wall limit and interruption | Wall expiry retained censored outcomes and null completion steps. Actual collector SIGINT retained a cancelled attempt; SIGKILL retained a censored attempt. Reports were rebuilt from both.                                                                     |
| Immutable evidence          | Resume preserved the first pair's seals. Regenerated complete reports were equal. Reanalysis with the subsequently tightened job oracle retained all 40 successes.                                                                                               |
| Offline report              | A 512-step hostile fixture survived compression round-trip without omitted steps or changed bytes. Payloads remained non-executable data. A reviewed `insta` snapshot covers navigation and initial data. Partial resource metadata cannot block reconstruction. |

Full process stream tails are captured independently of context previews.
UTF-8 paging, resource isolation, failed writes, changed/removed cache inputs,
and interrupted capture are tested. Unobserved OS descendant totals and
ambiguous repair intent remain null/unknown.

## Evidence identifiers

Local evidence remains in ignored `_build/` directories; it is not a collected
autonomous research dataset:

- Full replay: `long-study-validation/run-1789671409064`, with sibling `-steps`,
  `-429`, `-500`, `-disconnect`, `-truncated`, `-stall`, `-collector-interrupt`
  and `-collector-kill` runs.
- Recorded replay: `long-study-validation/run-1789671409064-steps-recorded`.
- Calibration: `observation-calibration/run-1789672020418/calibration.json`.
- Rebuilt report ID: `7141853b-98cd-47a4-8e34-472588b4f453`, using analyzer bundle
  `911b9695dd10d4b931b84c763d879bbac7e06458`.
- Rebuilt JSON SHA-256:
  `5b557ef0ba1a75e54a757ca3924b5d8041a6e69502c78b7fba08b69f04c0c754`.
- Calibration SHA-256:
  `22daf4eadc7f0e6de3a989b39f4cae10edcf709a03013aa8d52b46f75cf3c5d3`.

Execution and analyzer bundles are identified separately. Later formatting does
not rewrite the recorded code or results. An unchanged-bundle check verifies
hashes without component compilation. Preparation costs remain outside attempts;
an adapter-only change reused Codex and reference artifacts. This is not a
controlled cold-build speed comparison with the previous implementation.

The replay covers one input per category/complexity stratum. The final analyzer
retains all 20 successful pairs and their point estimates, with confidence
intervals null because within-stratum input coverage is insufficient. Repeated
executions of one input do not substitute for independent input coverage.

## Observation calibration

Minimal/full/full/minimal conditions each ran two pairs: 16 arms. Both conditions
retained native decision records, full outputs and the Responses gate. Full
observation additionally enabled native OTLP. Every arm retained two decisions,
one tool call and a successful oracle outcome.

| Interface | Minimal mean process time (ms), n=4 | Full mean process time (ms), n=4 | Full minus minimal (ms) |
| --------- | ----------------------------------: | -------------------------------: | ----------------------: |
| Shell     |                              682.34 |                           594.78 |                  -87.56 |
| MBTX      |                             6155.52 |                          6147.66 |                   -7.85 |

These small-sample descriptive differences are consistent with run-to-run noise.
They do not show that tracing makes execution faster or costs nothing. This
isolates incremental OTLP export, not all observation overhead. Necessary MBTX
compilation and preparation remain tool costs. No average calibration value is
subtracted from online measurements. These are not launcher benchmarks.

## Validation limitations

The complete upstream workspace suite was attempted but did not complete. The
voice-host build requires unavailable GStreamer/pkg-config dependencies; the V8
companion build requested an upstream archive returning HTTP 404. Excluding
these components reached further builds, but linking exhausted local disk space.
A missing MBTX configuration initializer in the thread-manager sample was fixed.
The successful scoped checks must not be described as a successful full upstream
suite. Logs remain at `_build/workspace-validation*.log`.

Chrome, Firefox and Safari interaction tests were **not executed** because this
session lacked the required browser-control interface. Generation, JavaScript
syntax checks, compression tests and snapshots are not browser QA. Before
accepting browser coverage, open the same HTML offline in each browser and check:

1. Overview counts match JSON, including failed and unstarted arms.
2. Category, complexity and status filters update both table and chart.
3. Both trajectories expose every decision, with separate clocks and no assumed
   ordinal semantic matching.
4. Source, diagnostic tails and Unicode resources expand and export correctly.
5. Complete JSON export preserves all attempts, decisions and outcomes; hostile
   strings remain text and trigger no network requests.

## Linux handoff

```sh
moon run mbtx/scripts/validate-evaluation.mbtx
export OPENAI_API_KEY='YOUR_PRIVATE_KEY'
moon run mbtx/scripts/collect-pilot.mbtx relay
```

Review the independent ten-pair pilot before the fixed 80-pair study. The
[protocol](mbtx-long-study.md) documents batching, resume, logs, report generation
and evidence packaging. Linux results, browser acceptance and autonomous-model
conclusions remain subsequent acceptance work. No MBTX step-reduction benefit
is inferred from prescribed replay results.

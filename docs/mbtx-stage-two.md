# Stage Two: Agent-Step Accounting

Stage two adds an auditable count of logical model decision rounds to the
native Codex rollout trace. It does not claim that programmable MBTX reduces
steps. Linux execution of the validation entry remains user-operated.

## Measurement boundary

An `agent_step` starts when the task loop enters one sampling round and ends
when that round is accepted, fails, is cancelled, is preempted, or is abandoned.
An accepted final answer is included. Several tools emitted by one response
belong to one step. The next accepted response after those tool results is a
new step.

The trace distinguishes this logical identity from a concrete provider request.
Transport retries and endpoint fallbacks create additional request attempts
inside the same step. Compaction and other auxiliary model requests are tagged
with their purpose and do not become task steps. A missing boundary keeps the
affected aggregate unknown rather than turning it into zero.

The resulting counters are:

| Counter | Meaning |
| --- | --- |
| `agent_steps_started` | Sampling rounds entered, including failed and interrupted rounds. |
| `agent_steps` | Rounds whose response was accepted by the task loop. |
| `model_requests` | HTTP inference send attempts started at the transport boundary; `null` when that coverage is incomplete. |
| `http_transport_retries` | Sends after attempt zero within an observed request scope. |
| `tool_calls` | Model-emitted tool calls, including calls whose stream later fails. |
| `tool_executions` | Calls that reached the canonical dispatch boundary. |
| `poll_or_input_calls` | Explicit polling or input operations when visible in the trace. |
| `tool_errors` | Runtime tool failures; `null` if any tool result is missing or pending. |
| `repair_steps` | `null` until a repair attribution is proven by task evidence. |
| `process_spawns` | `null` until the same trace has complete process observation. |
| `task_success` | `null` in this layer; task oracles belong to evaluation. |

The native trace remains the source of facts. Each step, request scope, request
send, tool emission, and dispatch event is appended to `trace.jsonl`. The
reducer copies these events into `state.json`; the MoonBit evaluator derives
counts and marks coverage. It never reads credentials, environment variables,
or the current machine state when rebuilding a report.

`complete` describes accounting coverage within this capture, not task success
or coverage of every model endpoint. A fully observed 429 failure can have
`complete: true` and zero accepted steps. An unsealed capture, missing terminal
step, missing tool result or unsupported request transport restricts the
corresponding totals while retaining observed prefix counts.

## Implemented observation

`codex-rollout-trace` now provides:

- `AgentStepGuard` and `AgentStepContext` for the task-loop boundary;
- `StepObservation` events for start/finish, inference association, tool
  emission/dispatch, and actual HTTP sends;
- request telemetry callbacks before and after each send, including failures;
- task, compaction, and auxiliary request-purpose labels;
- associations between compaction lifecycles and concrete inference sends, so
  the same work is not counted twice;
- deterministic replay warnings for an intact prefix ending in a partial line;
- sequence-gap rejection and exclusive bundle creation to prevent overwrite;
- `mbtx/evaluation` analysis with explicit observed counts, coverage, warnings,
  inferred repair candidates, and unknown fields.

The native implementation reuses existing rollout trace and OTel request
telemetry. It does not introduce a second model client or a parallel log
format. WebSocket sends remain explicitly outside complete HTTP coverage until
their native callback has the same request-start boundary.

## Offline fixed replay

The integration suite in `codex-rs/core/tests/suite/step_accounting.rs` uses a
local fixed Responses server and hand-written expected counts. It covers:

1. one response with several successful `exec_command` calls followed by a final response;
2. a retryable provider failure, which creates two sends and one step;
3. an exhausted rate-limit response, which leaves a started but unaccepted step;
4. a partial stream after a verified file-write side effect;
5. interrupt followed by resume without rewriting the previous evidence;
6. manual compaction as request work outside task-step totals;
7. MBTX compiler rejection followed by a fixed corrective program in the next response.

The fixed replay is a measurement validation, not an autonomous capability
benchmark. It verifies that the counter rules match the raw event sequence.

## Linux validation

From the repository root, run:

```bash
moon run mbtx/scripts/validate-steps.mbtx
```

The entry builds the Codex CLI and MoonBit evaluator once, runs the native
trace/request and MoonBit analysis tests, runs the single-threaded fixed
replay, and reduces every newly completed bundle with:

```bash
codex-rs/target/debug/codex debug trace-reduce TRACE_BUNDLE
```

The prebuilt evaluator checks `state.json` against the handwritten
`expected-steps.json` and saves `step-report.json` in each bundle. The script
never contacts a model provider or relay; all Responses traffic uses a local
test server. Build dependencies may need downloading on a fresh installation.
The test subprocesses bypass HTTP proxies for loopback addresses, so local
fault responses and connection failures are not transformed by a host proxy.

Each invocation prints and creates a new directory under
`_build/stage-two-step-validation/run-TIMESTAMP/`. Prior directories are retained,
including incomplete runs. This is local validation output, not evidence for
an online research conclusion. To share Linux validation, retain the terminal
output and that invocation's entire directory, including manifests, raw
`trace.jsonl`, payloads, expected counts, reduced state and step reports.

## Acceptance boundary

Implementation validation on macOS ARM64 on 2026-09-16 passed 7 MoonBit tests,
229 native trace/API/client tests, 7 real Codex fixed replays, and 2 existing
compaction/resume regression tests. The replay produced 8 bundles (interruption
and resume have separate captures); all reduced and matched handwritten counts
with complete accounting coverage. Tests also check that repeated reduction
preserves raw bytes and counts. This is scoped validation, not the complete
upstream workspace suite or Linux validation.

Stage two is accepted when the Linux command completes with all fixed replay
assertions and evaluator comparisons passing, every new bundle reduces, and
the raw bundle can be replayed a second time without changing bytes or counts.
This establishes trustworthy step accounting. It does not establish a step
reduction advantage. The next stage can then define equal-goal open-ended tasks,
shared oracles, SigNoz views, and a paced Linux pilot.

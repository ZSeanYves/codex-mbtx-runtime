# Expanded study implementation validation

This record covers local implementation checks on macOS ARM64 on 2026-09-16.
It is not an autonomous model comparison. No real relay requests were made.
Linux sandbox and online acceptance remain user-operated.

## Verified behavior

| Layer | Observed result | What the result supports |
| --- | --- | --- |
| MoonBit analysis | 19 tests passed | Independent oracles, unknown evidence, strict pairing, deterministic hierarchical intervals and existing step accounting |
| Rust packages | 79 tests passed across the evaluator, MBTX extension and native trace packages; after the final bundle regression was added, all 14 evaluator tests passed | 80 distinct passing tests, including serialization, cooldown, workspace isolation, process capture, report escaping and bundle integrity |
| Fixture validation | 288 checks passed: 96 visible variants and 192 withheld inputs | Independently calculated MoonBit answers agree with the private reference programs; inputs remain unchanged |
| Actual Codex study replay | 24 pairs / 48 arms passed; 144 post-submission executions passed | One visible variant of each scenario traverses Codex, the sandbox, MBTX compilation where applicable, and fresh-input validation |
| Batching | Two 12-pair batches completed with unchanged first-batch seals | Resume retains completed evidence and the full planned denominator |
| Independent workflow condition | One additional pair passed | Artifact acceptance remains separate from reusable-program acceptance |
| Existing diagnostic replay | 16 fixed arms and 16 recorded-response arms passed | Existing tool paths and reconstruction still work |
| Faults and interruption | Eight arms with controlled 429, 500, disconnect and truncated-stream outcomes retained as relay errors; collector SIGKILL produced a censored partial report | External failure and interruption do not become an MBTX task failure or disappear from reports |
| Reuse | A second unchanged preparation reused the bundle without compiling Codex, the evaluator or fixtures | Build preparation remains outside measured attempts |
| Final evidence review | 24 strict pairs reconstructed; 48 attempt seals unchanged; reports identical with a different PATH and MOON_HOME | Reports use retained evidence and the selected analysis bundle, not current credentials or compiler configuration |
| Final process regression | One new `stdin-argv` pair and six fresh-input executions passed | The final process-observation handling works through real Codex and both submission paths |

The fixed replay intentionally prescribes two decision rounds per arm. Its equal
step counts validate accounting; they provide no evidence of an MBTX step benefit.
For replay only, the MBTX reference program invokes the Shell reference utility
through MBTX. This exercises the supported interface, not a native-MoonBit
algorithm comparison. The full 192-pair schedule has not been collected here.

## Local evidence identifiers

The following identifiers locate ignored local build artifacts. These artifacts
are not included as online research samples or committed with the source.

- Complete study replay: `_build/study-validation/run-1789561504254`.
- Complete offline pipeline: `_build/stage-three-validation/run-1789561834206`
  and its fault, interruption and recorded-replay siblings.
- Original tested bundle: `6103fd79c7c9eeba879a1305b162e6c3ca358781`.
- Final targeted-check bundle: `b62450ef0295576e93ff3b6b927c9072c5edbdbf`.
- Strict reconstructed report: study report `9ad4bc95-6539-42fc-ac3d-b8b2bdbb44ed`.
- Final fresh capture: `_build/study-recheck/run-1789562929462`.

The final bundle regression separately confirms that removing the recorded host
compiler prevents collection but still permits analysis-bundle verification;
altering the bundled analysis remains an error. Formatting and scoped Clippy
checks follow behavioral validation; no research result is inferred from them.

## Reproduction and remaining acceptance

Run `moon run mbtx/scripts/validate-evaluation.mbtx` from the repository root for
the complete offline pipeline, or append `--fast` for package checks only.
`just bazel-lock-update` completed without lockfile drift, and Bazel discovers the
separate fixture-worker binary. This is not a claim that the full Bazel test suite
was run.

Use the [study protocol](mbtx-study-protocol.md) for Linux setup, a separate online
smoke run, and the formal collection. Final research acceptance must inspect
success and missingness by arm and scenario, actual native-tool trajectories,
common utility usage, source validation, external faults and uncertainty. The
configured output contracts differ between tools and must remain part of the
interpretation. No universal advantage, pure-language advantage or default
backend switch is supported by this validation record.

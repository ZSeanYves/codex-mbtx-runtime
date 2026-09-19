// Lightweight DOM boundary tests; no browser or network dependency.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

class Element {
  constructor(tag) {
    this.tag = tag;
    this.children = [];
    this.dataset = {};
    this.events = {};
    this.textContent = "";
    this.value = "";
  }
  append(...children) { this.children.push(...children); }
  replaceChildren(...children) { this.children = children; }
  setAttribute() {}
  removeAttribute() {}
  addEventListener(name, callback) { this.events[name] = callback; }
}

function render(model) {
  const elements = new Map();
  const getElementById = (id) => {
    if (!elements.has(id)) elements.set(id, new Element("div"));
    return elements.get(id);
  };
  getElementById("report-data").textContent = JSON.stringify(model);
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, "report.js"), "utf8"), {
    document: { getElementById, createElement: (tag) => new Element(tag), querySelectorAll: () => [] },
    window: { addEventListener() {} },
    echarts: { init: () => ({ setOption() {}, resize() {} }) },
  });
  return getElementById;
}

function cells(element) {
  return element.tag === "td" ? [element.textContent] : element.children.flatMap(cells);
}

test("overview renders arrays, zero and unknown totals without blank cells", () => {
  const get = render({
    attempts: [{ task_id: "example", arm: "mbtx_program", pair_id: "p", tool_outcomes: {
      mbtx_compilations_observed: 0, process_denial_diagnostics_observed: [], child_process_count: null,
    } }],
    step_distributions: [{ complexity: "deep", arm: "mbtx_program", known_totals: 2,
      unknown_totals: 1, values: [9, 51], median: 30, range: [9, 51], design_target: [50, 100], within_target: 0 }],
    repeat_comparisons: [{ task_id: "example", arm: "mbtx_program", repetitions: [
      { repeat: 0, status: "task_failure", steps: 9, steps_to_success: null },
      { repeat: 1, status: "censored", steps: null, steps_to_success: null },
    ], first_two_step_difference: null, first_two_status_equal: false }],
  });
  assert.deepEqual(cells(get("step-distributions")), [
    "deep / mbtx_program", "2 / 1", "[\n  9,\n  51\n]", "30", "9 / 51", "[\n  50,\n  100\n]", "0",
  ]);
  assert.deepEqual(cells(get("repeat-comparisons")), [
    "unknown / example / mbtx_program", "Repeat 0: task_failure, steps 9, steps to success unknown; Repeat 1: censored, steps unknown, steps to success unknown", "unknown", "false",
  ]);
  const disclosure = get("execution-evidence").children[0];
  disclosure.open = true;
  disclosure.events.toggle();
  assert.deepEqual(cells(disclosure), [
    "example / mbtx_program / p", "0", "unknown", "0", "unknown", "unknown", "unknown / unknown",
  ]);
});

test("track denominators and interaction counters retain zero, partial and unknown evidence", () => {
  const get = render({
    by_track: [{ name: "basic", itt: {
      shell_tool: { assigned: 2, captured: 1, successes: 0 },
      mbtx_program: { assigned: 2, captured: 0, successes: 0 },
    }, conditional: { pairs: 0, mean_step_difference: null } }],
    attempts: [{ track: "basic", task_id: "task", arm: "shell_tool", pair_id: "p1",
      interaction: { first_successful_compile_step: null, reference_first_pages_observed: 1,
        reference_continuations_observed: 0, output_continuations_observed: 2, resource_reads_unknown: 0 },
      tool_outcomes: { mbtx_compile_failures: null, mbtx_execution_failures: null },
      accounting: { metrics: { http_transport_retries: 1, agent_stream_retries: null } },
    }],
  });
  assert.deepEqual(cells(get("track-summary")), [
    "basic / shell_tool", "1 / 2", "0 / 2", "0", "unknown",
    "basic / mbtx_program", "0 / 2", "0 / 2", "0", "unknown",
  ]);
  assert.deepEqual(cells(get("interaction-summary")), [
    "basic / task / shell_tool / p1", "unknown / unknown", "unknown", "1 / 0", "2", "unknown / unknown", "1 / unknown", "0",
  ]);
});

test("all-captured step distributions show both tracks without merging unknown prefixes", () => {
  const get = render({ track_step_distributions: [
    { track: "basic", arm: "shell_tool", known_totals: 2, unknown_totals: 1,
      values: [4, 12], median: 8, range: [4, 12] },
    { track: "complexity", arm: "mbtx_program", known_totals: 0, unknown_totals: 1,
      values: [], median: null, range: null },
  ] });
  assert.deepEqual(cells(get("track-step-distributions")), [
    "basic / shell_tool", "2 / 1", "[\n  4,\n  12\n]", "8", "4 / 12",
    "complexity / mbtx_program", "0 / 1", "[]", "unknown", "unknown / unknown",
  ]);
});

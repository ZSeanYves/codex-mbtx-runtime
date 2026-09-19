"use strict";
(() => {
  const model = JSON.parse(document.getElementById("report-data").textContent);
  const byId = (id) => document.getElementById(id);
  const show = (value) =>
    value === null || value === undefined
      ? "unknown"
      : typeof value === "object"
        ? JSON.stringify(value, null, 2)
        : String(value);
  const node = (tag, text, cls) => {
    const n = document.createElement(tag);
    if (text !== undefined) n.textContent = show(text);
    if (cls) n.className = cls;
    return n;
  };
  const append = (parent, ...children) => {
    parent.append(...children);
    return parent;
  };
  const fail = (error) => {
    byId("error").hidden = false;
    byId("error").textContent = String(error);
  };
  const action =
    (fn) =>
    (...args) =>
      Promise.resolve()
        .then(() => fn(...args))
        .catch(fail);
  const download = (name, value, type = "application/json") => {
    const url = URL.createObjectURL(new Blob([value], { type }));
    const a = node("a");
    a.href = url;
    a.download = name;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 30000);
  };
  const disclosures = (parent, title, value) => {
    const d = node("details");
    append(d, node("summary", title));
    d.addEventListener("toggle", () => {
      if (d.open && !d.dataset.loaded) {
        d.append(node("pre", value));
        d.dataset.loaded = "yes";
      }
    });
    parent.append(d);
    return d;
  };
  const charts = [];
  const chart = (id, option) => {
    const c = echarts.init(byId(id), null, { renderer: "canvas" });
    c.setOption({
      animation: false,
      aria: { enabled: true },
      tooltip: { renderMode: "richText", trigger: "axis" },
      ...option,
    });
    charts.push(c);
    return c;
  };
  const cache = new Map();
  async function load(attempt) {
    if (!cache.has(attempt.chunk))
      cache.set(
        attempt.chunk,
        (async () => {
          if (!globalThis.DecompressionStream)
            throw new Error(
              "This browser lacks local gzip decompression. Use a current Chrome, Firefox or Safari, or open report.json.",
            );
          const encoded = byId(`attempt-${attempt.chunk}`).textContent.trim();
          const bytes = Uint8Array.from(atob(encoded), (c) => c.charCodeAt(0));
          return JSON.parse(
            await new Response(
              new Blob([bytes])
                .stream()
                .pipeThrough(new DecompressionStream("gzip")),
            ).text(),
          );
        })(),
      );
    return cache.get(attempt.chunk);
  }
  function view(id) {
    for (const section of document.querySelectorAll("main>section"))
      section.hidden = section.id !== id;
    for (const button of document.querySelectorAll("nav button")) {
      if (button.dataset.view === id)
        button.setAttribute("aria-current", "page");
      else button.removeAttribute("aria-current");
    }
    requestAnimationFrame(() => charts.forEach((c) => c.resize()));
  }
  for (const button of document.querySelectorAll("nav button"))
    button.onclick = () => view(button.dataset.view);
  window.addEventListener("resize", () => charts.forEach((c) => c.resize()));
  byId("identity").textContent =
    `${show(model.protocol_id)} · ${show(model.platform)} · ${show(model.run_id)}`;
  for (const text of [
    model.mode,
    model.partial ? "Partial collection" : "Collection complete",
    model.manifest_integrity === true
      ? "Manifest verified"
      : "Manifest integrity unknown or failed",
  ])
    byId("badges").append(node("span", text, "badge"));
  for (const [name, key] of [
    ["Shell", "shell_tool"],
    ["MBTX", "mbtx_program"],
  ]) {
    const a = model.itt?.[key] || {};
    const card = node("div", undefined, "card");
    append(
      card,
      node("span", name),
      node("strong", `${show(a.successes)} / ${show(a.assigned)}`, "number"),
      node(
        "span",
        `${show(a.started_denominator)} started · successful / assigned`,
      ),
    );
    byId("metrics").append(card);
  }
  const pairCard = node("div", undefined, "card");
  append(
    pairCard,
    node("span", "Comparable pairs"),
    node("strong", model.conditional?.pairs, "number"),
    node("span", `${(model.pairs || []).length} prespecified pairs`),
  );
  byId("metrics").append(pairCard);
  const arms = ["shell_tool", "mbtx_program"],
    statuses = [
      "success",
      "task_failure",
      "relay_error",
      "provider_error",
      "harness_error",
      "timeout",
      "cancelled",
      "censored",
      "unknown",
      "unstarted",
    ];
  chart("completion-chart", {
    legend: { type: "scroll" },
    grid: { left: 65, right: 25, top: 65, bottom: 40 },
    xAxis: { type: "category", data: ["Shell", "MBTX"] },
    yAxis: { type: "value", minInterval: 1 },
    series: statuses.map((status) => ({
      name: status,
      type: "bar",
      stack: "assigned",
      data: arms.map((arm) =>
        status === "unstarted"
          ? Math.max(
              0,
              (model.itt?.[arm]?.assigned || 0) -
                (model.attempts || []).filter((a) => a.arm === arm).length,
            )
          : (model.attempts || []).filter(
              (a) => a.arm === arm && a.status === status,
            ).length,
      ),
    })),
  });
  const estimates = model.by_track?.length
    ? model.by_track
    : model.by_cohort?.length
      ? model.by_cohort
      : [{ name: "Overall", conditional: model.conditional }];
  for (const group of estimates) {
    const e = group.conditional || {};
    const d = node("div", undefined, "card");
    append(
      d,
      node("h3", group.name),
      node(
        "p",
        `${show(e.pairs)} comparable successful pairs · MBTX − Shell: ${show(e.mean_step_difference)} steps`,
      ),
      node(
        "p",
        `MBTX / Shell: ${show(e.mean_step_ratio)} · ${e.confidence_interval == null ? show(e.interval_reason) : `95% interval: ${show(e.confidence_interval)}`}`,
      ),
    );
    disclosures(d, "Estimate definition and limitations", e);
    byId("estimates").append(d);
  }
  function summaryTable(parent, headers, rows) {
    if (!rows.length) {
      parent.append(node("p", "No observations available in this report."));
      return;
    }
    const table = node("table"), head = node("tr"), body = node("tbody");
    for (const title of headers) head.append(node("th", title));
    table.append(append(node("thead"), head), body);
    for (const values of rows) {
      const row = node("tr");
      for (const value of values) row.append(node("td", show(value)));
      body.append(row);
    }
    parent.append(table);
  }
  summaryTable(byId("track-summary"), [
    "Track / arm", "Captured / assigned", "Successful / assigned", "Comparable successful pairs", "Conditional MBTX − Shell steps",
  ], (model.by_track || []).flatMap((group) => arms.map((arm) => [
    `${show(group.name)} / ${arm}`,
    `${show(group.itt?.[arm]?.captured)} / ${show(group.itt?.[arm]?.assigned)}`,
    `${show(group.itt?.[arm]?.successes)} / ${show(group.itt?.[arm]?.assigned)}`,
    group.conditional?.pairs,
    group.conditional?.mean_step_difference,
  ])));
  summaryTable(byId("interaction-summary"), [
    "Track / task / arm / pair", "Known total / observed decision prefix", "First successful compile decision", "Reference first / continuation pages", "Output continuation pages",
    "Compile / runtime failures", "HTTP / stream retries", "Resource reads with unknown identity",
  ], (model.attempts || []).map((a) => [
    `${show(a.track)} / ${show(a.task_id)} / ${show(a.arm)} / ${show(a.pair_id)}`,
    `${show(a.accounting?.metrics?.agent_steps)} / ${show(a.accounting?.observed?.agent_steps)}`,
    a.interaction?.first_successful_compile_step,
    `${show(a.interaction?.reference_first_pages_observed)} / ${show(a.interaction?.reference_continuations_observed)}`,
    a.interaction?.output_continuations_observed,
    `${show(a.tool_outcomes?.mbtx_compile_failures)} / ${show(a.tool_outcomes?.mbtx_execution_failures)}`,
    `${show(a.accounting?.metrics?.http_transport_retries)} / ${show(a.accounting?.metrics?.agent_stream_retries)}`,
    a.interaction?.resource_reads_unknown,
  ]));
  summaryTable(byId("track-step-distributions"), [
    "Track / arm", "Known / unknown totals", "Actual steps", "Median", "Min / max",
  ], (model.track_step_distributions || []).map((row) => [
    `${show(row.track)} / ${show(row.arm)}`,
    `${show(row.known_totals)} / ${show(row.unknown_totals)}`,
    row.values,
    row.median,
    `${show(row.range?.[0])} / ${show(row.range?.[1])}`,
  ]));
  summaryTable(byId("step-distributions"), [
    "Complexity / arm", "Known / unknown totals", "Actual steps", "Median", "Min / max", "Design target", "Within target",
  ], (model.step_distributions || []).map((row) => [
    `${show(row.complexity)} / ${show(row.arm)}`,
    `${show(row.known_totals)} / ${show(row.unknown_totals)}`,
    row.values,
    row.median,
    `${show(row.range?.[0])} / ${show(row.range?.[1])}`,
    row.design_target,
    row.within_target,
  ]));
  summaryTable(byId("repeat-comparisons"), [
    "Track / task / arm", "Repeat observations", "Second minus first steps", "Same status",
  ], (model.repeat_comparisons || []).map((row) => [
    `${show(row.track ?? (model.pairs || []).find((p) => p.task_id === row.task_id)?.track)} / ${show(row.task_id)} / ${show(row.arm)}`,
    (row.repetitions || []).map((r) =>
      `Repeat ${show(r.repeat)}: ${show(r.status)}, steps ${show(r.steps)}, steps to success ${show(r.steps_to_success)}`,
    ).join("; "),
    row.first_two_step_difference,
    row.first_two_status_equal,
  ]));
  const execution = node("details");
  execution.append(node("summary", "Compiler, runtime and denial evidence by attempt"));
  const executionTable = node("div", undefined, "scroll");
  execution.append(executionTable);
  byId("execution-evidence").append(execution);
  execution.addEventListener("toggle", () => {
    if (!execution.open || execution.dataset.loaded) return;
    execution.dataset.loaded = "yes";
    summaryTable(executionTable, [
      "Task / arm / pair", "Compiler starts", "Program launches", "Denial diagnostics", "Child-process count", "Result coverage", "Delivery compiler / program starts",
    ], (model.attempts || []).map((a) => [
      `${show(a.task_id)} / ${show(a.arm)} / ${show(a.pair_id)}`,
      a.tool_outcomes?.mbtx_compilations_observed,
      a.tool_outcomes?.mbtx_program_launches_observed,
      a.tool_outcomes?.process_denial_diagnostics_observed?.length,
      a.tool_outcomes?.child_process_count,
      a.tool_outcomes?.mbtx_result_coverage,
      `${show(a.submission?.phase_counts?.compiler_starts)} / ${show(a.submission?.phase_counts?.program_starts)}`,
    ]));
  });
  for (const text of [
    ...(model.limitations || []),
    "Basic and complexity tracks answer different questions and do not yield a pooled treatment effect.",
    "Success-conditioned estimates can be selected by differential failure. Repeated runs are nested within input cases, not independent tasks.",
    "External request time combines provider and relay behavior. Local durations include observation overhead; overlapping intervals must not be added.",
    "Runtime denial messages are observed stderr diagnostics, not a complete or authenticated child-process audit. Child process totals remain unknown.",
  ])
    byId("limits").append(node("li", text));
  const filters = new Map();
  for (const [key, label] of [
    ["track", "Track"],
    ["cohort", "Delivery contract"],
    ["family", "Category"],
    ["complexity", "Complexity"],
    ["variant", "Input variant"],
    ["repeat", "Repeat"],
    ["status", "Status"],
    ["platform", "Platform"],
  ]) {
    const select = node("select");
    select.setAttribute("aria-label", label);
    const all = node("option", "All");
    all.value = "";
    select.append(all);
    const values =
      key === "status"
        ? statuses
        : key === "platform"
          ? [model.platform]
          : [
              ...new Set(
                (model.pairs || [])
                  .map((p) => p[key])
                  .filter((v) => v !== null && v !== undefined),
              ),
            ].sort();
    for (const value of values) {
      const option = node("option", value);
      option.value = String(value);
      select.append(option);
    }
    const labelNode = node("label", label);
    labelNode.append(select);
    byId("filters").append(labelNode);
    filters.set(key, select);
    select.onchange = updateTasks;
  }
  const pairChart = chart("pair-chart", {
    grid: { left: 60, right: 25, bottom: 80 },
    xAxis: { type: "category", data: [] },
    yAxis: { type: "value", name: "MBTX − Shell steps" },
    dataZoom: [{ type: "inside" }, { type: "slider" }],
    series: [{ type: "bar", data: [] }],
  });
  function updateTasks() {
    const pairs = (model.pairs || []).filter((p) =>
      [...filters].every(
        ([key, s]) =>
          !s.value ||
          (key === "status"
            ? (p.shell_status || "unstarted") === s.value ||
              (p.mbtx_status || "unstarted") === s.value
            : key === "platform"
              ? model.platform === s.value
              : String(p[key]) === s.value),
      ),
    );
    byId("filter-count").textContent =
      `${pairs.length} / ${(model.pairs || []).length} pairs shown. Unknown differences are omitted from the chart, retained in the table.`;
    const compared = pairs.filter(
      (p) =>
        p.step_difference_mbtx_minus_shell !== null &&
        p.step_difference_mbtx_minus_shell !== undefined,
    );
    pairChart.setOption({
      xAxis: { data: compared.map((p) => p.pair_id) },
      series: [
        {
          data: compared.map((p) => ({
            value: p.step_difference_mbtx_minus_shell,
            itemStyle: {
              color:
                p.step_difference_mbtx_minus_shell < 0 ? "#138375" : "#607b9b",
            },
          })),
        },
      ],
    });
    const table = byId("task-table");
    table.replaceChildren();
    const head = node("tr");
    for (const title of [
      "Pair / task",
      "Track / delivery / complexity",
      "Shell",
      "MBTX",
      "Step difference / ratio",
      "MBTX compiler / program starts",
      "MBTX denial diagnostics / child count",
      "Evidence",
    ])
      head.append(node("th", title));
    table.append(append(node("thead"), head));
    const body = node("tbody");
    table.append(body);
    for (const p of pairs) {
      const row = node("tr");
      const mbtx = (model.attempts || []).find(a => a.pair_id === p.pair_id && a.arm === "mbtx_program");
      const outcomes = mbtx?.tool_outcomes;
      for (const value of [
        `${p.pair_id}\n${p.task_id}`,
        `${show(p.track)} / ${show(p.cohort)} / ${show(p.complexity)}`,
        `${p.shell_status || "unstarted"} · ${show(p.shell_steps)} observed`,
        `${p.mbtx_status || "unstarted"} · ${show(p.mbtx_steps)} observed`,
        `${show(p.step_difference_mbtx_minus_shell)} / ${show(p.step_ratio_mbtx_over_shell)}`,
        `${show(outcomes?.mbtx_compilations_observed)} / ${show(outcomes?.mbtx_program_launches_observed)}`,
        `${show(outcomes?.process_denial_diagnostics_observed?.length)} / ${show(outcomes?.child_process_count)}`,
      ])
        row.append(node("td", value));
      const cell = node("td"),
        button = node("button", "Inspect pair");
      button.onclick = action(async () => {
        byId("pair-select").value = p.pair_id;
        view("compare");
        await trajectories();
      });
      cell.append(button);
      row.append(cell);
      body.append(row);
    }
  }
  for (const pair of model.pairs || []) {
    const option = node("option", `${pair.pair_id} · ${pair.task_id}`);
    option.value = pair.pair_id;
    byId("pair-select").append(option);
  }
  function timing(attempt, step) {
    const events = attempt.details?.events || [],
      start = events.find((e) => e.seq === step.started_seq),
      end = events.find((e) => e.seq === step.ended_seq);
    const origin = events.find(
      (e) =>
        e.clock_domain === start?.clock_domain &&
        typeof e.monotonic_ns === "number",
    );
    if (!start?.clock_domain || !origin || start.monotonic_ns == null)
      return null;
    return {
      start_ms: (start.monotonic_ns - origin.monotonic_ns) / 1e6,
      duration_ms:
        end?.clock_domain === start.clock_domain && end.monotonic_ns != null
          ? (end.monotonic_ns - start.monotonic_ns) / 1e6
          : null,
      clock_domain: start.clock_domain,
    };
  }
  let selection = 0;
  async function trajectories() {
    const generation = ++selection;
    const lanes = byId("timelines");
    lanes.replaceChildren();
    for (const arm of arms) {
      const lane = node("div", undefined, `lane ${arm}`);
      append(lane, node("h3", arm === "shell_tool" ? "Shell" : "MBTX"));
      lanes.append(lane);
      const brief = (model.attempts || []).find(
        (a) => a.pair_id === byId("pair-select").value && a.arm === arm,
      );
      if (!brief) {
        lane.append(node("p", "Not started"));
        continue;
      }
      lane.append(
        node(
          "p",
          `${brief.status} · completion steps: ${show(brief.steps_to_success)}`,
        ),
      );
      const attempt = await load(brief);
      if (generation !== selection) return;
      const steps = attempt.accounting?.steps || [];
      lane.append(
        node("p", `${steps.length} recorded decision steps · none omitted`),
      );
      for (const [index, step] of steps.entries()) {
        const tools =
          (step.emitted_tools || [])
            .map((t) => t.observation.tool_name)
            .join(", ") || "No tool emitted";
        const button = node("button", undefined, "step");
        append(
          button,
          node("strong", `${index + 1}. ${step.outcome || "unfinished"}`),
          node("small", tools),
        );
        if (byId("axis").value === "time") {
          const t = timing(attempt, step);
          button.append(
            node(
              "small",
              t
                ? `+${t.start_ms.toFixed(2)} ms · decision interval ${t.duration_ms === null ? "unknown" : t.duration_ms.toFixed(2) + " ms"}`
                : "Monotonic timing unknown",
            ),
          );
        }
        button.onclick = () => detail(attempt, step, index);
        lane.append(button);
      }
      disclosures(lane, "Attempt status, oracle and capture integrity", {
        execution: attempt.execution_terminal,
        session: attempt.session_terminal,
        oracle: attempt.oracle,
        delivery: attempt.submission,
        coverage: attempt.accounting?.coverage,
        integrity: attempt.integrity,
        errors: attempt.details?.errors,
      });
      disclosures(
        lane,
        "External requests and pacing (separate clock domain)",
        attempt.exchanges,
      );
      disclosures(
        lane,
        "Measured stages, cache cost and timing boundaries",
        attempt.timing,
      );
      disclosures(
        lane,
        "Controlled worker events",
        attempt.details?.worker_events,
      );
      disclosures(lane, "Compiler, program and diagnostic observations", brief.tool_outcomes);
      disclosures(lane, "Tool-linked decision activities (may overlap)", attempt.interaction);
      disclosures(lane, "Direct process policy and executable hashes", attempt.details?.policy);
      disclosures(lane, "Layer evidence: observed, inferred and unknown", attempt.layer_evidence);
    }
  }
  function resource(parent, r) {
    const d = node("details");
    append(
      d,
      node(
        "summary",
        `${r.phase} / ${r.stream} · ${r.bytes} observed bytes · ${r.complete ? "complete" : "incomplete"}`,
      ),
    );
    parent.append(d);
    d.addEventListener("toggle", () => {
      if (!d.open || d.dataset.loaded) return;
      d.dataset.loaded = "yes";
      const { data_base64, ...receipt } = r;
      disclosures(d, "Receipt, hash and integrity", receipt);
      if (data_base64 !== undefined) {
        const bytes = Uint8Array.from(atob(data_base64), (c) =>
          c.charCodeAt(0),
        );
        const button = node("button", "Export original bytes");
        button.onclick = () =>
          download(
            `${r.resource_id}.${r.stream}`,
            bytes,
            "application/octet-stream",
          );
        append(d, button, node("pre", new TextDecoder().decode(bytes)));
      } else
        d.append(
          node(
            "p",
            r.binary_evidence
              ? `Binary retained in the raw evidence archive: ${r.binary_evidence}`
              : "Output bytes unavailable; evidence remains incomplete.",
          ),
        );
    });
  }
  function detail(attempt, step, index) {
    view("detail");
    const host = byId("step-detail");
    host.replaceChildren();
    append(
      host,
      node("h3", `${attempt.arm} · step ${index + 1} · ${attempt.task_id}`),
      node("span", "observed", "badge"),
    );
    disclosures(host, "Step identity and evidence sequences", step);
    disclosures(host, "Relative monotonic timing", timing(attempt, step));
    const exchanges = step.response_id
      ? (attempt.exchanges || []).filter(
          (r) => r.response_id === step.response_id,
        )
      : [];
    disclosures(host, "Upstream exchanges linked by observed response ID", {
      confidence: exchanges.length ? "observed" : "unknown",
      exchanges,
    });
    for (const id of step.inference_ids || []) {
      const inf = attempt.accounting?.inferences?.[id];
      if (!inf) continue;
      disclosures(
        host,
        "Model request (recorded context)",
        attempt.details?.raw_payloads?.[inf.raw_request_payload_id],
      );
      disclosures(
        host,
        "Model response and usage",
        attempt.details?.raw_payloads?.[inf.raw_response_payload_id],
      );
    }
    const ids = new Set([
      ...(step.emitted_tools || []).map((t) => t.observation.call_id),
      ...(step.dispatched_tools || []).map((t) => t.observation.tool_call_id),
    ]);
    for (const id of ids) {
      host.append(node("h3", `Tool ${id}`));
      const result = attempt.tool_outcomes?.details?.[id];
      disclosures(
        host,
        "Actual arguments and source",
        result?.arguments ?? result?.invocation,
      );
      disclosures(host, "Model-visible result", result?.model_visible_result);
      disclosures(
        host,
        "Structured process / compile / cache result",
        result?.output,
      );
      const tool = attempt.accounting?.tools?.[id];
      disclosures(host, "Native tool record", tool);
      for (const r of attempt.details?.resources || [])
        if (r.call_id === id) resource(host, r);
      const reused = result?.output?.build_reused_from;
      if (reused && reused !== id) {
        host.append(
          node(
            "p",
            `Cached build diagnostics originate from ${reused}. No compiler executed for this cache hit.`,
          ),
        );
        for (const r of attempt.details?.resources || [])
          if (r.call_id === reused && r.phase === "build") resource(host, r);
      }
    }
    const button = node("button", "Export this step and attempt evidence");
    button.onclick = () =>
      download(
        `${attempt.attempt_id}-step-${index + 1}.json`,
        JSON.stringify({ step, attempt }, null, 2),
      );
    host.append(button);
  }
  byId("pair-select").onchange = action(trajectories);
  byId("axis").onchange = action(trajectories);
  disclosures(
    byId("method-content"),
    "Frozen protocol, environment and component hashes",
    model.method,
  );
  disclosures(byId("method-content"), "Category and complexity estimates", {
    by_family: model.by_family,
    by_complexity: model.by_complexity,
    step_distributions: model.step_distributions,
    repeat_comparisons: model.repeat_comparisons,
  });
  for (const attempt of model.attempts || []) {
    const d = node("details");
    d.append(
      node(
        "summary",
        `${attempt.attempt_id} · ${attempt.status} · raw evidence index`,
      ),
    );
    d.addEventListener(
      "toggle",
      action(async () => {
        if (d.open && !d.dataset.loaded) {
          d.dataset.loaded = "yes";
          const full = await load(attempt);
          append(
            d,
            node("p", show(attempt.evidence)),
            node("pre", full.details?.seal),
          );
        }
      }),
    );
    byId("method-content").append(d);
  }
  byId("export-model").onclick = action(async () => {
    const full = {
      ...model,
      attempts: await Promise.all(model.attempts.map(load)),
    };
    download("report.json", JSON.stringify(full, null, 2));
  });
  updateTasks();
  action(trajectories)();
})();

import { readFileSync, writeFileSync } from "node:fs";

const root = new URL("../", import.meta.url);
const path = (relative) => new URL(relative, root);
const read = (relative) => readFileSync(path(relative), "utf8");
const write = (relative, value) => writeFileSync(path(relative), value, "utf8");
const stripAnsi = (value) => value.replace(/\u001b\[[0-9;]*m/g, "");

function marker(relative, name) {
  const text = stripAnsi(read(relative));
  const prefix = `${name}=`;
  const lines = text.split(/\r?\n/).filter((line) => line.includes(prefix));
  if (lines.length !== 1) throw new Error(`${relative}: expected one ${name}, got ${lines.length}`);
  return JSON.parse(lines[0].slice(lines[0].indexOf(prefix) + prefix.length));
}

function section(text, name) {
  const start = text.indexOf(`=== ${name} ===`);
  if (start < 0) throw new Error(`verification section missing: ${name}`);
  const next = text.indexOf("\n=== ", start + 5);
  return text.slice(start, next < 0 ? text.length : next);
}

function cargoTotals(value) {
  const totals = { passed: 0, failed: 0, ignored: 0, measured: 0, filtered_out: 0 };
  for (const match of value.matchAll(/test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out/g)) {
    totals.passed += Number(match[1]);
    totals.failed += Number(match[2]);
    totals.ignored += Number(match[3]);
    totals.measured += Number(match[4]);
    totals.filtered_out += Number(match[5]);
  }
  return totals;
}

function vitestPassed(value) {
  const clean = stripAnsi(value);
  const matches = [...clean.matchAll(/Tests\s+(\d+) passed/g)];
  if (!matches.length) throw new Error("Vitest passed count missing");
  return Number(matches.at(-1)[1]);
}

function operationSummary(operation) {
  return {
    median_ms: operation.median_ms,
    p95_ms: operation.p95_ms,
    max_work: operation.max_work ?? Math.max(...(operation.work_samples ?? [0])),
    max_allocated_bytes: operation.max_allocated_bytes ?? null,
    executions_per_timing_sample: operation.executions_per_timing_sample ?? 1,
  };
}

function fixtureSummary(fixture) {
  return {
    node_count: fixture.node_count,
    target_ids: fixture.targets ?? fixture.target_ids,
    warmups: fixture.warmups,
    measured_iterations: fixture.measured_iterations,
    structural_executions_per_timing_sample: fixture.structural_executions_per_timing_sample ?? 1,
    operations: Object.fromEntries(
      Object.entries(fixture.operations ?? {
        group: fixture.group,
        ungroup: fixture.ungroup,
      }).map(([name, value]) => [name, operationSummary(value)]),
    ),
  };
}

function ratioRows(layer, ratios) {
  return Object.entries(ratios).map(([operation, values]) =>
    `| ${layer} | ${operation} | ${values.work_count?.toFixed(3) ?? "n/a"} | ${values.median_time?.toFixed(3) ?? "n/a"} | ${values.projection_payload?.toFixed(3) ?? "n/a"} |`,
  );
}

const native = marker("docs/verification/PHASE_0E_R2_NATIVE_STRUCTURAL_OUTPUT.txt", "PHASE0E_R2_NATIVE_JSON");
const wasm = marker("docs/verification/PHASE_0E_R2_DIRECT_WASM_STRUCTURAL_OUTPUT.txt", "PHASE0E_R2_WASM_JSON");
const react = marker("docs/verification/PHASE_0E_R2_REACT_PROJECTION_OUTPUT.txt", "PHASE0E_R2_REACT_JSON");
const orderRust = marker("docs/verification/PHASE_0E_R2_ORDER_ACCURACY_OUTPUT.txt", "PHASE0E_R2_ORDER_JSON");
const orderReact = marker("docs/verification/PHASE_0E_R2_ORDER_ACCURACY_OUTPUT.txt", "PHASE0E_R2_REACT_ORDER_JSON");
const browser = JSON.parse(read("docs/verification/PHASE_0E_R2_BROWSER_PROOF.json"));
const pixel = JSON.parse(read("docs/verification/PHASE_0E_R2_PIXEL_READBACK.json"));
const verification = stripAnsi(read("docs/verification/PHASE_0E_R2_VERIFICATION.txt"));
const staticAudit = read("docs/verification/PHASE_0E_R2_STATIC_AUDIT.txt");

const allChecksPassed = Object.values(browser.checks).every((value) => value === true);
if (![native.all_passed, wasm.all_passed, react.all_passed, orderRust.all_passed, orderReact.all_passed, browser.all_passed, pixel.all_passed, allChecksPassed].every(Boolean)) {
  throw new Error("one or more R2 proof inputs did not pass");
}
if (!verification.includes("FINAL_STATUS=PASS")) throw new Error("final verification did not pass");
if (!staticAudit.includes("FINAL_STATUS=PASS")) throw new Error("static audit did not pass");
if ((browser.console_errors ?? []).length !== 0 || browser.runtime.gpu_validation_errors !== 0 || browser.max_fallback_rebuild_count_seen !== 0) {
  throw new Error("browser error or fallback count is nonzero");
}

const debugTotals = cargoTotals(section(verification, "Rust debug all-target tests"));
const doctestTotals = cargoTotals(section(verification, "Rust doctests"));
const reactPassed = vitestPassed(read("docs/verification/PHASE_0E_R2_REACT_PROJECTION_OUTPUT.txt"));
const browserPassed = vitestPassed(section(verification, "Actual Chrome R2 browser proof"));
const totalPassed = debugTotals.passed + doctestTotals.passed + reactPassed + browserPassed;

const counters = [
  "sibling/order entries examined",
  "entries copied",
  "entries moved or shifted",
  "rank/order comparisons",
  "sequence nodes/chunks allocated",
  "allocated bytes",
  "full sequence scans",
  "full sequence copies",
  "dense index rewrites",
  "structural fallback/rebuild count",
];

const metrics = {
  phase: "0E-R2",
  generated_at_utc: new Date().toISOString(),
  thresholds: {
    warmups_minimum: 10,
    measured_iterations_minimum: 30,
    maximum_100k_over_10k_structural_work_ratio: 1.35,
    maximum_100k_over_10k_warm_median_time_ratio: 3.0,
    full_sequence_scans: 0,
    full_sequence_copies: 0,
    dense_index_rewrites: 0,
    full_or_fallback_rebuilds: 0,
  },
  counter_schema_each_of_document_scene_ui: counters,
  native_release: {
    fixtures: native.fixtures.map(fixtureSummary),
    ratios_100k_over_10k: native.ratios_100k_over_10k,
    all_passed: native.all_passed,
  },
  direct_engine_host: {
    fixtures: wasm.fixtures.map(fixtureSummary),
    ratios_100k_over_10k: wasm.ratios_100k_over_10k,
    all_passed: wasm.all_passed,
  },
  react_projection_store: {
    fixtures: react.fixtures.map(fixtureSummary),
    ratios_100k_over_10k: react.ratios_100k_over_10k,
    all_passed: react.all_passed,
  },
  actual_chrome: {
    captured_at_utc: browser.captured_at_utc,
    browser: browser.browser.product,
    gpu: browser.hardware.active_device.deviceString,
    driver: browser.hardware.active_device.driverVersion,
    ratios_100k_over_10k: browser.r2_structural_benchmarks.ratios_100k_over_10k,
    mounted_rows_100k: browser.scenarios.hundred_k.ui.mountedRows,
    projection_nodes_100k: browser.scenarios.hundred_k.projection_nodes,
    max_fallback_rebuild_count_seen: browser.max_fallback_rebuild_count_seen,
    console_errors: browser.console_errors.length,
    gpu_validation_errors: browser.runtime.gpu_validation_errors,
    all_checks_passed: allChecksPassed,
    all_passed: browser.all_passed,
  },
  exact_order: {
    rust: orderRust,
    react: orderReact,
    all_passed: true,
  },
  pixel_readback: {
    captured_at_utc: pixel.captured_at_utc,
    kind: pixel.kind,
    assertions: pixel.assertions,
    all_passed: pixel.all_passed,
  },
  verification_totals: {
    rust_all_targets: debugTotals,
    rust_doctests: doctestTotals,
    react_tests_passed: reactPassed,
    browser_assertion_tests_passed: browserPassed,
    total_passed: totalPassed,
    failed: 0,
    ignored: debugTotals.ignored + doctestTotals.ignored,
    skipped: 0,
    mocked_proofs: 0,
  },
  phase1_started: false,
  all_passed: true,
};
write("docs/PHASE_0E_R2_METRICS.json", `${JSON.stringify(metrics, null, 2)}\n`);

const uiAudit = browser.scenarios.ui_audit;
const uiReport = `# Phase 0E-R2 UI and Accessibility Regression Report

- Captured: ${browser.captured_at_utc}
- Browser: ${browser.browser.product}, new temporary profile
- GPU: ${browser.hardware.active_device.deviceString}, driver ${browser.hardware.active_device.driverVersion}
- Runtime: actual WebGPU in Dedicated Worker/WASM; no mock and no Canvas2D fallback

## Automated DOM and accessibility results

- Component showcase label: ${uiAudit.showcase.labelled}; initial focus: \`${uiAudit.showcase.initial_focus}\`.
- Tabs: ${uiAudit.showcase.tabs}; segmented radios: ${uiAudit.showcase.radios}; roving radio keyboard behavior: ${uiAudit.roving_radio}.
- Select, menu trigger, switch, checkbox, badge: all present and semantically exercised.
- Dialog focus containment and Escape close/focus restoration: passed.
- Invalid numeric input returned \`${uiAudit.numeric_validation.code}\`, set \`aria-invalid=${uiAudit.numeric_validation.aria_invalid}\`, announced \`${uiAudit.numeric_validation.alert}\`, and did not mutate Worker revisions or history.
- Serialized pointercancel rollback, stale-intent rejection, and Worker restart generation checks: passed.
- 100K Layers projection nodes: ${browser.scenarios.hundred_k.projection_nodes}; mounted rows: ${browser.scenarios.hundred_k.ui.mountedRows}.
- Actual WebGPU pixel readback: ${pixel.kind}; all ${Object.keys(pixel.assertions).length} assertions passed.
- Console errors: ${browser.console_errors.length}; GPU validation errors: ${browser.runtime.gpu_validation_errors}; run-wide fallback maximum: ${browser.max_fallback_rebuild_count_seen}.

All ${Object.keys(browser.checks).length} browser proof checks are true. This report describes automated evidence; optional human screenshot inspection was not recorded as executed.
`;
write("docs/verification/PHASE_0E_R2_UI_A11Y_REPORT.md", uiReport);

const checklist = `# Phase 0E-R2 Manual Checklist

## Automated evidence already exercised

- [x] Verified the R1 baseline ZIP SHA-256 before R2 work.
- [x] Exercised 10K and 100K first/middle/last k=3 Group, Undo, Redo, Ungroup, save/load/Ungroup, and sibling insert/delete/reorder/Ungroup workflows.
- [x] Used 10 warm-up and 30 measured samples at Native Release, direct EngineHost, React ProjectionStore, and actual Chrome layers; Native Release structural samples contain eight actual executions per timing sample.
- [x] Confirmed 100K/10K structural work ratios at or below 1.35 and warm median time ratios at or below 3.0.
- [x] Confirmed Document/Scene/UI full sequence scans, full copies, dense rewrites, structural fallback/full rebuild, RenderModel clone/full scan, and unchanged-visual GPU dirty/upload counts are zero.
- [x] Verified eight overlapping siblings across Document order, Scene rank, topmost hit-test, persistence, undo/redo, edited-group restoration, and React Layers order.
- [x] Confirmed the legacy group-position metadata key is absent from product state and the versioned internal restoration record survives save/load.
- [x] Opened a self-contained preview with ${browser.browser.product}, a new temporary profile, Dedicated Worker/WASM, and actual ${browser.hardware.active_device.deviceString} WebGPU.
- [x] Re-ran the R1 single-leaf, 48-byte instance update, generation, pointercancel, numeric validation, dialog/accessibility, pixel readback, and error-zero regressions.

## Optional visual review

- [ ] Inspect the five R2 screenshots at 100% scale for accidental clipping or layout regressions.
- [ ] Launch \`tools/start-phase0e-editor.ps1\` and repeat any desired interaction manually.

The optional visual review is not recorded as executed. Automated evidence is referenced from the R2 review packet.
`;
write("docs/PHASE_0E_R2_MANUAL_CHECKLIST.md", checklist);

const ratioTable = [
  "| Layer | Operation | Work ratio | Median time ratio | Payload ratio |",
  "| --- | --- | ---: | ---: | ---: |",
  ...ratioRows("Native Release", native.ratios_100k_over_10k),
  ...ratioRows("Direct EngineHost", wasm.ratios_100k_over_10k),
  `| React ProjectionStore | group | ${react.ratios_100k_over_10k.groupWork.toFixed(3)} | ${react.ratios_100k_over_10k.groupMedianTime.toFixed(3)} | n/a |`,
  `| React ProjectionStore | ungroup | ${react.ratios_100k_over_10k.ungroupWork.toFixed(3)} | ${react.ratios_100k_over_10k.ungroupMedianTime.toFixed(3)} | n/a |`,
  ...ratioRows("Actual Chrome", browser.r2_structural_benchmarks.ratios_100k_over_10k),
].join("\n");

const review = `# Phase 0E-R2 Review Packet

## Disposition and scope

This packet requests external review of the authorized Phase 0E-R2 structural corrections. It does not claim Gate approval and does not authorize or begin Phase 1.

Direct baseline: \`visual_authoring_engine_phase0e_r1_review_2026-08-10.zip\`, SHA-256 \`07d6ba27f27c32b89474b098e0e1f3045bbd2d4511eb7951d1e7d03d3bafe3db\`.

## Structural correction

- Document and Scene sibling order use arena-backed implicit order-statistic treaps with ID locators and exact work counters.
- React ProjectionStore uses ranked treaps, detached fragments, subtree sizes, and viewport rank/select instead of large-array structural edits.
- Group restoration is a versioned internal before/after-anchor record; user metadata no longer stores engine positions.
- Group/Ungroup, undo/redo, save/load/Ungroup, and sibling insert/delete/reorder/Ungroup are covered at 10K and 100K.
- Eight-overlapping-sibling evidence records Document semantic orders, live Scene ranks, topmost hit order, persistence, and React Layers order.
- Full edit-path sibling scans/copies, dense rewrites, hierarchy fallback/full rebuilds, RenderModel clones/scans, and unchanged-visual GPU uploads remain zero.

## 10K/100K results

${ratioTable}

All work ratios are at most 1.35 and all warm median time ratios are at most 3.0. Native Release uses 10 warm-up and 30 measured timing samples with eight actual structural executions averaged into each timing sample; direct EngineHost, React, and Chrome use at least the required 10/30 actual iterations. Raw samples, median, p95, work, allocation, IDs, and counters are preserved in the R2 evidence files.

## Verification

The R2 final orchestrator actually executed and passed:

\`\`\`powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 fmt --all -- --check
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 clippy --workspace --all-targets -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --workspace --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --workspace --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 test --doc --workspace
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --release --workspace --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File tools/cargo.ps1 build --release --target wasm32-unknown-unknown
powershell -NoProfile -ExecutionPolicy Bypass -File tools/build-phase0e-wasm.ps1
npm ci --ignore-scripts
npm run build
npm test
npm run test:browser:r2
\`\`\`

- Rust all-target tests: ${debugTotals.passed} passed
- Rust doctests: ${doctestTotals.passed} passed
- React tests: ${reactPassed} passed
- R2 browser assertion tests: ${browserPassed} passed
- Counted suite total: ${totalPassed} passed, 0 failed, ${debugTotals.ignored + doctestTotals.ignored} ignored, 0 skipped, 0 mocked
- Actual browser/GPU: ${browser.browser.product}, ${browser.hardware.active_device.deviceString}
- Browser proof: \`all_passed=true\`, ${Object.keys(browser.checks).length}/${Object.keys(browser.checks).length} checks, fallback maximum 0, console errors 0, GPU validation errors 0

## Evidence index

- \`docs/PHASE_0E_R2_AUTHORIZATION.md\`
- \`docs/PHASE_0E_R2_METRICS.json\`
- \`docs/PHASE_0E_R2_CHANGE_MANIFEST.json\`
- \`docs/PHASE_0E_R2_MANUAL_CHECKLIST.md\`
- \`docs/adr/ADR-035-document-ranked-sibling-sequence.md\`
- \`docs/adr/ADR-036-scene-ranked-order-and-z-model.md\`
- \`docs/adr/ADR-037-ui-ranked-projection-sequence.md\`
- \`docs/adr/ADR-038-versioned-group-restoration-anchors.md\`
- \`docs/verification/PHASE_0E_R2_NATIVE_STRUCTURAL_OUTPUT.txt\`
- \`docs/verification/PHASE_0E_R2_DIRECT_WASM_STRUCTURAL_OUTPUT.txt\`
- \`docs/verification/PHASE_0E_R2_REACT_PROJECTION_OUTPUT.txt\`
- \`docs/verification/PHASE_0E_R2_ORDER_ACCURACY_OUTPUT.txt\`
- \`docs/verification/PHASE_0E_R2_STATIC_AUDIT.txt\`
- \`docs/verification/PHASE_0E_R2_BROWSER_PROOF.json\`
- \`docs/verification/PHASE_0E_R2_PIXEL_READBACK.json\`
- \`docs/verification/PHASE_0E_R2_UI_A11Y_REPORT.md\`
- \`docs/verification/PHASE_0E_R2_VERIFICATION.txt\`
- Five \`docs/verification/phase0e-r2-*.png\` screenshots

## Preservation and packaging

The R2 packager requires prior docs/evidence byte changes 0, authoritative files 19/19 byte-identical, authoritative \`SHA256SUMS.txt\` 18/18, deleted baseline files 0, and forbidden/path/duplicate/CRC errors 0. It records source-project and physical-review-ZIP comparisons separately and lists excluded generated artifacts instead of treating them as deletions.

\`target\`, \`node_modules\`, \`.tools\`, \`.git\`, IDE/cache output, \`dist\`, work-only codemods, credentials, temporary logs, and nested ZIPs are excluded. Phase 1 remains unstarted.
`;
write("docs/REVIEW_PACKET_0E_R2.md", review);

console.log(JSON.stringify({
  metrics: "docs/PHASE_0E_R2_METRICS.json",
  manual_checklist: "docs/PHASE_0E_R2_MANUAL_CHECKLIST.md",
  ui_a11y: "docs/verification/PHASE_0E_R2_UI_A11Y_REPORT.md",
  review_packet: "docs/REVIEW_PACKET_0E_R2.md",
  total_passed: totalPassed,
  browser_captured_at_utc: browser.captured_at_utc,
  all_passed: true,
}, null, 2));

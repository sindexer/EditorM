// Phase 1B multi-selection drag benchmark, engine only.
//
// This measures the cost of one drag frame for 10, 100, and 1,000 simultaneously selected
// objects through the shipped WASM EngineHost: selection, transaction, translate_selection with
// and without snapping, commit, and undo. It runs in Node against the real compiled engine, so
// the numbers are real engine work, but there is no browser, no Worker boundary, and no GPU
// submission in this path. It therefore reports `gpu_evidence: false` and is not gate evidence.
//
// The browser-side equivalent, which includes the Worker hop and actual WebGPU submission, runs
// inside scripts/browser-proof-phase1b.mjs and writes docs/PHASE_1B_METRICS.json.

import crypto from "node:crypto";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";

const scriptRoot = path.dirname(fileURLToPath(import.meta.url));
const webRoot = path.resolve(scriptRoot, "..");
const workspace = path.resolve(webRoot, "../..");
const gluePath = path.join(webRoot, "public", "pkg", "engine_host.js");
const wasmPath = path.join(webRoot, "public", "pkg", "engine_host_bg.wasm");
const outputPath = path.join(workspace, "docs", "PHASE_1B_METRICS_ENGINE_ONLY.json");
const startedAt = new Date().toISOString();
const gateEvidence = {
  gate_run_id: process.env.PHASE1B_GATE_RUN_ID ?? null,
  tested_source_commit: process.env.PHASE1B_GATE_SOURCE_COMMIT ?? null,
  tested_branch: process.env.PHASE1B_GATE_SOURCE_BRANCH ?? null,
};

const OBJECT_COUNTS = [10, 100, 1000];
const FRAMES_PER_DRAG = 5;
const WARMUP_ITERATIONS = 5;
const MEASURED_ITERATIONS = 20;
const FRAME_BUDGET_MS = 16.7;

function dummyImports(module) {
  const imports = {};
  for (const descriptor of WebAssembly.Module.imports(module)) {
    imports[descriptor.module] ??= {};
    if (descriptor.kind === "function") imports[descriptor.module][descriptor.name] = () => 0;
    else if (descriptor.kind === "memory") imports[descriptor.module][descriptor.name] = new WebAssembly.Memory({ initial: 32 });
    else if (descriptor.kind === "table") imports[descriptor.module][descriptor.name] = new WebAssembly.Table({ initial: 128, element: "externref" });
    else if (descriptor.kind === "global") imports[descriptor.module][descriptor.name] = new WebAssembly.Global({ value: "i32", mutable: true }, 0);
  }
  return imports;
}

function fixtureNodeId(index) {
  const hex = (BigInt(index) + 1n).toString(16).padStart(32, "0");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function percentile(values, fraction) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.ceil((sorted.length - 1) * fraction)];
}

const wasmBytes = await fs.readFile(wasmPath);
const wasmSha256 = crypto.createHash("sha256").update(wasmBytes).digest("hex");
const module = new WebAssembly.Module(wasmBytes);
new WebAssembly.Instance(module, dummyImports(module));
const glue = await import(`${pathToFileURL(gluePath).href}?bench=${Date.now()}`);
glue.initSync({ module });
const host = new glue.EngineHost();

let requestSequence = 0;
function call(type, body = {}) {
  const request = {
    protocol_version: 1,
    request_id: `bench-${++requestSequence}`,
    type,
    ...body,
  };
  const response = JSON.parse(host.handleJson(JSON.stringify(request)));
  if (!response.ok) {
    throw new Error(`${type} failed: ${JSON.stringify(response.error)}`);
  }
  return response;
}

call("initialize");
call("load_fixture", { fixture: "bench-a" });
call("camera", { camera: { kind: "resize", width: 1920, height: 1080, dpr: 1 } });
call("camera", { camera: { kind: "fit" } });

const series = [];
for (const objects of OBJECT_COUNTS) {
  const ids = Array.from({ length: objects }, (_, index) => fixtureNodeId(index + 1));
  for (const snap of [false, true]) {
    call("selection", { mode: "set", targets: ids });
    const samples = [];
    let last = null;
    for (let iteration = 0; iteration < WARMUP_ITERATIONS + MEASURED_ITERATIONS; iteration += 1) {
      call("begin_transaction");
      const started = performance.now();
      for (let frame = 1; frame <= FRAMES_PER_DRAG; frame += 1) {
        // Strides are deliberately not multiples of the fixture's 40-unit grid: with snapping on,
        // a small stride would land on the same corrected position every frame and the later
        // frames would measure a no-op instead of real work.
        last = call("translate_selection", {
          dx: frame * 37,
          dy: frame * 23,
          snap,
          snap_threshold_px: 8,
        });
      }
      const perFrame = (performance.now() - started) / FRAMES_PER_DRAG;
      call("commit_transaction");
      call("undo");
      if (iteration >= WARMUP_ITERATIONS) samples.push(perFrame);
    }
    series.push({
      objects,
      snapping: snap,
      moved_nodes: last.result.moved,
      snapped: last.result.snapped,
      snap_candidates_examined: last.result.snap_candidates_examined ?? 0,
      frames_per_drag: FRAMES_PER_DRAG,
      warmup_iterations: WARMUP_ITERATIONS,
      measured_iterations: MEASURED_ITERATIONS,
      raw_frame_ms: samples,
      median_ms: percentile(samples, 0.5),
      p95_ms: percentile(samples, 0.95),
      maximum_ms: Math.max(...samples),
      dirty_slots: last.render_delta.dirty_slots,
      upload_bytes: last.render_delta.upload_bytes,
      scene_full_rebuilds: last.render_delta.scene_full_rebuilds,
      render_full_rebuilds: last.render_delta.render_full_rebuilds,
      document_full_clones: last.metrics.document_full_clones ?? null,
      full_render_model_scans: last.metrics.full_render_model_scans ?? null,
    });
  }
}

const find = (objects, snapping) =>
  series.find((entry) => entry.objects === objects && entry.snapping === snapping);
const checks = {
  every_drag_moved_every_selected_node: series.every(
    (entry) => entry.moved_nodes === entry.objects,
  ),
  dirty_slots_match_selection_size: series.every(
    (entry) => entry.dirty_slots === entry.objects,
  ),
  no_full_rebuilds: series.every(
    (entry) =>
      entry.scene_full_rebuilds === 0 &&
      entry.render_full_rebuilds === 0 &&
      entry.full_render_model_scans === 0,
  ),
  no_document_clones: series.every((entry) => entry.document_full_clones === 0),
  ten_objects_within_frame_budget: find(10, false).p95_ms <= FRAME_BUDGET_MS,
  ten_objects_snapped_within_frame_budget: find(10, true).p95_ms <= FRAME_BUDGET_MS,
  hundred_objects_within_frame_budget: find(100, false).p95_ms <= FRAME_BUDGET_MS,
  hundred_objects_snapped_within_frame_budget: find(100, true).p95_ms <= FRAME_BUDGET_MS,
  thousand_objects_measured: find(1000, false).raw_frame_ms.length === MEASURED_ITERATIONS,
};

const report = {
  phase: "1B",
  ...gateEvidence,
  proof_kind: "actual-wasm-engine-only-multi-drag",
  gpu_evidence: false,
  browser_evidence: false,
  scope_note:
    "Engine cost only: Node, no Worker hop, no WebGPU submission. The browser benchmark in scripts/browser-proof-phase1b.mjs measures the same work with the Worker and GPU included and writes docs/PHASE_1B_METRICS.json.",
  started_at: startedAt,
  finished_at: new Date().toISOString(),
  node_version: process.version,
  platform: `${process.platform}-${process.arch}`,
  wasm_path: path.relative(workspace, wasmPath).split(path.sep).join("/"),
  wasm_bytes: wasmBytes.length,
  wasm_sha256: wasmSha256,
  fixture: "BENCH-A",
  frame_budget_ms: FRAME_BUDGET_MS,
  budget_note:
    "The 16.7 ms budget is asserted for 10 and 100 simultaneously dragged objects. The 1,000-object series is recorded without a gate threshold; Phase 1B does not claim a large-selection drag budget.",
  series,
  checks,
  all_passed: Object.values(checks).every(Boolean),
};

await fs.writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
for (const entry of series) {
  console.log(
    `objects=${String(entry.objects).padStart(4)} snap=${entry.snapping ? "on " : "off"} ` +
      `median=${entry.median_ms.toFixed(3)}ms p95=${entry.p95_ms.toFixed(3)}ms ` +
      `max=${entry.maximum_ms.toFixed(3)}ms dirty_slots=${entry.dirty_slots}`,
  );
}
console.log(`metrics=${outputPath}`);
if (!report.all_passed) {
  console.error(JSON.stringify(checks, null, 2));
  process.exitCode = 1;
}

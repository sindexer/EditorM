// Phase 1B direct-WASM proof.
//
// Loads the pinned engine_host WASM package in Node and exercises the Phase 1B request surface
// against the actual compiled engine: multiple selection, rubber-band selection, snapped batch
// translation, alignment, distribution, and undo. This proves the shipped WASM module, not a
// native rebuild of the same crate. It does not touch WebGPU, so it never claims GPU evidence.

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
const outputPath = path.join(workspace, "docs", "verification", "PHASE_1B_DIRECT_WASM_PROOF.json");
const startedAt = new Date().toISOString();
const gateEvidence = {
  gate_run_id: process.env.PHASE1B_GATE_RUN_ID ?? null,
  tested_source_commit: process.env.PHASE1B_GATE_SOURCE_COMMIT ?? null,
  tested_branch: process.env.PHASE1B_GATE_SOURCE_BRANCH ?? null,
};

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

const wasmBytes = await fs.readFile(wasmPath);
const wasmSha256 = crypto.createHash("sha256").update(wasmBytes).digest("hex");
const module = new WebAssembly.Module(wasmBytes);
new WebAssembly.Instance(module, dummyImports(module));
const glue = await import(pathToFileURL(gluePath).href + `?phase1b=${Date.now()}`);
glue.initSync({ module });
const host = new glue.EngineHost();

let requestSequence = 0;
function call(type, body = {}) {
  const request = { protocol_version: 1, request_id: `phase1b-${++requestSequence}`, type, ...body };
  return JSON.parse(host.handleJson(JSON.stringify(request)));
}

const checks = [];
function check(name, passed, detail) {
  checks.push({ name, passed: Boolean(passed), detail: detail ?? null });
  if (!passed) process.exitCode = 1;
}

function requireOk(name, response) {
  check(name, response.ok === true, response.ok ? null : response.error);
  return response;
}

function nodeBounds(id) {
  const snapshot = call("get_ui_snapshot");
  const node = snapshot.projection.upserts.find((entry) => entry.id === id);
  return node?.world_bounds ?? null;
}

function rectangleId(index) {
  return `00000000-0000-4000-8000-${String(index).padStart(12, "0")}`;
}

requireOk("initialize", call("initialize"));
requireOk("load_editor_fixture", call("load_fixture", { fixture: "editor" }));
requireOk(
  "resize_camera",
  call("camera", { camera: { kind: "resize", width: 1920, height: 1080, dpr: 1 } }),
);

const snapshot = call("get_ui_snapshot");
const frame = snapshot.projection.upserts.find((node) => node.kind === "frame");
check("editor_fixture_has_frame", Boolean(frame), frame ? null : "no frame node");

// Three rectangles at different offsets inside the Frame.
const layout = [
  { x: 0, y: 400 },
  { x: 300, y: 500 },
  { x: 700, y: 600 },
];
const ids = layout.map((entry, index) => {
  const id = rectangleId(index + 100);
  requireOk(
    `create_rectangle_${index}`,
    call("command", {
      command: {
        kind: "create_shape",
        node_id: id,
        parent_id: frame.id,
        index,
        shape: "rectangle",
        name: `Rect ${index}`,
        x: entry.x,
        y: entry.y,
        width: 100,
        height: 60,
      },
    }),
  );
  return id;
});

// Multiple selection.
const selected = requireOk("selection_set", call("selection", { mode: "set", targets: ids }));
check("selection_set_count", selected.selection.ordered.length === 3, selected.selection.ordered);

const rejected = call("selection", {
  mode: "set",
  targets: [...ids, "00000000-0000-4000-8000-999999999999"],
});
check("selection_set_rejects_unknown_id", rejected.ok === false, rejected.error);
check(
  "selection_survives_rejected_request",
  rejected.selection.ordered.length === 3,
  rejected.selection.ordered,
);

// Rubber-band selection: viewport x 0..500 covers world x -960..-460.
const marquee = requireOk(
  "marquee_select",
  call("marquee_select", { x0: 0, y0: 0, x1: 500, y1: 1080, root_id: frame.id }),
);
const marqueeSelected = [...marquee.result.selected].sort();
check(
  "marquee_selects_intersecting_top_level_nodes",
  JSON.stringify(marqueeSelected) === JSON.stringify([ids[0], ids[1]].sort()),
  marqueeSelected,
);

// Snapped batch translation measured from the transaction base.
requireOk("selection_replace_second", call("selection", { target: ids[1], mode: "replace" }));
const begin = requireOk("begin_transaction", call("begin_transaction"));
check("drag_base_captured", begin.result.drag_targets === 1, begin.result);
const translated = requireOk(
  "translate_selection_snapped",
  call("translate_selection", { dx: -297, dy: 0, snap: true, snap_threshold_px: 8 }),
);
check("translation_snapped", translated.result.snapped === true, translated.result);
check(
  "snap_corrected_applied_delta",
  Math.abs(translated.result.applied_delta[0] + 300) < 1e-6,
  translated.result.applied_delta,
);
check(
  "snap_guide_reported",
  Array.isArray(translated.result.guides) && translated.result.guides.length > 0,
  translated.result.guides,
);
requireOk("commit_transaction", call("commit_transaction"));
check(
  "snapped_node_left_edge_matches_first",
  Math.abs(nodeBounds(ids[1]).min[0] - nodeBounds(ids[0]).min[0]) < 1e-6,
  [nodeBounds(ids[0]).min[0], nodeBounds(ids[1]).min[0]],
);
requireOk("undo_snapped_move", call("undo"));

// Repeated drag frames must not accumulate drift.
const driftStart = nodeBounds(ids[1]).min[0];
requireOk("begin_drift_transaction", call("begin_transaction"));
for (const delta of [10, 25, 60]) {
  requireOk(`translate_frame_${delta}`, call("translate_selection", { dx: delta, dy: 0, snap: false }));
}
requireOk("commit_drift_transaction", call("commit_transaction"));
check(
  "repeated_frames_measure_from_base",
  Math.abs(nodeBounds(ids[1]).min[0] - (driftStart + 60)) < 1e-6,
  [driftStart, nodeBounds(ids[1]).min[0]],
);
requireOk("undo_drift", call("undo"));

// Alignment as one request and one undo step.
requireOk("selection_set_all", call("selection", { mode: "set", targets: ids }));
const beforeAlign = call("heartbeat").history.undo_depth;
const aligned = requireOk("arrange_align_left", call("arrange", { operation: "align_left" }));
check("align_moved_two_nodes", aligned.result.moved === 2, aligned.result);
check(
  "align_is_one_history_entry",
  aligned.history.undo_depth === beforeAlign + 1,
  { before: beforeAlign, after: aligned.history.undo_depth },
);
const alignedEdges = ids.map((id) => nodeBounds(id).min[0]);
check(
  "aligned_left_edges_match",
  alignedEdges.every((edge) => Math.abs(edge - alignedEdges[0]) < 1e-6),
  alignedEdges,
);
requireOk("undo_alignment", call("undo"));
const restoredEdges = ids.map((id) => nodeBounds(id).min[0]);
check(
  "undo_restores_every_target",
  Math.abs(restoredEdges[1] - restoredEdges[0]) > 1,
  restoredEdges,
);

// Distribution equalizes gaps and keeps the extremes.
const distributed = requireOk(
  "arrange_distribute_horizontal",
  call("arrange", { operation: "distribute_horizontal" }),
);
check("distribute_reported_change", distributed.result.changed === true, distributed.result);
const bounds = ids.map((id) => nodeBounds(id));
const sorted = [...bounds].sort((left, right) => left.min[0] - right.min[0]);
const firstGap = sorted[1].min[0] - sorted[0].max[0];
const secondGap = sorted[2].min[0] - sorted[1].max[0];
check("distribute_equalized_gaps", Math.abs(firstGap - secondGap) < 1e-6, [firstGap, secondGap]);
requireOk("undo_distribution", call("undo"));

// Typed failures leave no partial state.
const noTransaction = call("translate_selection", { dx: 10, dy: 0, snap: false });
check(
  "translation_without_transaction_is_typed",
  noTransaction.ok === false && noTransaction.error.code === "no_transaction",
  noTransaction.error,
);
const badOperation = call("arrange", { operation: "align_diagonal" });
check(
  "unknown_arrange_is_typed",
  badOperation.ok === false && badOperation.error.code === "invalid_arrange",
  badOperation.error,
);
requireOk("selection_two_only", call("selection", { mode: "set", targets: ids.slice(0, 2) }));
const tooFew = call("arrange", { operation: "distribute_horizontal" });
check("distribute_requires_three", tooFew.ok === false, tooFew.error);
check("no_transaction_left_open", tooFew.history.transaction_active === false, tooFew.history);

const finalState = call("heartbeat");
const report = {
  phase: "1B",
  ...gateEvidence,
  proof_kind: "actual-wasm-direct-node",
  gpu_evidence: false,
  started_at: startedAt,
  finished_at: new Date().toISOString(),
  node_version: process.version,
  platform: `${process.platform}-${process.arch}`,
  wasm_path: path.relative(workspace, wasmPath).split(path.sep).join("/"),
  wasm_bytes: wasmBytes.length,
  wasm_sha256: wasmSha256,
  protocol_version: glue.EngineHost.protocolVersion(),
  render_binary_schema_version: glue.EngineHost.renderBinarySchemaVersion(),
  engine_sequence: finalState.engine_sequence,
  checks,
  checks_passed: checks.filter((entry) => entry.passed).length,
  checks_total: checks.length,
  all_passed: checks.every((entry) => entry.passed),
};

await fs.writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
console.log(`PHASE_1B_DIRECT_WASM_JSON=${JSON.stringify(report)}`);
if (!report.all_passed) {
  console.error("Phase 1B direct WASM proof failed");
  process.exitCode = 1;
}

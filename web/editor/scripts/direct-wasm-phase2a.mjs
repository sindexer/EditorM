// Phase 2A direct-WASM proof.
//
// Loads the engine_host WASM package this repository currently ships and exercises the Phase 2A
// path surface against the actual compiled engine: the PATH-A fixture, path instance and vertex
// buffers, ordered draw batches, creating and editing a path, undo and redo, the camera-only
// upload contract, and the negative cases. It never touches WebGPU, so it never claims GPU
// evidence: the browser harness owns that.
//
// This is the Phase 2A artifact. The Phase 1B proof is a historical record of the Gate 1B run
// and is never regenerated here.

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
const outputArgument = process.argv.find((argument) => argument.startsWith("--output="));
const outputPath = outputArgument
  ? path.resolve(workspace, outputArgument.slice("--output=".length))
  : null;
const startedAt = new Date().toISOString();

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
const glue = await import(pathToFileURL(gluePath).href + `?phase2a=${Date.now()}`);
glue.initSync({ module });
const host = new glue.EngineHost();

let requestSequence = 0;
function call(type, body = {}) {
  const request = { protocol_version: 1, request_id: `phase2a-${++requestSequence}`, type, ...body };
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

function projectedNodes(response) {
  const table = {};
  for (const node of response.projection.upserts) table[node.id] = node;
  return table;
}

const PATH_NODE = "6f2c3b4a-1d5e-4a7b-8c9d-0e1f2a3b4c5d";
const ANCHORS = [
  { id: "6f2c3b4a-1d5e-4a7b-8c9d-0e1f2a3b4c01", position: [0, 0] },
  { id: "6f2c3b4a-1d5e-4a7b-8c9d-0e1f2a3b4c02", position: [160, 0] },
  { id: "6f2c3b4a-1d5e-4a7b-8c9d-0e1f2a3b4c03", position: [80, 120] },
];

requireOk("initialize", call("initialize"));
requireOk(
  "resize_camera",
  call("camera", { camera: { kind: "resize", width: 1920, height: 1080, dpr: 1 } }),
);

// ------------------------------------------------------------------ PATH-A fixture
const fixture = requireOk("load_path_fixture", call("load_fixture", { fixture: "path-a" }));
check("fixture_is_path_a", fixture.fixture === "PATH-A", fixture.fixture);
check(
  "schema_version_3",
  fixture.render_binary_schema_version === 3 &&
    fixture.resources.path_instance_stride_bytes === 80 &&
    fixture.resources.path_vertex_stride_bytes === 32,
  {
    schema: fixture.render_binary_schema_version,
    path_instance_stride_bytes: fixture.resources.path_instance_stride_bytes,
    path_vertex_stride_bytes: fixture.resources.path_vertex_stride_bytes,
  },
);
check("fixture_has_four_paths", fixture.resources.path_instance_count === 4, fixture.resources.path_instance_count);
const fixtureVertices = fixture.resources.path_vertex_count;
check(
  "fixture_tessellates_to_triangles",
  fixtureVertices > 0 && fixtureVertices % 3 === 0,
  fixtureVertices,
);
check(
  "fixture_sends_path_buffers",
  fixture.binary.path_instances === true && fixture.binary.path_vertices === true,
  fixture.binary,
);
const fixtureBatches = fixture.resources.draw_batches ?? [];
check(
  "draw_batches_cover_every_path_vertex",
  fixtureBatches.length > 0 &&
    fixtureBatches
      .filter((batch) => batch.kind === "paths")
      .reduce((total, batch) => total + batch.vertex_count, 0) === fixtureVertices,
  fixtureBatches,
);
const fixtureNodes = projectedNodes(call("get_ui_snapshot"));
const fixturePaths = Object.values(fixtureNodes).filter((node) => node.kind === "path");
check("fixture_projects_four_path_nodes", fixturePaths.length === 4, fixturePaths.map((node) => node.name));
check(
  "every_fixture_path_has_finite_bounds",
  fixturePaths.every((node) => Array.isArray(node.world_bounds?.min) && Array.isArray(node.world_bounds?.max)),
  fixturePaths.map((node) => node.world_bounds),
);

// ------------------------------------------------------------------ camera-only upload contract
const camera = requireOk(
  "camera_pan",
  call("camera", { camera: { kind: "pan", dx: 24, dy: 18 } }),
);
check("camera_only_sends_no_triangles", camera.binary.path_vertices === false, camera.binary);
check("camera_only_uploads_zero_vertices", camera.metrics.path_vertices_uploaded === 0, camera.metrics.path_vertices_uploaded);
check("camera_only_retessellates_nothing", camera.metrics.path_tessellations === 0, camera.metrics.path_tessellations);

// ------------------------------------------------------------------ create, edit, undo, redo
const rootId = Object.values(fixtureNodes).find((node) => node.kind === "document")?.id;
const created = requireOk(
  "create_path",
  call("command", {
    command: {
      kind: "create_path",
      node_id: PATH_NODE,
      parent_id: rootId,
      index: 0,
      name: "Proof Path",
      x: -300,
      y: 220,
      closed: true,
      anchors: ANCHORS,
    },
  }),
);
check("create_path_tessellates_once", created.metrics.path_tessellations === 1, created.metrics.path_tessellations);
const createdVertices = created.resources.path_vertex_count;
check("create_path_adds_triangles", createdVertices > fixtureVertices, {
  before: fixtureVertices,
  after: createdVertices,
});
const createdBounds = projectedNodes(call("get_ui_snapshot"))[PATH_NODE]?.world_bounds ?? null;
check("created_path_is_projected", createdBounds !== null, createdBounds);

const edited = requireOk(
  "set_path_geometry",
  call("command", {
    command: {
      kind: "set_path_geometry",
      node_id: PATH_NODE,
      closed: true,
      anchors: ANCHORS.map((anchor, index) =>
        index === 2 ? { ...anchor, position: [80, 260] } : anchor,
      ),
    },
  }),
);
check("edit_retessellates_exactly_one_path", edited.metrics.path_tessellations === 1, edited.metrics.path_tessellations);
check("edit_causes_no_full_render_rebuild", edited.render_delta.render_full_rebuilds === 0, edited.render_delta);
check("edit_causes_no_fallback_rebuild", edited.metrics.fallback_rebuild_count === 0, edited.metrics.fallback_rebuild_count);
const editedBounds = projectedNodes(call("get_ui_snapshot"))[PATH_NODE]?.world_bounds ?? null;
check(
  "edit_grows_the_path_bounds",
  editedBounds !== null && editedBounds.max[1] > createdBounds.max[1],
  { created: createdBounds, edited: editedBounds },
);

const undone = requireOk("undo_geometry", call("undo"));
const undoneBounds = projectedNodes(call("get_ui_snapshot"))[PATH_NODE]?.world_bounds ?? null;
check(
  "undo_restores_path_geometry",
  undoneBounds !== null && Math.abs(undoneBounds.max[1] - createdBounds.max[1]) < 1e-9,
  { undone: undoneBounds, created: createdBounds },
);
check(
  "undo_restores_triangle_count",
  undone.resources.path_vertex_count === createdVertices,
  { after_undo: undone.resources.path_vertex_count, created: createdVertices },
);

const redone = requireOk("redo_geometry", call("redo"));
const redoneBounds = projectedNodes(call("get_ui_snapshot"))[PATH_NODE]?.world_bounds ?? null;
check(
  "redo_reapplies_path_geometry",
  redoneBounds !== null && Math.abs(redoneBounds.max[1] - editedBounds.max[1]) < 1e-9,
  { redone: redoneBounds, edited: editedBounds },
);
check("redo_keeps_the_path_instance", redone.resources.path_instance_count === 5, redone.resources.path_instance_count);

call("undo");
const removed = requireOk("undo_creation", call("undo"));
check(
  "undo_removes_the_created_path",
  projectedNodes(call("get_ui_snapshot"))[PATH_NODE] === undefined &&
    removed.resources.path_instance_count === 4,
  removed.resources.path_instance_count,
);

// ------------------------------------------------------------------ negative cases
const beforeFailures = call("heartbeat");
const invalidCases = [
  ["closed_path_needs_three_anchors", ANCHORS.slice(0, 2)],
  [
    "duplicate_anchor_identity_rejected",
    [ANCHORS[0], { ...ANCHORS[1], id: ANCHORS[0].id }, ANCHORS[2]],
  ],
  ["anchor_id_must_be_a_uuid", [{ ...ANCHORS[0], id: "not-a-uuid" }, ANCHORS[1], ANCHORS[2]]],
];
for (const [name, anchors] of invalidCases) {
  const failure = call("command", {
    command: {
      kind: "create_path",
      node_id: "7a1b2c3d-4e5f-4a6b-8c9d-0e1f2a3b4c5d",
      parent_id: rootId,
      index: 0,
      name: "Invalid",
      x: 0,
      y: 0,
      closed: true,
      anchors,
    },
  });
  check(name, failure.ok === false, failure.error);
  check(
    `${name}_leaves_document_unchanged`,
    failure.revisions.document === beforeFailures.revisions.document &&
      failure.history.undo_depth === beforeFailures.history.undo_depth,
    { before: beforeFailures.revisions, after: failure.revisions },
  );
  check(`${name}_sends_no_path_triangles`, failure.binary.path_vertices === false, failure.binary);
}

const wrongKind = call("command", {
  command: {
    kind: "set_path_geometry",
    node_id: fixturePaths[0].id,
    closed: false,
    anchors: [ANCHORS[0]],
  },
});
check("open_path_needs_two_anchors", wrongKind.ok === false, wrongKind.error);

const finalState = call("heartbeat");
const report = {
  phase: "2A",
  proof_kind: "actual-wasm-direct-node",
  gpu_evidence: false,
  browser_evidence: false,
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
  fixture: {
    name: "PATH-A",
    path_instance_count: fixture.resources.path_instance_count,
    path_vertex_count: fixtureVertices,
    draw_batches: fixtureBatches,
  },
  checks,
  checks_passed: checks.filter((entry) => entry.passed).length,
  checks_total: checks.length,
  all_passed: checks.every((entry) => entry.passed),
};

if (outputPath) {
  await fs.writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
  console.log(`PHASE_2A_DIRECT_WASM_WRITTEN=${path.relative(workspace, outputPath).split(path.sep).join("/")}`);
} else {
  console.log("PHASE_2A_DIRECT_WASM_MODE=verification-only (no artifact written)");
}
console.log(`PHASE_2A_DIRECT_WASM_JSON=${JSON.stringify(report)}`);
if (!report.all_passed) {
  console.error("Phase 2A direct WASM proof failed");
  process.exitCode = 1;
}

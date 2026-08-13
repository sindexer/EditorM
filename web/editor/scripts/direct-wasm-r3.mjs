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
const outputPath = path.join(workspace, "docs", "verification", "PHASE_0E_R3_ACTUAL_DIRECT_WASM_OUTPUT.txt");
const startedAt = new Date().toISOString();
const quick = process.env.PHASE0E_R3_QUICK === "1";
const focus = process.env.PHASE0E_R3_FOCUS === "1";

function fixtureNodeId(index) {
  const hex = (BigInt(index) + 1n).toString(16).padStart(32, "0");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function percentile(values, fraction) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.ceil((sorted.length - 1) * fraction)];
}

function summarize(samples) {
  return {
    raw_samples_ms: samples.times,
    raw_work_samples: samples.work,
    median_ms: percentile(samples.times, 0.5),
    p95_ms: percentile(samples.times, 0.95),
    maximum_work: Math.max(...samples.work),
    maximum_sequence_depth: Math.max(...samples.depth),
    tree_rebalances: Math.max(...samples.rebalances),
  };
}

function engineWork(response) {
  let total = 0;
  let maximumDepth = 0;
  let rebalances = 0;
  for (const prefix of ["document", "scene"]) {
    const metric = (suffix) => Number(response.metrics[`${prefix}_sequence_${suffix}`] ?? 0);
    total +=
      Math.max(metric("entries_examined"), metric("rank_order_comparisons")) +
      metric("entries_copied") +
      metric("entries_moved_or_shifted") +
      metric("nodes_allocated") +
      Math.ceil(metric("allocated_bytes") / 8) +
      metric("tree_rebalances") +
      metric("full_scans") +
      metric("full_copies") +
      metric("dense_index_rewrites") +
      metric("fallback_or_rebuild_count");
    maximumDepth = Math.max(maximumDepth, metric("maximum_depth"));
    rebalances += metric("tree_rebalances");
  }
  return { total, maximumDepth, rebalances };
}

function assertBounded(response, label) {
  if (!response.ok) throw new Error(`${label}: ${response.error?.code}: ${response.error?.message}`);
  const failures = [];
  for (const prefix of ["document", "scene"]) {
    for (const suffix of ["full_scans", "full_copies", "dense_index_rewrites", "fallback_or_rebuild_count"]) {
      if (Number(response.metrics[`${prefix}_sequence_${suffix}`] ?? 0) !== 0) failures.push(`${prefix}.${suffix}`);
    }
  }
  if (Number(response.metrics.fallback_rebuild_count ?? 0) !== 0) failures.push("fallback_rebuild_count");
  if (Number(response.metrics.full_render_model_scans ?? 0) !== 0) failures.push("full_render_model_scans");
  if (Number(response.metrics.render_items_cloned ?? 0) !== 0) failures.push("render_items_cloned");
  if (Number(response.render_delta.scene_full_rebuilds ?? 0) !== 0) failures.push("scene_full_rebuilds");
  if (Number(response.render_delta.render_full_rebuilds ?? 0) !== 0) failures.push("render_full_rebuilds");
  if (Number(response.render_delta.dirty_slots ?? 0) !== 0) failures.push("structural_gpu_dirty_slots");
  if (Number(response.metrics.instance_upload_bytes ?? 0) !== 0) failures.push("structural_gpu_upload");
  if (failures.length) throw new Error(`${label}: bounded assertions failed: ${failures.join(",")}`);
}

function targetIndexes(count, k, pattern) {
  if (pattern === "contiguous_front") return Array.from({ length: k }, (_, index) => index + 1);
  if (pattern === "contiguous_middle") {
    const start = Math.floor((count - k) / 2) + 1;
    return Array.from({ length: k }, (_, index) => start + index);
  }
  if (pattern === "uniform") {
    if (k === 1) return [1];
    return Array.from({ length: k }, (_, index) => 1 + Math.floor((index * (count - 1)) / (k - 1)));
  }
  if (pattern === "random_seed_0x5eed") {
    let state = 0x5eed ^ count ^ k;
    const selected = new Set();
    while (selected.size < k) {
      state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
      selected.add(1 + (state % count));
    }
    return [...selected].sort((left, right) => left - right);
  }
  if (k === count) return Array.from({ length: count }, (_, index) => index + 1);
  const runCount = Math.min(7, k, count - k + 1);
  const baseLength = Math.floor(k / runCount);
  let remainder = k % runCount;
  const gap = Math.floor((count - k) / (runCount + 1));
  let cursor = 1 + gap;
  const result = [];
  for (let run = 0; run < runCount; run += 1) {
    const length = baseLength + (remainder-- > 0 ? 1 : 0);
    for (let offset = 0; offset < length; offset += 1) result.push(cursor + offset);
    cursor += length + gap;
  }
  return result;
}

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
let rawInstance;
try {
  rawInstance = new WebAssembly.Instance(module, dummyImports(module));
} catch (error) {
  throw new Error(`explicit WebAssembly.Instance construction failed: ${error instanceof Error ? error.stack : error}`);
}
const glue = await import(pathToFileURL(gluePath).href + `?r3=${Date.now()}`);
glue.initSync({ module });
const host = new glue.EngineHost();
let requestSequence = 0;
function call(type, body = {}) {
  const request = { protocol_version: 1, request_id: `direct-r3-${++requestSequence}`, type, ...body };
  return JSON.parse(host.handleJson(JSON.stringify(request)));
}

const initialized = call("initialize");
if (!initialized.ok) throw new Error(`WASM initialize failed: ${JSON.stringify(initialized.error)}`);
const ks = focus ? [1_000] : quick ? [3, 30] : [3, 30, 100, 300, 1_000, 3_000, 10_000];
const patterns = focus ? ["contiguous_middle"] : quick ? ["contiguous_front"] : ["contiguous_front", "contiguous_middle", "uniform", "random_seed_0x5eed", "multiple_runs"];
const counts = focus ? [100_000] : quick ? [10_000] : [10_000, 100_000];
const warmups = focus ? 10 : quick ? 1 : 10;
const measured = focus ? 30 : quick ? 2 : 30;
const matrix = [];

for (const count of counts) {
  const fixture = count === 10_000 ? "bench-b" : "bench-c";
  const loaded = call("load_fixture", { fixture });
  if (!loaded.ok) throw new Error(`fixture ${fixture} failed: ${JSON.stringify(loaded.error)}`);
  for (const k of ks.filter((value) => value <= count)) {
    for (const pattern of patterns) {
      const reset = call("load_fixture", { fixture });
      if (!reset.ok) throw new Error("fixture reset " + fixture + " failed: " + JSON.stringify(reset.error));
      const indexes = targetIndexes(count, k, pattern);
      if (indexes.length !== k) throw new Error(`${count}/${k}/${pattern}: target generator produced ${indexes.length}`);
      const targets = indexes.map(fixtureNodeId);
      const groupId = fixtureNodeId(BigInt(count) * 1000n + BigInt(k) * 10n + BigInt(patterns.indexOf(pattern) + 1));
      const samples = Object.fromEntries(["group", "undo", "redo", "ungroup"].map((name) => [name, { times: [], work: [], depth: [], rebalances: [] }]));
      for (let iteration = 0; iteration < warmups + measured; iteration += 1) {
        for (const [operation, type, body] of [
          ["group", "command", { command: { kind: "group", group_id: groupId, name: "R3 direct WASM", targets } }],
          ["undo", "undo", {}],
          ["redo", "redo", {}],
          ["ungroup", "command", { command: { kind: "ungroup", node_id: groupId } }],
        ]) {
          const before = performance.now();
          const response = call(type, body);
          const elapsed = performance.now() - before;
          assertBounded(response, `${count}/${k}/${pattern}/${operation}`);
          if (iteration >= warmups) {
            const work = engineWork(response);
            samples[operation].times.push(elapsed);
            samples[operation].work.push(work.total);
            samples[operation].depth.push(work.maximumDepth);
            samples[operation].rebalances.push(work.rebalances);
          }
        }
      }
      const saved = call("save_document");
      if (!saved.ok) throw new Error("save after measured cycle failed");
      const document = JSON.parse(saved.result.document_json);
      const root = document.document.nodes.find((node) => node.id === fixtureNodeId(0));
      const expected = Array.from({ length: count }, (_, index) => fixtureNodeId(index + 1));
      if (!root || root.children.length !== count || root.children.some((value, index) => value !== expected[index])) {
        throw new Error(`${count}/${k}/${pattern}: exact sibling order was not restored`);
      }
      matrix.push({
        node_count: count,
        selection_count: k,
        pattern,
        target_ids: targets,
        operations: Object.fromEntries(Object.entries(samples).map(([name, value]) => [name, summarize(value)])),
        exact_sibling_order_restored: true,
      });
    }
  }
}

function entry(count, k, pattern = "contiguous_middle") {
  return matrix.find((value) => value.node_count === count && value.selection_count === k && value.pattern === pattern);
}
const thresholds = [];
if (!quick && !focus) {
  for (const count of counts) {
    for (const pattern of patterns) {
      const work30 = entry(count, 30, pattern).operations.group.maximum_work / 30;
      const work3000 = entry(count, 3_000, pattern).operations.group.maximum_work / 3_000;
      thresholds.push({ name: `work_per_k_${count}_${pattern}`, value: work3000 / work30, limit: 2, passed: work3000 / work30 <= 2 });
      const work300 = entry(count, 300, pattern).operations.group.maximum_work;
      const work3000Total = entry(count, 3_000, pattern).operations.group.maximum_work;
      thresholds.push({ name: `x10_work_${count}_${pattern}`, value: work3000Total / work300, limit: 15, passed: work3000Total / work300 <= 15 });
    }
  }
  for (const k of ks) for (const pattern of patterns) {
    const ratio = entry(100_000, k, pattern).operations.group.maximum_work / entry(10_000, k, pattern).operations.group.maximum_work;
    thresholds.push({ name: `n_ratio_${k}_${pattern}`, value: ratio, limit: 1.35, passed: ratio <= 1.35 });
  }
  const wasm1000 = entry(100_000, 1_000).operations.group.median_ms;
  thresholds.push({ name: "actual_wasm_k1000_group_median_ms", value: wasm1000, limit: 25, passed: wasm1000 <= 25 });
  const wasmRatio = entry(100_000, 3_000).operations.group.median_ms / entry(100_000, 300).operations.group.median_ms;
  thresholds.push({ name: "actual_wasm_k3000_over_k300_median", value: wasmRatio, limit: 15, passed: wasmRatio <= 15 });
}
if (thresholds.some((threshold) => !threshold.passed)) {
  throw new Error(`R3 direct WASM thresholds failed: ${JSON.stringify(thresholds.filter((value) => !value.passed))}`);
}

const report = {
  phase: "0E-R3",
  layer: "actual-engine-host-bg-wasm-via-node",
  command: "node scripts/direct-wasm-r3.mjs",
  started_at_utc: startedAt,
  finished_at_utc: new Date().toISOString(),
  exit_code: 0,
  environment: { node: process.version, v8: process.versions.v8, platform: process.platform, architecture: process.arch },
  boundaries: {
    glue_path: path.relative(workspace, gluePath).replaceAll("\\", "/"),
    wasm_path: path.relative(workspace, wasmPath).replaceAll("\\", "/"),
    wasm_sha256: wasmSha256,
    wasm_byte_length: wasmBytes.byteLength,
    webassembly_module_created: module instanceof WebAssembly.Module,
    webassembly_instance_created: rawInstance instanceof WebAssembly.Instance,
    glue_instance_created: host instanceof glue.EngineHost,
    native_binary_used: false,
  },
  protocol_version: glue.EngineHost.protocolVersion(),
  render_binary_schema_version: glue.EngineHost.renderBinarySchemaVersion(),
  initialize_request: initialized,
  warmups,
  measured_iterations: measured,
  fixture_creation_and_load_excluded: true,
  no_op_measurements_forbidden: true,
  matrix,
  thresholds,
  all_passed: true,
};
await fs.mkdir(path.dirname(outputPath), { recursive: true });
await fs.writeFile(outputPath, JSON.stringify(report, null, 2) + "\n", "utf8");
console.log(`PHASE0E_R3_ACTUAL_WASM_JSON=${JSON.stringify({ wasm_sha256: wasmSha256, wasm_byte_length: wasmBytes.byteLength, matrix_entries: matrix.length, thresholds, all_passed: true })}`);
console.log(`output=${outputPath}`);

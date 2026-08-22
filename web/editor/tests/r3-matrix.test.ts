import { describe, expect, test } from "vitest";
import { ProjectionStore } from "../src/engine";
import type { StoredProjectionNode } from "../src/engine";
import type { EngineResponse, ProjectionNode, StructuralProjectionOperation, UiCounters } from "../src/types";

const FOCUS = process.env.PHASE0E_R3_REACT_FOCUS === "1";
const FOCUS_K = Number(process.env.PHASE0E_R3_REACT_FOCUS_K ?? "3");
const KS = FOCUS ? [FOCUS_K] : [3, 30, 100, 300, 1_000, 3_000, 10_000];
const ALL_PATTERNS = ["contiguous_front", "contiguous_middle", "uniform", "random_seed_0x5eed", "multiple_runs"] as const;
type Pattern = typeof ALL_PATTERNS[number];
const PATTERNS: readonly Pattern[] = ALL_PATTERNS;

function node(id: string, parent: string | null, children: string[] = []): ProjectionNode {
  return {
    id, parent_id: parent, children, name: id, kind: parent === null ? "document" : "rectangle",
    visible: true, locked: false, opacity: 1,
    local_transform: [1, 0, 0, 1, 0, 0], world_transform: [1, 0, 0, 1, 0, 0],
    geometry: parent === null ? null : { width: 10, height: 10 },
    world_bounds: parent === null ? null : { min: [0, 0], max: [10, 10] },
  };
}

function protocolNode(stored: StoredProjectionNode, update: Partial<ProjectionNode> = {}): ProjectionNode {
  return { ...stored, children: stored.children.toArray(), ...update };
}

function response(full: boolean, upserts: ProjectionNode[], removed: string[] = [], structuralOps: StructuralProjectionOperation[] = []): EngineResponse {
  return {
    type: "engine_response", protocol_version: 1, render_binary_schema_version: 1, engine_sequence: 1,
    request_id: "r3-matrix", ok: true, result: {}, error: null, runtime_owner: "dedicated-worker", fixture: "r3",
    revisions: { document: 1, scene: 1, render: 1 },
    projection: { schema_version: 2, full, upserts, removed, structural_ops: structuralOps },
    selection: { ordered: [], primary: null }, active_root: "root", history: { undo_depth: 0, redo_depth: 0, transaction_active: false },
    camera: { center: [0, 0], zoom: 1, viewport: [100, 100], dpr: 1 }, culling: {},
    render_delta: { full: false, dirty_slots: 0, dirty_ranges: 0, removed_slots: 0, upload_bytes: 0, scene_full_rebuilds: 0, render_full_rebuilds: 0 },
    resources: { instance_stride_bytes: 48, dirty_record_stride_bytes: 52, instance_capacity: 0, total_render_items: 0, renderable_items: 0, gpu_encodable_items: 0, gpu_omitted_items: 0, allocated_slots: 0, free_slots: 0 },
    render_encoding: { gpu_omitted_items: 0, diagnostics: [] }, binary: {}, metrics: {},
  };
}

function targets(count: number, k: number, pattern: Pattern): number[] {
  if (pattern === "contiguous_front") return Array.from({ length: k }, (_, index) => index);
  if (pattern === "contiguous_middle") {
    const start = Math.floor((count - k) / 2);
    return Array.from({ length: k }, (_, index) => start + index);
  }
  if (pattern === "uniform") return Array.from({ length: k }, (_, index) => Math.floor(index * (count - 1) / (k - 1)));
  if (pattern === "random_seed_0x5eed") {
    let state = 0x5eed ^ count ^ k;
    const selected = new Set<number>();
    while (selected.size < k) {
      state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
      selected.add(state % count);
    }
    return [...selected].sort((left, right) => left - right);
  }
  if (k === count) return Array.from({ length: count }, (_, index) => index);
  const runCount = Math.min(7, k, count - k + 1);
  const baseLength = Math.floor(k / runCount);
  let remainder = k % runCount;
  const gap = Math.floor((count - k) / (runCount + 1));
  let cursor = gap;
  const result: number[] = [];
  for (let run = 0; run < runCount; run += 1) {
    const length = baseLength + (remainder-- > 0 ? 1 : 0);
    for (let offset = 0; offset < length; offset += 1) result.push(cursor + offset);
    cursor += length + gap;
  }
  return result;
}

function delta(after: UiCounters, before: UiCounters): UiCounters {
  const result = {} as UiCounters;
  for (const key of Object.keys(after) as (keyof UiCounters)[]) result[key] = after[key] - before[key];
  return result;
}

function work(value: UiCounters): number {
  return Math.max(value.sequenceEntriesExamined, value.sequenceRankOrderComparisons) +
    value.sequenceEntriesCopied + value.sequenceEntriesMovedOrShifted + value.sequenceNodesAllocated + Math.ceil(value.sequenceAllocatedBytes / 8) +
    value.sequenceTreeRebalances + value.sequenceFullScans + value.sequenceFullCopies +
    value.sequenceDenseIndexRewrites + value.sequenceFallbackOrRebuildCount;
}

function assertBounded(value: UiCounters): void {
  expect(value.sequenceFullScans).toBe(0);
  expect(value.sequenceFullCopies).toBe(0);
  expect(value.sequenceDenseIndexRewrites).toBe(0);
  expect(value.sequenceFallbackOrRebuildCount).toBe(0);
  expect(value.hierarchyFullRebuilds).toBe(0);
  expect(value.projectionStructuralOperations).toBe(1);
}

function percentile(values: number[], fraction: number): number {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.ceil((sorted.length - 1) * fraction)];
}

interface Samples { times: number[]; work: number[]; depth: number[]; rebalances: number[]; lastCounters?: UiCounters }
interface Entry { node_count: number; selection_count: number; pattern: string; operations: Record<string, ReturnType<typeof summary>>; exact_sibling_order_restored: boolean }

function summary(samples: Samples) {
  return {
    raw_samples_ms: samples.times, raw_work_samples: samples.work,
    median_ms: percentile(samples.times, 0.5), p95_ms: percentile(samples.times, 0.95),
    maximum_work: Math.max(...samples.work), maximum_sequence_depth: Math.max(...samples.depth),
    tree_rebalances: Math.max(...samples.rebalances), counters: samples.lastCounters,
  };
}

async function run(count: number): Promise<Entry[]> {
  const children = Array.from({ length: count }, (_, index) => `node-${index}`);
  const baseline = [node("root", null, children), ...children.map((id) => node(id, "root"))];
  const entries: Entry[] = [];
  for (const k of KS) for (const pattern of PATTERNS) {
    const store = new ProjectionStore();
    store.apply(response(true, baseline));
    const positions = targets(count, k, pattern);
    const targetIds = positions.map((index) => children[index]);
    const groupId = `group-${count}-${k}-${pattern}`;
    const samples = Object.fromEntries(["group", "undo", "redo", "ungroup"].map((name) => [name, { times: [], work: [], depth: [], rebalances: [] } as Samples]));
    const applyGroup = () => {
      const groupNode = { ...node(groupId, "root", targetIds), kind: "group" as const, geometry: null, world_bounds: null };
      const moved = targetIds.map((id) => protocolNode(store.nodes.get(id)!, { parent_id: groupId }));
      store.apply(response(false, [groupNode, ...moved], [], [{ kind: "group", group_id: groupId, parent_id: "root", index: positions[0], children: targetIds, positions }]));
    };
    const applyUngroup = () => {
      const restored = targetIds.map((id) => protocolNode(store.nodes.get(id)!, { parent_id: "root" }));
      store.apply(response(false, restored, [groupId], [{ kind: "ungroup", group_id: groupId, parent_id: "root", index: positions[0], children: targetIds, positions }]));
    };
    for (let iteration = 0; iteration < 40; iteration += 1) {
      for (const [name, operation] of [["group", applyGroup], ["undo", applyUngroup], ["redo", applyGroup], ["ungroup", applyUngroup]] as const) {
        const before = { ...store.counters };
        const started = performance.now();
        operation();
        const elapsed = performance.now() - started;
        const measured = delta({ ...store.counters }, before);
        assertBounded(measured);
        if (iteration >= 10) {
          samples[name].times.push(elapsed);
          samples[name].work.push(work(measured));
          samples[name].depth.push(store.counters.sequenceMaximumDepth);
          samples[name].rebalances.push(measured.sequenceTreeRebalances);
          samples[name].lastCounters = measured;
        }
      }
    }
    expect(store.nodes.get("root")?.children.toArray()).toEqual(children);
    entries.push({
      node_count: count, selection_count: k, pattern,
      operations: Object.fromEntries(Object.entries(samples).map(([name, value]) => [name, summary(value)])),
      exact_sibling_order_restored: true,
    });
    await new Promise<void>((resolve) => setImmediate(resolve));
  }
  return entries;
}

describe("Phase 0E-R3 React ProjectionStore complexity matrix", () => {
  test("all N, k, and selection patterns remain bounded and exact", async () => {
    const started = new Date().toISOString();
    const matrix = [...await run(10_000), ...await run(100_000)];
    const entry = (count: number, k: number, pattern: string) => matrix.find((value) => value.node_count === count && value.selection_count === k && value.pattern === pattern)!;
    if (FOCUS) {
      const groupWork = Object.fromEntries(matrix.map((value) => [
        `${value.node_count}/${value.pattern}`,
        value.operations.group.maximum_work,
      ]));
      const groupCounters = Object.fromEntries(matrix.map((value) => [
        `${value.node_count}/${value.pattern}`, value.operations.group.counters,
      ]));
      const ratios = Object.fromEntries(PATTERNS.map((pattern) => [
        pattern,
        entry(100_000, FOCUS_K, pattern).operations.group.maximum_work /
          entry(10_000, FOCUS_K, pattern).operations.group.maximum_work,
      ]));
      console.log("PHASE0E_R3_REACT_FOCUS_JSON=" + JSON.stringify({
        selection_count: FOCUS_K, group_work: groupWork, group_counters: groupCounters, n_ratios: ratios,
      }));
      return;
    }
    const thresholds: Array<{ name: string; value: number; limit: number; passed: boolean }> = [];
    for (const count of [10_000, 100_000]) for (const pattern of PATTERNS) {
      const perK = (entry(count, 3_000, pattern).operations.group.maximum_work / 3_000) / (entry(count, 30, pattern).operations.group.maximum_work / 30);
      expect(perK).toBeLessThanOrEqual(2);
      thresholds.push({ name: `work_per_k_${count}_${pattern}`, value: perK, limit: 2, passed: true });
      const x10 = entry(count, 3_000, pattern).operations.group.maximum_work / entry(count, 300, pattern).operations.group.maximum_work;
      expect(x10).toBeLessThanOrEqual(15);
      thresholds.push({ name: `x10_work_${count}_${pattern}`, value: x10, limit: 15, passed: true });
    }
    for (const k of KS) for (const pattern of PATTERNS) {
      const ratio = entry(100_000, k, pattern).operations.group.maximum_work / entry(10_000, k, pattern).operations.group.maximum_work;
      expect(ratio).toBeLessThanOrEqual(1.35);
      thresholds.push({ name: `n_ratio_${k}_${pattern}`, value: ratio, limit: 1.35, passed: true });
    }
    console.log("PHASE0E_R3_REACT_MATRIX_JSON=" + JSON.stringify({
      phase: "0E-R3", layer: "react-projection-store", command: "vitest run tests/r3-matrix.test.ts",
      started_at_utc: started, finished_at_utc: new Date().toISOString(), warmups: 10, measured_iterations: 30,
      fixture_creation_and_load_excluded: true, matrix, thresholds, all_passed: true,
    }));
  }, 1_200_000);
});

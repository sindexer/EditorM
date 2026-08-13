import { describe, expect, test } from "vitest";
import { ProjectionStore } from "../src/engine";
import type { StoredProjectionNode } from "../src/engine";
import { RankedSequence, emptySequenceWork } from "../src/rankedSequence";
import type { EngineResponse, ProjectionNode, StructuralProjectionOperation } from "../src/types";

function node(id: string, parent: string | null, children: string[] = []): ProjectionNode {
  return {
    id,
    parent_id: parent,
    children,
    name: id,
    kind: parent === null ? "document" : "rectangle",
    visible: true,
    locked: false,
    opacity: 1,
    local_transform: [1, 0, 0, 1, 0, 0],
    world_transform: [1, 0, 0, 1, 0, 0],
    geometry: parent === null ? null : { width: 10, height: 10 },
    world_bounds: parent === null ? null : { min: [0, 0], max: [10, 10] },
  };
}

function protocolNode(stored: StoredProjectionNode, update: Partial<ProjectionNode> = {}): ProjectionNode {
  return { ...stored, children: stored.children.toArray(), ...update };
}

function response(full: boolean, upserts: ProjectionNode[], removed: string[] = [], structuralOps: StructuralProjectionOperation[] = []): EngineResponse {
  return {
    type: "engine_response",
    protocol_version: 1,
    render_binary_schema_version: 1,
    engine_sequence: 1,
    request_id: "test",
    ok: true,
    result: {},
    error: null,
    runtime_owner: "dedicated-worker",
    fixture: "test",
    revisions: { document: 1, scene: 1, render: 1 },
    projection: { schema_version: 2, full, upserts, removed, structural_ops: structuralOps },
    selection: { ordered: [], primary: null },
    history: { undo_depth: 0, redo_depth: 0, transaction_active: false },
    camera: { center: [0, 0], zoom: 1, viewport: [100, 100], dpr: 1 },
    culling: {},
    render_delta: { full: false, dirty_slots: 0, dirty_ranges: 0, removed_slots: 0, upload_bytes: 0, scene_full_rebuilds: 0, render_full_rebuilds: 0 },
    resources: { instance_stride_bytes: 48, dirty_record_stride_bytes: 52, instance_capacity: 0, total_render_items: 0, renderable_items: 0, gpu_encodable_items: 0, gpu_omitted_items: 0, allocated_slots: 0, free_slots: 0 },
    render_encoding: { gpu_omitted_items: 0, diagnostics: [] },
    binary: {},
    metrics: {},
  };
}

describe("versioned UI projection", () => {
  test("single-leaf updates do not rebuild or serialize the hierarchy", () => {
    const store = new ProjectionStore();
    const children = Array.from({ length: 10_000 }, (_, index) => `node-${index}`);
    store.apply(response(true, [node("root", null, children), ...children.map((id) => node(id, "root"))]));
    const rebuilds = store.counters.hierarchyRebuilds;
    const serializes = store.counters.layersFullSerializes;
    const moved = protocolNode(store.nodes.get("node-500")!, { local_transform: [1, 0, 0, 1, 25, 25] as ProjectionNode["local_transform"] });
    store.apply(response(false, [moved]));

    expect(store.order).toHaveLength(10_001);
    expect(store.counters.hierarchyRebuilds).toBe(rebuilds);
    expect(store.counters.layersFullSerializes).toBe(serializes);
    expect(store.counters.projectionFullSnapshots).toBe(1);
    expect(store.counters.projectionDeltaNodes).toBe(1);
  });

  test("compact group deltas update and restore child parent and local transforms without child upserts", () => {
    const store = new ProjectionStore();
    store.apply(response(true, [node("root", null, ["a", "b", "c"]), node("a", "root"), node("b", "root"), node("c", "root")]));
    const groupedA: ProjectionNode["local_transform"] = [1, 0, 0, 1, -10, 0];
    const groupedC: ProjectionNode["local_transform"] = [1, 0, 0, 1, 10, 0];
    const group = { ...node("group", "root", ["a", "c"]), kind: "group" as const, geometry: null, world_bounds: null };
    store.apply(
      response(false, [group], [], [
        {
          kind: "group",
          group_id: "group",
          parent_id: "root",
          index: 0,
          children: ["a", "c"],
          child_local_transforms: [groupedA, groupedC],
          positions: [0, 2],
        },
      ]),
    );
    expect(store.nodes.get("a")?.parent_id).toBe("group");
    expect(store.nodes.get("c")?.parent_id).toBe("group");
    expect(store.nodes.get("a")?.local_transform).toEqual(groupedA);
    expect(store.nodes.get("c")?.local_transform).toEqual(groupedC);

    const restored: ProjectionNode["local_transform"] = [1, 0, 0, 1, 0, 0];
    store.apply(
      response(false, [], ["group"], [
        {
          kind: "ungroup",
          group_id: "group",
          parent_id: "root",
          index: 0,
          children: ["a", "c"],
          child_local_transforms: [restored, restored],
          positions: [0, 2],
        },
      ]),
    );
    expect(store.nodes.get("root")?.children.toArray()).toEqual(["a", "b", "c"]);
    expect(store.nodes.get("a")?.parent_id).toBe("root");
    expect(store.nodes.get("c")?.parent_id).toBe("root");
    expect(store.nodes.get("a")?.local_transform).toEqual(restored);
  });

  test("100k group and ungroup use bounded structural operations without a full hierarchy rebuild", () => {
    const store = new ProjectionStore();
    const children = Array.from({ length: 100_000 }, (_, index) => "node-" + index);
    store.apply(response(true, [node("root", null, children), ...children.map((id) => node(id, "root"))]));
    const fullRebuilds = store.counters.hierarchyFullRebuilds;
    const first = children[0];
    const middle = children[50_000];
    const last = children[99_999];
    const group = { ...node("group", "root", [first, middle, last]), kind: "group" as const, geometry: null, world_bounds: null };
    const moved = [first, middle, last].map((id) => protocolNode(store.nodes.get(id)!, { parent_id: "group" }));
    const positions = [0, 50_000, 99_999];
    const operation: StructuralProjectionOperation = { kind: "group", group_id: "group", parent_id: "root", index: 0, children: [first, middle, last], positions };
    store.apply(response(false, [group, ...moved], [], [operation]));

    expect(store.nodes.get("root")?.children.at(0)).toBe("group");
    expect(store.nodes.get("root")?.children).toHaveLength(99_998);
    expect(store.order.slice(0, 5)).toEqual(["root", "group", first, middle, last]);
    expect(store.counters.hierarchyFullRebuilds).toBe(fullRebuilds);
    expect(store.counters.hierarchyNodesVisited).toBe(100_001 + 5);

    const restored = [first, middle, last].map((id) => protocolNode(store.nodes.get(id)!, { parent_id: "root" }));
    store.apply(response(false, restored, ["group"], [{ kind: "ungroup", group_id: "group", parent_id: "root", index: 0, children: [first, middle, last], positions }]));
    expect(store.nodes.has("group")).toBe(false);
    expect(store.nodes.get("root")?.children.toArray()).toEqual(children);
    expect(store.order.slice(0, 5)).toEqual(["root", ...children.slice(0, 4)]);
    expect(store.counters.hierarchyFullRebuilds).toBe(fullRebuilds);
  });
});

describe("ranked projection sequence", () => {
  test("rank, extraction, and fragment insertion preserve exact order", () => {
    const buildWork = emptySequenceWork();
    const sequence = RankedSequence.from(["a", "b", "c", "d", "e", "f", "g", "h"], buildWork);
    const work = emptySequenceWork();
    const fragment = sequence.extractByValue("c", 3, work);
    sequence.insertFragment(1, fragment, work);
    expect(sequence.toArray()).toEqual(["a", "c", "d", "e", "b", "f", "g", "h"]);
    expect(sequence.rankOf("e", work)).toBe(3);
    expect(sequence.at(4, work)).toBe("b");
    expect(work.fullSequenceScans).toBe(0);
    expect(work.fullSequenceCopies).toBe(0);
    expect(work.denseIndexRewrites).toBe(0);
  });
});


describe("R3 adversarial ranked sequence balance", () => {
  test("monotonic, reverse, and collision-shaped IDs stay logarithmic", () => {
    for (const values of [
      Array.from({ length: 10_000 }, (_, index) => "monotonic-" + index.toString().padStart(8, "0")),
      Array.from({ length: 10_000 }, (_, index) => "reverse-" + (9_999 - index).toString().padStart(8, "0")),
      Array.from({ length: 10_000 }, (_, index) => "collision-prefix-00000000-" + index),
    ]) {
      const work = emptySequenceWork();
      const sequence = RankedSequence.from(values, work);
      for (let offset = 0; offset < 1_000; offset += 1) {
        const rank = offset % 3 === 0 ? 0 : offset % 3 === 1 ? Math.floor(sequence.length / 2) : sequence.length;
        sequence.insert(rank, "edge-" + values[0] + "-" + offset, work);
      }
      expect(sequence.maximumDepth).toBeLessThanOrEqual(RankedSequence.logarithmicDepthBound(sequence.length));
      expect(work.maximumSequenceDepth).toBeLessThanOrEqual(RankedSequence.logarithmicDepthBound(sequence.length));
      expect(work.treeRebalances).toBeGreaterThan(0);
      expect(work.fallbackOrRebuildCount).toBe(0);
      expect(work.fullSequenceScans).toBe(0);
      expect(work.denseIndexRewrites).toBe(0);
    }
  });
});


interface ProjectionBenchResult {
  nodeCount: number;
  targets: string[];
  groupTimes: number[];
  ungroupTimes: number[];
  groupWork: number[];
  ungroupWork: number[];
}

function counterSnapshot(store: ProjectionStore) {
  return { ...store.counters };
}

function counterDelta(after: ReturnType<typeof counterSnapshot>, before: ReturnType<typeof counterSnapshot>) {
  const result = {} as ReturnType<typeof counterSnapshot>;
  for (const key of Object.keys(after) as (keyof typeof after)[]) result[key] = after[key] - before[key];
  return result;
}

function uiStructuralWork(counters: ReturnType<typeof counterSnapshot>): number {
  return Math.max(counters.sequenceEntriesExamined, counters.sequenceRankOrderComparisons) +
    counters.sequenceEntriesCopied +
    counters.sequenceEntriesMovedOrShifted +
    counters.sequenceNodesAllocated +
    counters.sequenceFullScans +
    counters.sequenceFullCopies +
    counters.sequenceDenseIndexRewrites;
}

function assertUiBounded(counters: ReturnType<typeof counterSnapshot>) {
  expect(counters.sequenceFullScans).toBe(0);
  expect(counters.sequenceFullCopies).toBe(0);
  expect(counters.sequenceDenseIndexRewrites).toBe(0);
  expect(counters.hierarchyFullRebuilds).toBe(0);
  expect(counters.projectionStructuralOperations).toBe(1);
}

function percentile(values: number[], fraction: number): number {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.ceil((sorted.length - 1) * fraction)];
}

function runProjectionBenchmark(nodeCount: number): ProjectionBenchResult {
  const store = new ProjectionStore();
  const children = Array.from({ length: nodeCount }, (_, index) => `node-${index}`);
  store.apply(response(true, [node("root", null, children), ...children.map((id) => node(id, "root"))]));
  const targets = [children[0], children[Math.floor(nodeCount / 2)], children[nodeCount - 1]];
  const positions = [0, Math.floor(nodeCount / 2), nodeCount - 1];
  const groupId = `group-${nodeCount}`;
  const result: ProjectionBenchResult = { nodeCount, targets, groupTimes: [], ungroupTimes: [], groupWork: [], ungroupWork: [] };

  for (let iteration = 0; iteration < 40; iteration += 1) {
    const groupNode = { ...node(groupId, "root", targets), kind: "group" as const, geometry: null, world_bounds: null };
    const moved = targets.map((id) => protocolNode(store.nodes.get(id)!, { parent_id: groupId }));
    const beforeGroup = counterSnapshot(store);
    const startedGroup = performance.now();
    store.apply(response(false, [groupNode, ...moved], [], [{ kind: "group", group_id: groupId, parent_id: "root", index: 0, children: targets, positions }]));
    const groupElapsed = performance.now() - startedGroup;
    const groupDelta = counterDelta(counterSnapshot(store), beforeGroup);
    assertUiBounded(groupDelta);

    const restored = targets.map((id) => protocolNode(store.nodes.get(id)!, { parent_id: "root" }));
    const beforeUngroup = counterSnapshot(store);
    const startedUngroup = performance.now();
    store.apply(response(false, restored, [groupId], [{ kind: "ungroup", group_id: groupId, parent_id: "root", index: 0, children: targets, positions }]));
    const ungroupElapsed = performance.now() - startedUngroup;
    const ungroupDelta = counterDelta(counterSnapshot(store), beforeUngroup);
    assertUiBounded(ungroupDelta);

    if (iteration >= 10) {
      result.groupTimes.push(groupElapsed);
      result.ungroupTimes.push(ungroupElapsed);
      result.groupWork.push(uiStructuralWork(groupDelta));
      result.ungroupWork.push(uiStructuralWork(ungroupDelta));
    }
  }
  expect(store.nodes.get("root")?.children.toArray()).toEqual(children);
  return result;
}

describe("R2 React ProjectionStore bounded benchmark", () => {
  test("10k and 100k group/ungroup use 10 warmups and 30 measured iterations", () => {
    const tenK = runProjectionBenchmark(10_000);
    const hundredK = runProjectionBenchmark(100_000);
    const ratios = {
      groupWork: Math.max(...hundredK.groupWork) / Math.max(...tenK.groupWork),
      ungroupWork: Math.max(...hundredK.ungroupWork) / Math.max(...tenK.ungroupWork),
      groupMedianTime: percentile(hundredK.groupTimes, 0.5) / percentile(tenK.groupTimes, 0.5),
      ungroupMedianTime: percentile(hundredK.ungroupTimes, 0.5) / percentile(tenK.ungroupTimes, 0.5),
    };
    expect(ratios.groupWork).toBeLessThanOrEqual(1.35);
    expect(ratios.ungroupWork).toBeLessThanOrEqual(1.35);
    expect(ratios.groupMedianTime).toBeLessThanOrEqual(3.0);
    expect(ratios.ungroupMedianTime).toBeLessThanOrEqual(3.0);
    const summarize = (fixture: ProjectionBenchResult) => ({
      node_count: fixture.nodeCount,
      target_ids: fixture.targets,
      warmups: 10,
      measured_iterations: 30,
      group: { samples_ms: fixture.groupTimes, median_ms: percentile(fixture.groupTimes, 0.5), p95_ms: percentile(fixture.groupTimes, 0.95), work_samples: fixture.groupWork },
      ungroup: { samples_ms: fixture.ungroupTimes, median_ms: percentile(fixture.ungroupTimes, 0.5), p95_ms: percentile(fixture.ungroupTimes, 0.95), work_samples: fixture.ungroupWork },
    });
    console.log("PHASE0E_R2_REACT_JSON=" + JSON.stringify({ phase: "0E-R2", layer: "react-projection-store", fixtures: [summarize(tenK), summarize(hundredK)], ratios_100k_over_10k: ratios, all_passed: true }));
  }, 30_000);
});


describe("R2 Layers and engine z-order agreement", () => {
  test("eight overlapping siblings retain exact noncontiguous group and ungroup order", () => {
    const store = new ProjectionStore();
    const siblings = Array.from({ length: 8 }, (_, index) => `sibling-${index}`);
    store.apply(response(true, [node("root", null, siblings), ...siblings.map((id) => node(id, "root"))]));
    const targets = [siblings[0], siblings[3], siblings[7]];
    const positions = [0, 3, 7];
    const groupId = "group-eight";
    const groupNode = { ...node(groupId, "root", targets), kind: "group" as const, geometry: null, world_bounds: null };
    const moved = targets.map((id) => protocolNode(store.nodes.get(id)!, { parent_id: groupId }));
    store.apply(response(false, [groupNode, ...moved], [], [{ kind: "group", group_id: groupId, parent_id: "root", index: 0, children: targets, positions }]));
    const layersOrder = store.viewport(1, 9);
    expect(layersOrder).toEqual([groupId, siblings[0], siblings[3], siblings[7], siblings[1], siblings[2], siblings[4], siblings[5], siblings[6]]);
    const layerRenderableOrder = layersOrder.filter((id) => id !== groupId);
    const engineTopmostOrder = [siblings[6], siblings[5], siblings[4], siblings[2], siblings[1], siblings[7], siblings[3], siblings[0]];
    expect([...layerRenderableOrder].reverse()).toEqual(engineTopmostOrder);
    expect("sibling_index" in store.nodes.get(siblings[5])!).toBe(false);

    const restored = targets.map((id) => protocolNode(store.nodes.get(id)!, { parent_id: "root" }));
    store.apply(response(false, restored, [groupId], [{ kind: "ungroup", group_id: groupId, parent_id: "root", index: 0, children: targets, positions }]));
    expect(store.viewport(1, 8)).toEqual(siblings);
    expect([...store.viewport(1, 8)].reverse()).toEqual([...siblings].reverse());
    console.log("PHASE0E_R2_REACT_ORDER_JSON=" + JSON.stringify({ phase: "0E-R2", overlapping_sibling_count: 8, targets, grouped_layers_order: layersOrder, engine_topmost_order: engineTopmostOrder, restored_layers_order: store.viewport(1, 8), stale_dense_index_present: false, all_passed: true }));
  });
});

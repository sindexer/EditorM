import { randomUUID } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { beforeAll, describe, expect, test } from "vitest";
import { EngineHost, initSync } from "../public/pkg/engine_host.js";

const wasmPath = path.resolve(import.meta.dirname, "../public/pkg/engine_host_bg.wasm");
let sequence = 0;

type HostResponse = Record<string, any>;

function send(host: EngineHost, type: string, payload: Record<string, unknown> = {}): HostResponse {
  sequence += 1;
  return JSON.parse(host.handleJson(JSON.stringify({
    protocol_version: 1,
    request_id: `phase1c-${sequence}`,
    type,
    ...payload,
  })));
}

function newHost(): EngineHost {
  return new EngineHost();
}

function editorHost(): { host: EngineHost; root: string; slide: string } {
  const host = newHost();
  const response = send(host, "load_fixture", { fixture: "editor" });
  expect(response.ok).toBe(true);
  const root = response.projection.upserts.find((node: HostResponse) => node.kind === "document").id;
  return { host, root, slide: response.active_root };
}

function createShape(host: EngineHost, parent: string, index: number, x: number, y: number): string {
  const id = randomUUID();
  const response = send(host, "command", {
    command: {
      kind: "create_shape", node_id: id, parent_id: parent, index,
      shape: "rectangle", name: `Rectangle ${index}`, x, y, width: 40, height: 30,
    },
  });
  expect(response.ok).toBe(true);
  return id;
}

function snapshotNodes(host: EngineHost): Map<string, HostResponse> {
  const response = send(host, "get_ui_snapshot");
  expect(response.ok).toBe(true);
  return new Map(response.projection.upserts.map((node: HostResponse) => [node.id, node]));
}

beforeAll(() => {
  initSync({ module: new WebAssembly.Module(fs.readFileSync(wasmPath)) });
});

describe("Phase 1C direct WASM contract", () => {
  test("active slide root and other slides are excluded from selection", () => {
    const { host, root, slide } = editorHost();
    const firstChild = createShape(host, slide, 0, 10, 20);
    expect(send(host, "selection", { mode: "replace", target: slide }).error.code)
      .toBe("selection_outside_active_root");
    expect(send(host, "selection", { mode: "replace", target: firstChild }).ok).toBe(true);

    const secondSlide = randomUUID();
    expect(send(host, "command", {
      command: {
        kind: "create_shape", node_id: secondSlide, parent_id: root, index: 1,
        shape: "frame", name: "Second slide", x: 2000, y: 0, width: 1920, height: 1080,
      },
    }).ok).toBe(true);
    const switched = send(host, "set_active_root", { node_id: secondSlide });
    expect(switched.active_root).toBe(secondSlide);
    expect(switched.selection.ordered).toEqual([]);
    expect(send(host, "selection", { mode: "replace", target: firstChild }).error.code)
      .toBe("selection_outside_active_root");
  });

  test("aggregate clockwise rotation commits and undoes as one history step", () => {
    const { host, slide } = editorHost();
    const first = createShape(host, slide, 0, 10, 20);
    const second = createShape(host, slide, 1, 100, 60);
    const selected = send(host, "selection", { mode: "set", targets: [first, second] });
    const beforeDepth = selected.history.undo_depth;
    expect(send(host, "begin_transaction").ok).toBe(true);
    const preview = send(host, "transform_selection", { matrix: [0, -1, 1, 0, 0, 0] });
    expect(preview.result.transformed).toBe(2);
    const committed = send(host, "commit_transaction");
    expect(committed.history.undo_depth).toBe(beforeDepth + 1);
    const transforms = new Map<string, number[]>(preview.projection.upserts.map((node: HostResponse) => [node.id, node.local_transform]));
    expect(Math.atan2(transforms.get(first)![2], transforms.get(first)![0]) * 180 / Math.PI).toBeCloseTo(90);
    expect(Math.atan2(transforms.get(second)![2], transforms.get(second)![0]) * 180 / Math.PI).toBeCloseTo(90);
    const undone = send(host, "undo");
    const restored = new Map<string, number[]>(undone.projection.upserts.map((node: HostResponse) => [node.id, node.local_transform]));
    expect(restored.get(first)!.slice(0, 4)).toEqual([1, 0, 0, 1]);
    expect(restored.get(second)!.slice(0, 4)).toEqual([1, 0, 0, 1]);
  });

  test("invalid command batch is fail-closed with no partial mutation", () => {
    const { host, slide } = editorHost();
    const first = createShape(host, slide, 0, 10, 20);
    const snapshot = send(host, "get_ui_snapshot");
    const before = snapshot.projection.upserts.find((node: HostResponse) => node.id === first).local_transform;
    const beforeDepth = snapshot.history.undo_depth;
    const failed = send(host, "command_batch", {
      commands: [
        { kind: "set_transform", node_id: first, matrix: [1, 0, 0, 1, 500, 600] },
        { kind: "set_transform", node_id: randomUUID(), matrix: [1, 0, 0, 1, 0, 0] },
      ],
    });
    expect(failed.ok).toBe(false);
    expect(failed.history.undo_depth).toBe(beforeDepth);
    expect(failed.history.transaction_active).toBe(false);
    const after = send(host, "get_ui_snapshot").projection.upserts
      .find((node: HostResponse) => node.id === first).local_transform;
    expect(after).toEqual(before);
  });

  test("rollback restores a transform preview without adding undo history", () => {
    const { host, slide } = editorHost();
    const first = createShape(host, slide, 0, 10, 20);
    const selected = send(host, "selection", { mode: "replace", target: first });
    const beforeDepth = selected.history.undo_depth;
    const before = snapshotNodes(host).get(first)!.local_transform;
    expect(send(host, "begin_transaction").ok).toBe(true);
    expect(send(host, "transform_selection", { matrix: [1, 0, 0, 1, 80, 40] }).ok).toBe(true);
    expect(snapshotNodes(host).get(first)!.local_transform).not.toEqual(before);
    const rolledBack = send(host, "rollback_transaction");
    expect(rolledBack.ok).toBe(true);
    expect(rolledBack.history.undo_depth).toBe(beforeDepth);
    expect(rolledBack.history.transaction_active).toBe(false);
    expect(snapshotNodes(host).get(first)!.local_transform).toEqual(before);
  });

  test("group and ungroup preserve child world transforms and each add one undo step", () => {
    const { host, slide } = editorHost();
    const first = createShape(host, slide, 0, 10, 20);
    const second = createShape(host, slide, 1, 100, 80);
    const before = snapshotNodes(host);
    const group = randomUUID();
    const beforeDepth = send(host, "get_ui_snapshot").history.undo_depth;
    const grouped = send(host, "command", {
      command: { kind: "group", group_id: group, name: "Group", targets: [first, second] },
    });
    expect(grouped.ok).toBe(true);
    expect(grouped.history.undo_depth).toBe(beforeDepth + 1);
    const groupedNodes = snapshotNodes(host);
    expect(groupedNodes.get(group)!.world_bounds).not.toBeNull();
    expect(groupedNodes.get(first)!.world_transform).toEqual(before.get(first)!.world_transform);
    expect(groupedNodes.get(second)!.world_transform).toEqual(before.get(second)!.world_transform);

    const ungrouped = send(host, "command", { command: { kind: "ungroup", node_id: group } });
    expect(ungrouped.ok).toBe(true);
    expect(ungrouped.history.undo_depth).toBe(beforeDepth + 2);
    const restored = snapshotNodes(host);
    expect(restored.has(group)).toBe(false);
    expect(restored.get(first)!.world_transform).toEqual(before.get(first)!.world_transform);
    expect(restored.get(second)!.world_transform).toEqual(before.get(second)!.world_transform);
  });
});

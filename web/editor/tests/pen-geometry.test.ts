import { describe, expect, test } from "vitest";
import { appendPenAnchor, dragPenAnchor, isCloseTarget, type PenDraft } from "../src/penGeometry";

const draft = (): PenDraft => ({
  nodeId: "node",
  parentId: "root",
  origin: [100, 200],
  anchors: [{ id: "a", position: [0, 0] }],
  created: false,
});

describe("Pen geometry interaction", () => {
  test("stores anchors in stable path-local coordinates", () => {
    const next = appendPenAnchor(draft(), [140, 260], "b");
    expect(next.anchors).toEqual([
      { id: "a", position: [0, 0] },
      { id: "b", position: [40, 60] },
    ]);
  });

  test("a drag records an interaction gesture without computing path handles", () => {
    const source = appendPenAnchor(draft(), [140, 260], "b");
    const next = dragPenAnchor(source, 1, [160, 250]);
    expect(next.anchors[1]).toEqual({
      id: "b",
      position: [40, 60],
      drag: [60, 50],
    });
    expect(next.anchors[1].id).toBe("b");
  });

  test("close targeting is zoom-aware and requires three anchors", () => {
    const two = appendPenAnchor(draft(), [160, 200], "b");
    expect(isCloseTarget(two, [104, 200], 2)).toBe(false);
    const three = appendPenAnchor(two, [160, 260], "c");
    expect(isCloseTarget(three, [104, 200], 2)).toBe(true);
    expect(isCloseTarget(three, [106, 200], 2)).toBe(false);
  });

  test("invalid zoom cannot accidentally close a path", () => {
    const three = appendPenAnchor(appendPenAnchor(draft(), [160, 200], "b"), [160, 260], "c");
    expect(isCloseTarget(three, [100, 200], 0)).toBe(false);
    expect(isCloseTarget(three, [100, 200], Number.NaN)).toBe(false);
  });
});

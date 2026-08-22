import { describe, expect, it } from "vitest";
import {
  axisAlignedBox,
  normalizedAngleDelta,
  resizeTransform,
  rotationAround,
  snappedRotationDelta,
} from "../src/transformGeometry";

describe("Phase 1C transform geometry", () => {
  it("uses increasing angles for clockwise screen-space rotation", () => {
    const matrix = rotationAround([0, 0], Math.PI / 2);
    expect(matrix.map((value) => Math.abs(value) < 1e-10 ? 0 : value)).toEqual([0, -1, 1, 0, 0, 0]);
  });

  it("does not jump while crossing the 0/360 degree boundary", () => {
    const delta = normalizedAngleDelta((-179 * Math.PI / 180) - (179 * Math.PI / 180));
    expect(delta * 180 / Math.PI).toBeCloseTo(2);
  });

  it("snaps rotation deltas to 15 degrees", () => {
    expect(snappedRotationDelta(22 * Math.PI / 180, true) * 180 / Math.PI).toBeCloseTo(15);
  });

  it("resizes an aggregate box around its opposite corner", () => {
    const box = axisAlignedBox([0, 0], [100, 50]);
    expect(box).not.toBeNull();
    expect(resizeTransform(box!, "se", [200, 100], false, false)).toEqual([2, 0, 0, 2, 0, 0]);
  });

  it("supports center resize and rejects zero-crossing flips", () => {
    const box = axisAlignedBox([0, 0], [100, 100])!;
    expect(resizeTransform(box, "e", [150, 50], false, true)).toEqual([2, 0, 0, 1, -50, 0]);
    expect(resizeTransform(box, "e", [40, 50], false, true)).toBeNull();
  });
});

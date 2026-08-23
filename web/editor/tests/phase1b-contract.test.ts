import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";
import {
  arrangeAvailability,
  guideSegment,
  isMarqueeDrag,
  marqueeRect,
  pressStartsMarquee,
  selectionIntent,
  unionWorldBounds,
} from "../src/selectionGeometry";

const editorRoot = path.resolve(process.cwd());
const workspace = path.resolve(editorRoot, "../..");
const app = readFileSync(path.join(editorRoot, "src/App.tsx"), "utf8");
const styles = readFileSync(path.join(editorRoot, "src/styles.css"), "utf8");
const bridge = readFileSync(path.join(workspace, "crates/wasm_bridge/src/lib.rs"), "utf8");
const browserProof = readFileSync(
  path.join(editorRoot, "scripts/browser-proof-phase1b.mjs"),
  "utf8",
);

describe("Phase 1B selection geometry", () => {
  test("browser proof allows Chrome time to expose WebGPU", () => {
    const readinessLoop = browserProof.slice(
      browserProof.indexOf("async function waitForReady()"),
      browserProof.indexOf("async function getProof()"),
    );
    expect(readinessLoop.indexOf("await sleep(100)")).toBeLessThan(
      readinessLoop.indexOf("if (status?.navigator_gpu === false)"),
    );
    expect(readinessLoop).toContain(
      "navigator.gpu was not exposed before the readiness deadline",
    );
    expect(browserProof).toContain(
      `await waitFor("window.__PHASE0E_PROOF__?.tool === 'rectangle'")`,
    );
    expect(browserProof.match(/x: point\.center_x \?\? point\.x/g)).toHaveLength(2);
    expect(browserProof.match(/y: point\.center_y \?\? point\.y/g)).toHaveLength(2);
    expect(browserProof).toContain(
      'await send("selection", { target: id, mode: "toggle" })',
    );
    expect(browserProof).toContain("snap-guide-horizontal");
    expect(browserProof).toContain(
      "entry.exterior_average_red > entry.interior_average_red + 4",
    );
  });

  test("a rubber band normalizes to a positive rectangle in either drag direction", () => {
    expect(marqueeRect([120, 90], [40, 220])).toEqual({ x: 40, y: 90, width: 80, height: 130 });
    expect(marqueeRect([40, 90], [120, 220])).toEqual({ x: 40, y: 90, width: 80, height: 130 });
  });

  test("a press without travel stays a click instead of becoming a rubber band", () => {
    expect(isMarqueeDrag([100, 100], [101, 102])).toBe(false);
    expect(isMarqueeDrag([100, 100], [104, 100])).toBe(true);
    expect(isMarqueeDrag([100, 100], [100, 96])).toBe(true);
  });

  test("union bounds cover every selected node and ignore nodes without bounds", () => {
    const union = unionWorldBounds([
      { min: [10, 20], max: [30, 40] },
      null,
      { min: [-5, 25], max: [12, 90] },
      undefined,
    ]);
    expect(union).toEqual({ min: [-5, 20], max: [30, 90] });
    expect(unionWorldBounds([null, undefined])).toBeNull();
  });

  test("snap guides project onto the axis they constrain", () => {
    const worldToViewport = (point: [number, number]): [number, number] => [point[0] * 2 + 10, point[1] * 2 + 20];
    expect(
      guideSegment({ axis: "vertical", position: 100, start: 0, end: 50, target: "a" }, worldToViewport),
    ).toEqual({ x1: 210, y1: 20, x2: 210, y2: 120 });
    expect(
      guideSegment({ axis: "horizontal", position: 100, start: 0, end: 50, target: "a" }, worldToViewport),
    ).toEqual({ x1: 10, y1: 220, x2: 110, y2: 220 });
  });

  test("alignment needs two nodes and distribution needs three", () => {
    expect(arrangeAvailability(1)).toEqual({ canAlign: false, canDistribute: false });
    expect(arrangeAvailability(2)).toEqual({ canAlign: true, canDistribute: false });
    expect(arrangeAvailability(3)).toEqual({ canAlign: true, canDistribute: true });
  });

  test("pressing a Frame background bands across it, but a selected node is dragged", () => {
    expect(pressStartsMarquee(undefined, false)).toBe(true);
    expect(pressStartsMarquee("frame", false)).toBe(true);
    expect(pressStartsMarquee("frame", true)).toBe(false);
    expect(pressStartsMarquee("rectangle", false)).toBe(false);
    expect(pressStartsMarquee("group", false)).toBe(false);
  });

  test("pressing an already selected node keeps the whole selection so it can be dragged", () => {
    const selection = ["a", "b", "c"];
    expect(selectionIntent("b", selection, false)).toEqual({ mode: "keep", target: "b" });
    expect(selectionIntent("d", selection, false)).toEqual({ mode: "replace", target: "d" });
    expect(selectionIntent("b", selection, true)).toEqual({ mode: "toggle", target: "b" });
    expect(selectionIntent(null, selection, false)).toEqual({ mode: "marquee", target: null });
  });
});

describe("Phase 1B editor wiring", () => {
  test("a drag on empty canvas starts a rubber band and commits it through the engine", () => {
    expect(app).toContain('kind: "marquee"');
    expect(app).toContain('await engine.send("marquee_select"');
    expect(app).toContain('data-testid="marquee"');
  });

  test("moving a selection sends one world delta with the current snapping mode", () => {
    expect(app).toContain('.send("translate_selection", { dx, dy, snap, snap_threshold_px: SNAP_THRESHOLD_PX })');
    expect(app).toContain("const snap = snapEnabled && !event.altKey;");
    expect(app).toContain('data-testid="snap-toggle"');
  });

  test("the editor never computes alignment itself; it asks the engine", () => {
    for (const operation of [
      "align_left",
      "align_horizontal_center",
      "align_right",
      "align_top",
      "align_vertical_center",
      "align_bottom",
      "distribute_horizontal",
      "distribute_vertical",
    ]) {
      expect(app).toContain(`{ id: "${operation}"`);
    }
    // Each button exposes a hyphenated test id derived from its operation.
    expect(app).toContain('data-testid={operation.id.replace(/_/g, "-")}');
    expect(app).toContain('await engine.send("arrange", { operation });');
  });

  test("multiple selection draws every outline plus one union rectangle", () => {
    expect(app).toContain('data-testid="selection-union"');
    expect(app).toContain("selectionOutlines.length > 1");
    expect(styles).toContain(".selection-union");
    expect(styles).toContain(".marquee-rect");
    expect(styles).toContain(".snap-guide");
  });

  test("transform handles stay on a single selection in this phase", () => {
    expect(app).toContain("if (engine.projection.selection.length > 1) return null;");
  });
});

describe("Phase 1B engine protocol", () => {
  test("the host accepts the new selection, marquee, translate, and arrange requests", () => {
    for (const variant of ["MarqueeSelect", "TranslateSelection", "Arrange"]) {
      expect(bridge).toContain(`    ${variant} {`);
    }
    expect(bridge).toContain('"set" | "extend" =>');
  });

  test("a drag is measured from a base captured when the transaction opens", () => {
    expect(bridge).toContain("fn capture_drag_base(&self) -> DragBase");
    expect(bridge).toContain("self.drag_base = Some(self.capture_drag_base());");
    expect(bridge).toContain("translate_selection requires an active transaction");
  });

  test("arranging runs through the runtime planner and one batched transaction", () => {
    expect(bridge).toContain("self.runtime.plan_align(&targets, AlignMode::Left)");
    expect(bridge).toContain("self.runtime.apply_arrange(&plan)?");
    expect(bridge).toContain("fn consume_batch(&mut self, outcome: &BatchOutcome)");
  });
});

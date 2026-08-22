import fs from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

const root = path.resolve(import.meta.dirname, "../../..");
const app = fs.readFileSync(path.join(root, "web/editor/src/App.tsx"), "utf8");
const bridge = fs.readFileSync(path.join(root, "crates/wasm_bridge/src/lib.rs"), "utf8");

describe("Phase 1C direct editing contract", () => {
  test("canvas hit testing and marquee are scoped to the active slide", () => {
    expect(app).toContain('root_id: selectionRoot, selectable_only: true');
    expect(app).toContain("id === selectionRoot");
    expect(bridge).toContain("self.selectable_in_root(*id, root)");
    expect(bridge).toContain("filter(|id| self.selectable_in_root(*id, root))");
  });

  test("eight handles use one aggregate engine transform path", () => {
    expect(app).toContain("RESIZE_HANDLES.map");
    expect(app).toContain("resizeTransform(");
    expect(app).toContain('engine.send("transform_selection"');
    expect(bridge).toContain("TransformSelection {");
    expect(bridge).toContain("apply_batch_in_transaction(commands)");
  });

  test("rotation uses signed pointer delta and 15 degree Shift snapping", () => {
    expect(app).toContain("currentAngle - active.rotationStart");
    expect(app).toContain("snappedRotationDelta");
    expect(app).not.toContain("+ Math.PI / 2");
  });

  test("Inspector exposes mixed values and one atomic batch", () => {
    expect(app).toContain('placeholder={mixed ? "Mixed" : undefined}');
    expect(app).toContain('engine.send("command_batch", { commands })');
    expect(bridge).toContain("CommandBatch {");
    expect(bridge).toContain("apply_transactional_batch(commands)");
  });

  test("Group and Ungroup share their commands with Ctrl or Command shortcuts", () => {
    expect(app).toContain('modifier && event.key.toLowerCase() === "g"');
    expect(app).toContain("if (event.shiftKey) void ungroupSelection()");
    expect(app).toContain("else void groupSelection()");
  });
});

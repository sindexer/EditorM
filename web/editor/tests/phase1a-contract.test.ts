import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

const editorRoot = path.resolve(process.cwd());
const workspace = path.resolve(editorRoot, "../..");
const app = readFileSync(path.join(editorRoot, "src/App.tsx"), "utf8");
const engine = readFileSync(path.join(editorRoot, "src/engine.ts"), "utf8");
const bridge = readFileSync(path.join(workspace, "crates/wasm_bridge/src/lib.rs"), "utf8");
const shader = readFileSync(path.join(workspace, "shared/render_contract.wgsl"), "utf8");
const schema = JSON.parse(readFileSync(path.join(workspace, "shared/render_binary_schema.json"), "utf8"));
const generated = readFileSync(path.join(workspace, "web/phase0d-preview/src/render_contract.js"), "utf8");

describe("Phase 1A visible frame and primitive appearance contract", () => {
  test("Frame tool exposes keyboard shortcut and every required preset", () => {
    expect(app).toContain('{ id: "frame", label: "Frame", shortcut: "F"');
    expect(app).toContain('aria-label="Frame presets"');
    expect(app).toContain("createFramePreset(1920, 1080)");
    expect(app).toContain("createFramePreset(3840, 2160)");
    expect(app).toContain("createFramePreset(4096, 2160)");
    expect(app).toContain('aria-label="Custom frame width"');
    expect(app).toContain('aria-label="Custom frame height"');
    expect(app).toContain('shape: "frame"');
  });

  test("Inspector sends typed appearance commands instead of owning Document state", () => {
    for (const [command, variant] of [["set_fill", "SetFill"], ["set_corner_radii", "SetCornerRadii"], ["set_stroke", "SetStroke"], ["set_opacity", "SetOpacity"]]) {
      expect(app).toContain('kind: "' + command + '"');
      expect(bridge).toContain(variant);
    }
    expect(app).toContain('aria-label="Fill color"');
    expect(app).toContain('aria-label="Stroke color"');
    expect(app).toContain('label="Radius"');
    expect(app).toContain('label="Stroke width"');
    expect(engine).toContain('await this.send("load_fixture", { fixture: "editor" })');
  });

  test("render schema versions appearance independently and declares color contract", () => {
    expect(schema.version).toBe(2);
    expect(schema.instance_stride_bytes).toBe(112);
    expect(schema.dirty_record_stride_bytes).toBe(116);
    expect(schema.alpha_contract).toBe("premultiplied-linear");
    expect(schema.color_input).toBe("srgb");
    expect(schema.stroke_alignment).toBe("center");
    expect(schema.instance_fields.fill_linear).toBe(48);
    expect(schema.instance_fields.corner_radii).toBe(64);
    expect(schema.instance_fields.stroke_linear).toBe(80);
    expect(schema.instance_fields.stroke_width).toBe(96);
  });

  test("shared and generated WGSL use analytic coverage with no fragment discard", () => {
    expect(shader).toContain("fwidth(distance)");
    expect(shader).toContain("smoothstep(-width, width, distance)");
    expect(shader).toContain("ellipse_distance");
    expect(shader).toContain("rounded_rectangle_distance");
    expect(shader).toContain("ellipse_axis_ratio");
    expect(shader).toContain("item.size + vec2<f32>(expansion * 2.0)");
    expect(shader).toContain("premultiplied");
    expect(shader).not.toMatch(/\bdiscard\b/);
    expect(generated).toContain(JSON.stringify(shader));
  });
});

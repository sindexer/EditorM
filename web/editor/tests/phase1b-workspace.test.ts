import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";
import { decideThumbnailQueueUpdate, orderThumbnailQueue } from "../src/engine";
import type { SlideSummary } from "../src/types";

const editorRoot = path.resolve(process.cwd());
const appSource = readFileSync(path.join(editorRoot, "src", "App.tsx"), "utf8");
const engineSource = readFileSync(path.join(editorRoot, "src", "engine.ts"), "utf8");

function slide(id: string, index: number, active = false): SlideSummary {
  return {
    id,
    index,
    active,
    name: `Slide ${index + 1}`,
    width: 1920,
    height: 1080,
    aspect_ratio: 16 / 9,
    thumbnail_revision: 0,
    child_count: 100,
  };
}

describe("Phase 1B multi-slide workspace contract", () => {
  test("thumbnail work is ordered active, visible, then remaining without mutating input", () => {
    const slides = [slide("a", 0), slide("b", 1), slide("c", 2, true), slide("d", 3)];
    const queued = ["d", "a", "b", "c"];
    const ordered = orderThumbnailQueue(queued, slides, new Set(["b"]));
    expect(ordered).toEqual(["c", "b", "a", "d"]);
    expect(queued).toEqual(["d", "a", "b", "c"]);
  });

  test("a queued stale revision is refreshed in place instead of being orphaned", () => {
    expect(decideThumbnailQueueUpdate(undefined, 12, true, false)).toEqual({
      writePending: true,
      enqueue: false,
    });
    expect(decideThumbnailQueueUpdate({ revision: 12, status: "pending" }, 12, false, false)).toEqual({
      writePending: true,
      enqueue: true,
    });
    expect(decideThumbnailQueueUpdate({ revision: 12, status: "ready" }, 12, false, false)).toEqual({
      writePending: false,
      enqueue: false,
    });
  });

  test("workspace exposes the required panels and keyboard-accessible splitters", () => {
    for (const contract of [
      'aria-label="Slides panel"',
      'aria-label="Canvas workspace"',
      'aria-label="Layers panel"',
      'aria-label="Timeline tracks"',
      'title="Inspector"',
      'role="separator"',
      'aria-orientation={orientation}',
      'tabIndex={0}',
    ]) {
      expect(appSource).toContain(contract);
    }
    expect(appSource).toContain('label="Resize Slides panel"');
    expect(appSource).toContain('label="Resize Layers and Timeline panel"');
    expect(appSource).toContain('label="Resize Inspector"');
  });

  test("slide actions use typed Worker requests and default to 1920 by 1080", () => {
    for (const kind of ["activate", "create", "duplicate", "rename", "reorder", "delete"]) {
      expect(appSource).toContain(`kind: "${kind}"`);
    }
    expect(appSource).toContain('width: 1920');
    expect(appSource).toContain('height: 1080');
    expect(appSource).toContain('Alt + ↑/↓ to reorder');
  });

  test("Timeline is an honest structure-only shell synchronized by NodeId and scroll", () => {
    expect(appSource).toContain('Structure only · motion controls are not available in Phase 1B');
    expect(appSource).toContain('data-node-id={id}');
    expect(appSource).toContain('transform: `translateY(${-scrollTop}px)`');
    for (const forbidden of ["playhead", "keyframe", "easing", "frames per second"]) {
      expect(appSource.toLowerCase()).not.toContain(forbidden);
    }
  });

  test("panel preferences stay local and do not dispatch Document commands", () => {
    const start = appSource.indexOf("type WorkspacePreferences");
    const end = appSource.indexOf("const fail", start);
    const preferencesBoundary = appSource.slice(start, end);
    expect(preferencesBoundary).toContain('localStorage.setItem("editorm.phase1b.workspace"');
    expect(preferencesBoundary).not.toContain('engine.send("command"');
    expect(preferencesBoundary).not.toContain('engine.send("slide"');
  });

  test("Inspector routes selection, active creation tool, and active Slide contexts", () => {
    expect(appSource).toContain('data-inspector-context="tool"');
    expect(appSource).toContain('data-inspector-context="slide"');
    expect(appSource).toContain('active Slide');
    expect(appSource).toContain('Centered stroke · sRGB input · linear premultiplied GPU output');
  });

  test("thumbnails are derived from Rust RenderModel slots through bounded WebGPU work", () => {
    expect(engineSource).toContain('this.send("thumbnail_slots"');
    expect(engineSource).toContain('render_source: "rust-render-model-slots"');
    expect(engineSource).toContain('max_renders_per_engine_frame: 1');
    expect(engineSource).toContain('canvas.getContext("webgpu")');
    expect(engineSource).not.toContain('canvas.getContext("2d")');
  });
});

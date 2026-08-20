import { existsSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

const workspace = path.resolve(process.cwd(), "../..");
const proof = JSON.parse(readFileSync(path.join(workspace, "docs/verification/PHASE_1B_BROWSER_PROOF.json"), "utf8"));

describe("Phase 1B targeted actual browser proof integrity", () => {
  test("records a fresh Chrome Worker/WASM/GTX 970 WebGPU run", () => {
    expect(proof.phase).toBe("1B");
    expect(proof.proof_kind).toBe("actual-hardware-browser");
    expect(proof.all_passed).toBe(true);
    expect(Number.isFinite(Date.parse(proof.captured_at_utc))).toBe(true);
    expect(proof.browser).toMatch(/^Chrome\//);
    expect(proof.chrome_process_id).toBeGreaterThan(0);
    expect(proof.chrome_profile).toMatch(/phase1b-chrome-/);
    expect(proof.browser_mode).toContain("no Canvas2D fallback");
    expect(proof.execution.command).toBe("npm run test:browser:phase1b");
    expect(proof.gpu.active_device.deviceString).toBe("NVIDIA GeForce GTX 970");
    expect(proof.initial.worker_runtime_owner).toBe("dedicated-worker");
    expect(proof.initial.wasm_initialized).toBe(true);
    expect(proof.initial.actual_webgpu).toBe(true);
    expect(proof.gpu.pipeline_view_format).toBe(`${proof.gpu.surface_base_format}-srgb`);
    expect(proof.gpu.readback_view_format).toBe(proof.gpu.pipeline_view_format);
  });

  test("passes every multi-slide interaction and isolation assertion", () => {
    expect(Object.values(proof.checks).every(Boolean)).toBe(true);
    expect(proof.assertion_count).toBe(25);
    expect(proof.passed_assertion_count).toBe(proof.assertion_count);
    for (const name of [
      "per_slide_selection_camera_restore",
      "active_slide_hit_isolation",
      "other_slide_selection_blocked",
      "deep_duplicate_independent_ids",
      "rename_reorder_persisted",
      "delete_undo_redo_global_history",
      "isolated_thumbnail_invalidation",
      "layers_timeline_row_sync",
      "contextual_inspector",
    ]) expect(proof.checks[name]).toBe(true);
  });

  test("uses a bounded Rust RenderModel to WebGPU thumbnail queue", () => {
    const thumbnails = proof.final.thumbnails;
    expect(thumbnails.render_source).toBe("rust-render-model-slots");
    expect(thumbnails.priority_policy).toBe("active-visible-remaining");
    expect(thumbnails.max_renders_per_engine_frame).toBe(1);
    expect(thumbnails.canvas2d_fallback_count).toBe(0);
    expect(thumbnails.cached).toBe(3);
    expect(thumbnails.pending).toBe(0);
    expect(thumbnails.errors).toBe(0);
    expect(thumbnails.queue_depth).toBe(0);
  });

  test("has no runtime fallback, console, or GPU validation errors", () => {
    expect(proof.console_errors).toEqual([]);
    expect(proof.max_fallback_rebuild_count_seen).toBe(0);
    expect(proof.final.gpu.validation_errors).toBe(0);
    expect(proof.final.response_gpu_overlay_sequence_match).toBe(true);
  });

  test("records both current UI screenshots with exact byte sizes", () => {
    expect(Object.keys(proof.screenshots)).toHaveLength(2);
    for (const screenshot of Object.values(proof.screenshots) as Array<{ path: string; bytes: number }>) {
      const filename = path.join(workspace, screenshot.path);
      expect(existsSync(filename)).toBe(true);
      expect(statSync(filename).size).toBe(screenshot.bytes);
    }
  });
});

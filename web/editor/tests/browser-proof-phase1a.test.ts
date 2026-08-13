import { existsSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

const editorRoot = path.resolve(process.cwd());
const workspace = path.resolve(editorRoot, "../..");

function readJson(relativePath: string) {
  return JSON.parse(readFileSync(path.join(workspace, relativePath), "utf8"));
}

const browser = readJson("docs/verification/PHASE_1A_BROWSER_PROOF.json");
const pixels = readJson("docs/verification/PHASE_1A_PIXEL_READBACK.json");
const metrics = readJson("docs/PHASE_1A_METRICS.json");

describe("Phase 1A stored proof integrity", () => {
  test("browser proof records a fresh real Chrome Worker/WASM/WebGPU run", () => {
    expect(browser.phase).toBe("1A");
    expect(browser.all_passed).toBe(true);
    expect(Number.isFinite(Date.parse(browser.captured_at_utc))).toBe(true);
    expect(browser.browser).toMatch(/^Chrome\//);
    expect(browser.chrome_process_id).toBeGreaterThan(0);
    expect(browser.chrome_profile).toMatch(/phase1a-chrome-/);
    expect(browser.browser_mode).toMatch(/no mock; no Canvas2D fallback/);
    expect(browser.execution.command).toBe("npm run test:browser:phase1a");
    expect(Date.parse(browser.execution.started_at_utc)).toBeLessThanOrEqual(
      Date.parse(browser.execution.finished_at_utc),
    );
    expect(browser.gpu.active_device.deviceString).toBe("NVIDIA GeForce GTX 970");
    expect(browser.initial.worker_runtime_owner).toBe("dedicated-worker");
    expect(browser.initial.wasm_initialized).toBe(true);
    expect(browser.initial.actual_webgpu).toBe(true);
    expect(Object.values(browser.checks).every(Boolean)).toBe(true);
    expect(browser.console_errors).toEqual([]);
    expect(browser.max_fallback_rebuild_count_seen).toBe(0);
  });

  test("every screenshot in the browser proof exists with the recorded byte size", () => {
    expect(Object.keys(browser.screenshots)).toHaveLength(4);
    for (const screenshot of Object.values(browser.screenshots) as Array<{ path: string; bytes: number }>) {
      const screenshotPath = path.join(workspace, screenshot.path);
      expect(existsSync(screenshotPath)).toBe(true);
      expect(statSync(screenshotPath).size).toBe(screenshot.bytes);
    }
  });

  test("pixel proof covers every required DPR and zoom with partial analytic coverage", () => {
    const requiredMatrix = [1, 1.25, 1.5, 2].flatMap((dpr) =>
      [0.25, 1, 4].map((zoom) => `ellipse-dpr-${dpr}-zoom-${zoom}`),
    );
    const byLabel = new Map<string, { partial_coverage_present: boolean }>(
      pixels.cases.map((sample: { label: string; partial_coverage_present: boolean }) => [
        sample.label,
        sample,
      ]),
    );
    expect(pixels.all_passed).toBe(true);
    expect(pixels.cases).toHaveLength(17);
    for (const label of requiredMatrix) {
      expect(byLabel.get(label)?.partial_coverage_present).toBe(true);
    }
    for (const label of [
      "rotated-nonuniform-ellipse",
      "circle",
      "opacity-fill-stroke-black-background",
      "opacity-fill-stroke-white-background",
      "rounded-rectangle",
    ]) {
      expect(byLabel.get(label)?.partial_coverage_present).toBe(true);
    }
    expect(Object.values(pixels.assertions).every(Boolean)).toBe(true);
  });

  test("native GTX 970 performance proof preserves raw samples and required threshold", () => {
    const required = metrics.required_1000_visible;
    const comparative = metrics.comparative_10000_visible;
    expect(metrics.all_passed).toBe(true);
    expect(metrics.proof_kind).toBe("actual-native-wgpu-performance");
    expect(metrics.renderer.adapter).toBe("NVIDIA GeForce GTX 970");
    expect(required.viewport).toEqual([1920, 1080]);
    expect(required.dpr).toBe(1);
    expect(required.visible_instances).toBe(1000);
    expect(required.warmup_frames).toBe(30);
    expect(required.measured_frames).toBe(300);
    expect(required.raw_frame_ms).toHaveLength(300);
    expect(required.p95_ms).toBeLessThanOrEqual(metrics.threshold_ms);
    expect(required.batches).toBe(1);
    expect(required.draw_calls).toBe(1);
    expect(comparative.visible_instances).toBe(10000);
    expect(comparative.measured_frames).toBe(60);
    expect(comparative.raw_frame_ms).toHaveLength(60);
  });
});
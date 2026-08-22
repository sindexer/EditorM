import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

const editorRoot = path.resolve(process.cwd());
const workspace = path.resolve(editorRoot, "../..");

function readJson(relativePath: string) {
  return JSON.parse(readFileSync(path.join(workspace, relativePath), "utf8"));
}

const proofPath = "docs/verification/PHASE_1B_BROWSER_PROOF.json";
const proofExists = existsSync(path.join(workspace, proofPath));

// This file validates a produced Phase 1B browser proof. It is intentionally NOT part of the
// default `npm test` suite: it runs from `npm run test:browser:phase1b`, immediately after the
// harness executes on real hardware. `tests/phase1b-gate-status.test.ts` is the guard that runs
// in the default suite and refuses to let the repository claim a proof it does not have.
describe("Phase 1B browser proof", () => {
  test("a proof file exists to validate", () => {
    expect(
      proofExists,
      `${proofPath} is missing. Run: npm run test:browser:phase1b on a machine with a real GPU.`,
    ).toBe(true);
  });

  test.runIf(proofExists)("records a fresh actual Chrome/Worker/WASM/WebGPU run", () => {
    const proof = readJson(proofPath);
    expect(proof.phase).toBe("1B");
    expect(proof.proof_kind).toBe("actual-hardware-browser");
    expect(proof.browser).toMatch(/^Chrome\//);
    expect(proof.chrome_process_id).toBeGreaterThan(0);
    expect(proof.chrome_profile).toMatch(/phase1b-chrome-/);
    expect(proof.browser_mode).toMatch(/no mock; no Canvas2D fallback/);
    expect(proof.execution.command).toBe("npm run test:browser:phase1b");
    expect(Date.parse(proof.execution.started_at_utc)).toBeLessThanOrEqual(
      Date.parse(proof.execution.finished_at_utc),
    );
    expect(proof.initial.worker_runtime_owner).toBe("dedicated-worker");
    expect(proof.initial.actual_webgpu).toBe(true);
    expect(proof.gpu.software_renderer).toBe(false);
    expect(proof.execution.software_gpu_allowed).toBe(false);
    expect(proof.console_errors).toEqual([]);
  });

  test.runIf(proofExists)("covers every Phase 1B behaviour the gate requires", () => {
    const proof = readJson(proofPath);
    const required = [
      "shift_click_selects_two",
      "multiple_selection_draws_every_outline",
      "marquee_selects_intersecting_nodes",
      "select_all_selects_every_top_level_node",
      "multi_drag_moves_every_node_by_one_delta",
      "multi_drag_leaves_unselected_nodes_alone",
      "multi_drag_undo_restores_every_node",
      "coalesced_drag_has_no_drift",
      "coalesced_drag_coalesced_requests",
      "snapping_aligns_edges_exactly",
      "snap_guide_reported_and_rendered",
      "alt_suspends_snapping",
      "snap_toggle_off_suspends_snapping",
      "align_left_aligned",
      "align_left_undo_restored",
      "align_horizontal_center_aligned",
      "align_right_aligned",
      "align_top_aligned",
      "align_vertical_center_aligned",
      "align_bottom_aligned",
      "distribute_horizontal_equal_gaps",
      "distribute_horizontal_undo_restored",
      "distribute_vertical_equal_gaps",
      "distribute_vertical_undo_restored",
      "benchmark_all_passed",
      "actual_pixel_readback",
      "gpu_validation_errors_zero",
      "fallback_rebuild_zero",
    ];
    for (const name of required) {
      expect(Object.keys(proof.checks), `missing check ${name}`).toContain(name);
      expect(proof.checks[name], `failed check ${name}`).toBe(true);
    }
    expect(proof.passed_assertion_count).toBe(proof.assertion_count);
    expect(proof.all_passed).toBe(true);
  });

  test.runIf(proofExists)("carries pixel evidence for every interaction overlay", () => {
    const pixels = readJson("docs/verification/PHASE_1B_PIXEL_READBACK.json");
    expect(pixels.phase).toBe("1B");
    for (const name of [
      "selection_outline_rendered",
      "selection_union_rendered",
      "union_absent_for_single_selection",
      "marquee_band_rendered",
      "snap_guide_rendered",
      "dragged_rectangle_moved_on_webgpu_surface",
    ]) {
      expect(Object.keys(pixels.assertions), `missing pixel assertion ${name}`).toContain(name);
      expect(pixels.assertions[name], `failed pixel assertion ${name}`).toBe(true);
    }
    expect(pixels.all_passed).toBe(true);
  });

  test.runIf(proofExists)("records the multi-selection drag benchmark", () => {
    const metrics = readJson("docs/PHASE_1B_METRICS.json");
    expect(metrics.phase).toBe("1B");
    for (const objects of [10, 100, 1000]) {
      const series = metrics.series.filter(
        (entry: { objects: number }) => entry.objects === objects,
      );
      expect(series.length, `missing benchmark series for ${objects} objects`).toBe(2);
      for (const entry of series) {
        expect(entry.raw_frame_ms.length).toBe(entry.measured_iterations);
        expect(entry.moved_nodes).toBe(objects);
      }
    }
    expect(metrics.all_passed).toBe(true);
  });
});

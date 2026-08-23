import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

const editorRoot = path.resolve(process.cwd());
const workspace = path.resolve(editorRoot, "../..");

function readJson(relativePath: string) {
  return JSON.parse(readFileSync(path.join(workspace, relativePath), "utf8"));
}

const proofPath = "docs/verification/PHASE_2A_BROWSER_PROOF.json";
const proofExists = existsSync(path.join(workspace, proofPath));

// Validates a produced Phase 2A browser proof. Like the Phase 1B equivalent it is not part of
// the default `npm test` suite: it runs from `npm run test:browser:phase2a:path-render`,
// immediately after the harness executes on real hardware. `tests/phase2a-path-contract.test.ts`
// runs in the default suite and refuses to let the repository claim a proof it does not have.
describe("Phase 2A browser proof", () => {
  test("a proof file exists to validate", () => {
    expect(
      proofExists,
      `${proofPath} is missing. Run: npm run test:browser:phase2a:path-render on a machine with a real GPU.`,
    ).toBe(true);
  });

  test.runIf(proofExists)("records a fresh actual Chrome/Worker/WASM/WebGPU run", () => {
    const proof = readJson(proofPath);
    expect(proof.phase).toBe("2A");
    expect(proof.proof_kind).toBe("actual-hardware-browser");
    expect(proof.browser).toMatch(/^Chrome\//);
    expect(proof.chrome_process_id).toBeGreaterThan(0);
    expect(proof.chrome_profile).toMatch(/phase2a-chrome-/);
    expect(proof.browser_mode).toMatch(/no mock; no Canvas2D fallback/);
    expect(proof.execution.command).toBe("npm run test:browser:phase2a:path-render");
    expect(proof.initial.worker_runtime_owner).toBe("dedicated-worker");
    expect(proof.initial.wasm_initialized).toBe(true);
    expect(proof.initial.actual_webgpu).toBe(true);
    expect(proof.gpu.software_renderer).toBe(false);
    expect(proof.execution.software_gpu_allowed).toBe(false);
    expect(proof.console_errors).toEqual([]);
  });

  test.runIf(proofExists)("covers every Phase 2A path behaviour", () => {
    const proof = readJson(proofPath);
    for (const name of [
      "render_binary_schema_v3",
      "fixture_projects_four_paths",
      "paths_reach_the_gpu_as_triangles",
      "ordered_path_draw_batches",
      "actual_pixel_readback",
      "filled_interior_is_pickable",
      "outline_only_region_is_not_pickable",
      "stroke_is_pickable",
      "create_path_command_round_trip",
      "edit_path_tessellates_only_that_path",
      "undo_restores_path_geometry",
      "redo_reapplies_path_geometry",
      "undo_removes_the_created_path",
      "camera_only_uploads_no_triangles",
      "gpu_validation_errors_zero",
      "fallback_rebuild_zero",
    ]) {
      expect(Object.keys(proof.checks), `missing check ${name}`).toContain(name);
      expect(proof.checks[name], `failed check ${name}`).toBe(true);
    }
    expect(proof.passed_assertion_count).toBe(proof.assertion_count);
    expect(proof.all_passed).toBe(true);
  });

  test.runIf(proofExists)("carries WebGPU surface pixel evidence for every path kind", () => {
    const pixels = readJson("docs/verification/PHASE_2A_PIXEL_READBACK.json");
    expect(pixels.phase).toBe("2A");
    expect(pixels.proof_kind).toBe("actual-hardware-webgpu-surface-readback");
    for (const name of [
      "straight_path_visible",
      "straight_path_background_control",
      "bezier_path_visible_off_its_chord",
      "bezier_chord_is_empty",
      "closed_fill_interior_filled",
      "closed_fill_respects_outline",
      "closed_curve_stroke_visible",
      "closed_curve_fill_visible",
      "background_control_captured",
    ]) {
      expect(Object.keys(pixels.assertions), `missing pixel assertion ${name}`).toContain(name);
      expect(pixels.assertions[name], `failed pixel assertion ${name}`).toBe(true);
    }
    expect(pixels.all_passed).toBe(true);
  });

  test.runIf(proofExists)("records the actual-GPU frame behaviour for paths", () => {
    const metrics = readJson("docs/PHASE_2A_BROWSER_METRICS.json");
    expect(metrics.phase).toBe("2A");
    expect(metrics.fixture).toBe("PATH-A");
    expect(metrics.path_instance_count).toBe(4);
    expect(metrics.path_vertex_count).toBeGreaterThan(0);
    expect(metrics.single_path_edit.path_tessellations).toBe(1);
    expect(metrics.camera_only_frame.path_tessellations).toBe(0);
    expect(metrics.camera_only_frame.path_vertices_uploaded).toBe(0);
    expect(metrics.camera_only_frame.path_vertices_sent).toBe(false);
  });
});

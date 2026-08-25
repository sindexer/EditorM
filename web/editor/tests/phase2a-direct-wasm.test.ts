import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

const editorRoot = path.resolve(process.cwd());
const workspace = path.resolve(editorRoot, "../..");
const proofRelativePath = "docs/verification/PHASE_2A_DIRECT_WASM_PROOF.json";
const proofExists = existsSync(path.join(workspace, proofRelativePath));

const requiredChecks = [
  "fixture_is_path_a",
  "schema_version_3",
  "fixture_has_four_paths",
  "fixture_tessellates_to_triangles",
  "fixture_sends_path_buffers",
  "draw_batches_cover_every_path_vertex",
  "fixture_projects_four_path_nodes",
  "camera_only_sends_no_triangles",
  "camera_only_uploads_zero_vertices",
  "camera_only_retessellates_nothing",
  "create_path_tessellates_once",
  "edit_retessellates_exactly_one_path",
  "edit_causes_no_full_render_rebuild",
  "edit_causes_no_fallback_rebuild",
  "undo_restores_path_geometry",
  "undo_restores_triangle_count",
  "redo_reapplies_path_geometry",
  "undo_removes_the_created_path",
  "create_path_from_pen",
  "pen_create_tessellates_once",
  "set_path_from_pen",
  "pen_commit_transaction",
  "pen_path_is_projected_after_commit",
  "undo_removes_whole_pen_transaction",
  "closed_path_needs_three_anchors",
  "duplicate_anchor_identity_rejected",
  "anchor_id_must_be_a_uuid",
  "open_path_needs_two_anchors",
];

// Phase 2A owns its own direct-WASM artifact. The Gate 1B proof is historical and is never
// regenerated to satisfy this phase.
describe("Phase 2A stored direct-WASM proof integrity", () => {
  test("a Phase 2A direct-WASM proof exists", () => {
    expect(
      proofExists,
      `${proofRelativePath} is missing. Run: npm run test:direct-wasm:phase2a`,
    ).toBe(true);
  });

  test.runIf(proofExists)("records an executed run against the WASM this repository ships", () => {
    const proof = JSON.parse(readFileSync(path.join(workspace, proofRelativePath), "utf8"));
    expect(proof.phase).toBe("2A");
    expect(proof.proof_kind).toBe("actual-wasm-direct-node");
    expect(proof.protocol_version).toBe(1);
    expect(proof.render_binary_schema_version).toBe(3);
    expect(Date.parse(proof.started_at)).toBeLessThanOrEqual(Date.parse(proof.finished_at));
    expect(proof.engine_sequence).toBeGreaterThan(0);

    const wasm = readFileSync(path.join(workspace, proof.wasm_path));
    expect(wasm.length).toBe(proof.wasm_bytes);
    expect(createHash("sha256").update(wasm).digest("hex")).toBe(proof.wasm_sha256);
  });

  test.runIf(proofExists)("covers every Phase 2A path behaviour and passed", () => {
    const proof = JSON.parse(readFileSync(path.join(workspace, proofRelativePath), "utf8"));
    const names = new Set(proof.checks.map((check: { name: string }) => check.name));
    for (const required of requiredChecks) {
      expect(names.has(required), `missing check ${required}`).toBe(true);
    }
    expect(proof.checks_total).toBe(proof.checks.length);
    expect(proof.checks_passed).toBe(proof.checks_total);
    expect(proof.all_passed).toBe(true);
  });

  test.runIf(proofExists)("does not claim GPU or browser evidence", () => {
    const proof = JSON.parse(readFileSync(path.join(workspace, proofRelativePath), "utf8"));
    expect(proof.gpu_evidence).toBe(false);
    expect(proof.browser_evidence).toBe(false);
    expect(proof.fixture.path_vertex_count % 3).toBe(0);
  });
});

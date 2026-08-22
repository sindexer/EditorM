import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

const editorRoot = path.resolve(process.cwd());
const workspace = path.resolve(editorRoot, "../..");
const proof = JSON.parse(
  readFileSync(path.join(workspace, "docs/verification/PHASE_1B_DIRECT_WASM_PROOF.json"), "utf8"),
);

const requiredChecks = [
  "selection_set_count",
  "selection_set_rejects_unknown_id",
  "selection_survives_rejected_request",
  "marquee_selects_intersecting_top_level_nodes",
  "drag_base_captured",
  "translation_snapped",
  "snap_corrected_applied_delta",
  "snap_guide_reported",
  "snapped_node_left_edge_matches_first",
  "repeated_frames_measure_from_base",
  "align_moved_two_nodes",
  "align_is_one_history_entry",
  "aligned_left_edges_match",
  "undo_restores_every_target",
  "distribute_equalized_gaps",
  "translation_without_transaction_is_typed",
  "unknown_arrange_is_typed",
  "distribute_requires_three",
  "no_transaction_left_open",
];

describe("Phase 1B stored direct-WASM proof integrity", () => {
  test("the proof records an executed run rather than an assertion of intent", () => {
    expect(proof.phase).toBe("1B");
    expect(proof.proof_kind).toBe("actual-wasm-direct-node");
    expect(proof.all_passed).toBe(true);
    expect(Number.isFinite(Date.parse(proof.started_at))).toBe(true);
    expect(Date.parse(proof.started_at)).toBeLessThanOrEqual(Date.parse(proof.finished_at));
    expect(proof.node_version).toMatch(/^v\d+\./);
    expect(proof.engine_sequence).toBeGreaterThan(0);
  });

  test("the proof was produced by the WASM package this repository ships", () => {
    const wasm = readFileSync(path.join(workspace, proof.wasm_path));
    expect(wasm.length).toBe(proof.wasm_bytes);
    expect(createHash("sha256").update(wasm).digest("hex")).toBe(proof.wasm_sha256);
    expect(proof.protocol_version).toBe(1);
    expect(proof.render_binary_schema_version).toBe(2);
  });

  test("every Phase 1B behaviour is covered and passed", () => {
    const names = new Set(proof.checks.map((check: { name: string }) => check.name));
    for (const required of requiredChecks) {
      expect(names.has(required), `missing check ${required}`).toBe(true);
    }
    expect(proof.checks_total).toBe(proof.checks.length);
    expect(proof.checks_passed).toBe(proof.checks_total);
    expect(proof.checks.every((check: { passed: boolean }) => check.passed)).toBe(true);
  });

  test("this proof does not claim GPU or browser evidence", () => {
    expect(proof.gpu_evidence).toBe(false);
  });
});

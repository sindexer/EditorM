import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

const editorRoot = path.resolve(process.cwd());
const workspace = path.resolve(editorRoot, "../..");
const proofRelativePath = "docs/verification/PHASE_1B_DIRECT_WASM_PROOF.json";
// SHA-256 of the artifact as approved on main at 016622d. It is pinned here so that a stray
// regeneration fails the default suite instead of quietly replacing Gate 1B evidence.
const GATE_1B_PROOF_SHA256 = "fe8f32cfdf032f57c784ededf582b4a766f5feb6132c522536220812e86ec733";
const proof = JSON.parse(readFileSync(path.join(workspace, proofRelativePath), "utf8"));

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

  // The Phase 1B proof is a historical artifact of the Gate 1B run on commit
  // 330476b5. It records the WASM package that existed at that commit, and later phases rebuild
  // that package, so it must NOT be re-pinned to whatever the working tree ships today. What is
  // guarded here is that the artifact names the exact build it tested. The claim that the WASM
  // this repository ships right now still works lives in the Phase 2A direct-WASM proof.
  test("the proof names the exact WASM build and gate run it tested", () => {
    expect(proof.wasm_path).toBe("web/editor/public/pkg/engine_host_bg.wasm");
    expect(proof.wasm_bytes).toBeGreaterThan(0);
    expect(proof.wasm_sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(proof.tested_source_commit).toMatch(/^[0-9a-f]{40}$/);
    expect(proof.tested_branch).toBe("main");
    expect(proof.gate_run_id).toMatch(/^phase1b-/);
    expect(proof.protocol_version).toBe(1);
    // Schema 2 was the shipped contract at Gate 1B. Phase 2A moved the product to schema 3
    // without rewriting this record.
    expect(proof.render_binary_schema_version).toBe(2);
  });

  test("the stored Gate 1B artifact is byte-identical to the approved run", () => {
    // Any command that regenerates this file has changed history rather than adding evidence.
    const digest = createHash("sha256")
      .update(readFileSync(path.join(workspace, proofRelativePath)))
      .digest("hex");
    expect(digest).toBe(GATE_1B_PROOF_SHA256);
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

const benchmark = JSON.parse(
  readFileSync(path.join(workspace, "docs/PHASE_1B_METRICS_ENGINE_ONLY.json"), "utf8"),
);

describe("Phase 1B engine-only multi-drag benchmark", () => {
  test("measures 10, 100, and 1,000 simultaneously dragged objects, snapped and unsnapped", () => {
    for (const objects of [10, 100, 1000]) {
      const series = benchmark.series.filter(
        (entry: { objects: number }) => entry.objects === objects,
      );
      expect(series.length, `missing series for ${objects} objects`).toBe(2);
      for (const entry of series) {
        expect(entry.raw_frame_ms.length).toBe(entry.measured_iterations);
        expect(entry.moved_nodes).toBe(objects);
        expect(entry.dirty_slots).toBe(objects);
      }
    }
  });

  test("keeps drag work bounded: no full rebuilds, clones, or model scans", () => {
    expect(benchmark.checks.no_full_rebuilds).toBe(true);
    expect(benchmark.checks.no_document_clones).toBe(true);
    expect(benchmark.checks.dirty_slots_match_selection_size).toBe(true);
    expect(benchmark.all_passed).toBe(true);
  });

  test("does not claim browser or GPU evidence", () => {
    expect(benchmark.gpu_evidence).toBe(false);
    expect(benchmark.browser_evidence).toBe(false);
  });
});

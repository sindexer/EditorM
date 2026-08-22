import { describe, expect, test } from "vitest";

import {
  benchmarkErrors,
  calculateConclusion,
  calculateSummary,
  isAllowedEvidencePath,
  validateBinding,
} from "../scripts/finalize-gate-phase1b.mjs";

function item(status: "PASS" | "FAIL" | "UNVERIFIED", required = true) {
  return { id: `${status}-${required}`, status, required };
}

function metrics(p95For100 = 10, p95For1000 = 500) {
  const series = [];
  for (const objects of [10, 100, 1000]) {
    for (const snapping of [false, true]) {
      const measuredIterations = 2;
      series.push({
        objects,
        snapping,
        p95_ms: objects === 100 ? p95For100 : objects === 1000 ? p95For1000 : 5,
        measured_iterations: measuredIterations,
        raw_frame_ms: [1, 2],
        dirty_slots: objects,
      });
    }
  }
  return { phase: "1B", all_passed: true, frame_budget_ms: 16.7, series };
}

describe("Phase 1B gate finalizer rules", () => {
  test("summary is derived only from item statuses", () => {
    expect(calculateSummary([item("PASS"), item("PASS"), item("FAIL"), item("UNVERIFIED")])).toEqual({
      pass: 2,
      fail: 1,
      unverified: 1,
    });
  });

  test("PASSED requires every required item to pass", () => {
    expect(calculateConclusion([item("PASS"), item("PASS")])).toBe("PASSED");
    expect(calculateConclusion([item("PASS"), item("UNVERIFIED")])).toBe("NOT PASSED");
    expect(calculateConclusion([item("PASS"), item("FAIL")])).toBe("NOT PASSED");
  });

  test("evidence binding rejects a different run, source commit, or branch", () => {
    const context = { gateRunId: "run-1", sourceCommit: "abc", branch: "branch" };
    const artifact = {
      gate_run_id: "run-1",
      tested_source_commit: "abc",
      tested_branch: "branch",
    };
    expect(validateBinding(artifact, context)).toBe(true);
    expect(validateBinding({ ...artifact, gate_run_id: "run-2" }, context)).toBe(false);
    expect(validateBinding({ ...artifact, tested_source_commit: "def" }, context)).toBe(false);
    expect(validateBinding({ ...artifact, tested_branch: "other" }, context)).toBe(false);
  });

  test("keeps the existing 10/100 threshold and gives 1,000 no new threshold", () => {
    expect(benchmarkErrors(metrics())).toEqual([]);
    expect(benchmarkErrors(metrics(20))).toContain(
      "100-object p95 exceeds the existing 16.7 ms Gate threshold.",
    );
  });

  test("only verification and Gate-review paths may follow a tested commit", () => {
    expect(isAllowedEvidencePath("docs/verification/PHASE_1B_BROWSER_PROOF.json")).toBe(true);
    expect(isAllowedEvidencePath("docs/PHASE_1B_METRICS.json")).toBe(true);
    expect(isAllowedEvidencePath("docs/REVIEW_PACKET_1B.md")).toBe(true);
    expect(isAllowedEvidencePath("web/editor/scripts/browser-proof-phase1b.mjs")).toBe(false);
    expect(isAllowedEvidencePath("crates/runtime/src/lib.rs")).toBe(false);
  });
});

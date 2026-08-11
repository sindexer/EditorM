import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

describe("Phase 0E-R3 browser proof artifact", () => {
  test("records a fresh all-passed actual hardware R3 run", () => {
    const proofPath = path.resolve(process.cwd(), "../../docs/verification/PHASE_0E_R3_BROWSER_PROOF.json");
    const proof = JSON.parse(readFileSync(proofPath, "utf8"));
    expect(proof.phase).toBe("0E-R3");
    expect(proof.proof_kind).toBe("actual-hardware-browser");
    expect(proof.checks.r3_complexity_matrix).toBe(true);
    expect(proof.r3_complexity_matrix.matrix).toHaveLength(70);
    expect(proof.checks.r2_structural_10k_100k_bounded).toBe(true);
    expect(proof.all_passed).toBe(true);
    expect(Object.values(proof.checks)).not.toContain(false);
  });
});

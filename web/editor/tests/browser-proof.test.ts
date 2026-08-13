import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

describe("Phase 0E-R1 browser proof artifact", () => {
  test("records a fresh all-passed actual hardware run", () => {
    const proofPath = path.resolve(
      process.cwd(),
      "../../docs/verification/PHASE_0E_R1_BROWSER_PROOF.json",
    );
    const proof = JSON.parse(readFileSync(proofPath, "utf8"));
    expect(proof.phase).toBe("0E-R1");
    expect(proof.proof_kind).toBe("actual-hardware-browser");
    expect(proof.all_passed).toBe(true);
    expect(Object.values(proof.checks)).not.toContain(false);
  });
});

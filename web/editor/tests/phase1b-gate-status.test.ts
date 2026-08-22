import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

// Gate 1B honesty guard. It runs in the default suite and enforces one rule: the repository may
// only claim a browser/WebGPU proof when a passing hardware proof artifact is actually present.
// If no proof exists, an UNVERIFIED status artifact must exist instead, and the review packet
// must say so. This makes a false PASS a failing test rather than a documentation choice.

const editorRoot = path.resolve(process.cwd());
const workspace = path.resolve(editorRoot, "../..");
const proofPath = path.join(workspace, "docs/verification/PHASE_1B_BROWSER_PROOF.json");
const statusPath = path.join(workspace, "docs/verification/PHASE_1B_GATE_STATUS.json");

function readJson(file: string) {
  return JSON.parse(readFileSync(file, "utf8"));
}

const proofExists = existsSync(proofPath);

describe("Phase 1B gate status honesty", () => {
  test("a gate status artifact always records where the gate stands", () => {
    expect(existsSync(statusPath), "docs/verification/PHASE_1B_GATE_STATUS.json is missing").toBe(
      true,
    );
    const status = readJson(statusPath);
    expect(status.phase).toBe("1B");
    expect(Array.isArray(status.items)).toBe(true);
    expect(status.items.length).toBeGreaterThan(0);
    for (const item of status.items) {
      expect(["PASS", "FAIL", "UNVERIFIED"]).toContain(item.status);
      expect(typeof item.id).toBe("string");
      expect(typeof item.evidence).toBe("string");
    }
  });

  test("every PASS item names an evidence artifact that exists", () => {
    const status = readJson(statusPath);
    for (const item of status.items.filter(
      (entry: { status: string }) => entry.status === "PASS",
    )) {
      for (const artifact of item.artifacts ?? []) {
        expect(
          existsSync(path.join(workspace, artifact)),
          `${item.id} claims PASS but ${artifact} does not exist`,
        ).toBe(true);
      }
    }
  });

  test("browser and pixel items stay UNVERIFIED until a passing hardware proof exists", () => {
    const status = readJson(statusPath);
    const browserItems = status.items.filter(
      (entry: { requires_gpu?: boolean }) => entry.requires_gpu === true,
    );
    expect(browserItems.length).toBeGreaterThan(0);
    if (!proofExists) {
      expect(status.browser_proof_present).toBe(false);
      for (const item of browserItems) {
        expect(
          item.status,
          `${item.id} claims ${item.status} without a browser proof artifact`,
        ).not.toBe("PASS");
      }
      return;
    }
    const proof = readJson(proofPath);
    expect(status.browser_proof_present).toBe(true);
    expect(proof.all_passed, "a stored browser proof must be a passing run").toBe(true);
    expect(proof.gpu.software_renderer, "a gate proof must not be a software renderer").toBe(
      false,
    );
  });
});

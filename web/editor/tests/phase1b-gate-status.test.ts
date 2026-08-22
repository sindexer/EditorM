import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

// Gate 1B honesty guard. It runs in the default suite and verifies that the generated status is
// internally consistent and that any accepted browser proof is real, passing hardware evidence
// from the same Gate run and tested source commit.

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
      expect(typeof item.required).toBe("boolean");
    }
  });

  test("summary is calculated exactly from items", () => {
    const status = readJson(statusPath);
    const calculated = { pass: 0, fail: 0, unverified: 0 };
    for (const item of status.items) {
      calculated[item.status.toLowerCase() as keyof typeof calculated] += 1;
    }
    expect(status.summary).toEqual(calculated);
  });

  test("gate conclusion agrees with every required item", () => {
    const status = readJson(statusPath);
    const required = status.items.filter(
      (item: { required?: boolean }) => item.required !== false,
    );
    const allRequiredPass =
      required.length > 0 &&
      required.every((item: { status: string }) => item.status === "PASS");
    expect(status.gate_conclusion).toBe(allRequiredPass ? "PASSED" : "NOT PASSED");
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
    expect(proof.proof_kind).toBe("actual-hardware-browser");
    expect(proof.browser).toMatch(/^Chrome\//);
    expect(proof.initial.worker_runtime_owner).toBe("dedicated-worker");
    expect(proof.initial.wasm_initialized).toBe(true);
    expect(proof.initial.actual_webgpu).toBe(true);
    expect(proof.gpu.software_renderer, "a gate proof must not be a software renderer").toBe(
      false,
    );
    expect(proof.gate_run_id).toBe(status.gate_run_id);
    expect(proof.tested_source_commit).toBe(status.tested_source_commit);
    expect(proof.tested_branch).toBe(status.tested_branch);

    for (const relativePath of [
      "docs/verification/PHASE_1B_PIXEL_READBACK.json",
      "docs/PHASE_1B_METRICS.json",
    ]) {
      const artifact = readJson(path.join(workspace, relativePath));
      expect(artifact.gate_run_id).toBe(proof.gate_run_id);
      expect(artifact.tested_source_commit).toBe(proof.tested_source_commit);
      expect(artifact.tested_branch).toBe(proof.tested_branch);
    }
  });
});

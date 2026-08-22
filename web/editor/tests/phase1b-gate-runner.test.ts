import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

const workspace = path.resolve(process.cwd(), "../..");
const runner = readFileSync(path.join(workspace, "tools/run-phase1b-gate.ps1"), "utf8");

describe("Phase 1B Windows gate runner contract", () => {
  test("binds every child to one Gate run and source", () => {
    expect(runner).toContain("PHASE1B_GATE_RUN_ID");
    expect(runner).toContain("PHASE1B_GATE_SOURCE_COMMIT");
    expect(runner).toContain("PHASE1B_GATE_SOURCE_BRANCH");
    expect(runner).toContain("PHASE_1B_GATE_RUN.json");
  });

  test("enters web/editor exactly once", () => {
    expect(runner.match(/Set-Location \$editorRoot/g)).toHaveLength(1);
    expect(runner).not.toMatch(/cd\s+web[\\/]editor/i);
  });

  test("keeps the fail-closed Gate step order", () => {
    const requiredOrder = [
      'Id "cargo_fmt"',
      'Id "cargo_clippy"',
      'Id "cargo_build"',
      'Id "cargo_test"',
      'Id "wasm_build"',
      'Id "npm_ci"',
      'Id "phase1a_browser"',
      'Id "phase1b_browser"',
      'Id "direct_wasm"',
      'Id "engine_benchmark"',
      'Id "editor_build"',
      'Id "gate_finalize"',
      'Id "default_tests"',
    ];
    let previous = -1;
    for (const marker of requiredOrder) {
      const current = runner.indexOf(marker);
      expect(current, `missing runner step ${marker}`).toBeGreaterThan(previous);
      previous = current;
    }
  });

  test("finalizes before the default Gate guard and again on failure", () => {
    expect(runner.indexOf('Id "gate_finalize"')).toBeLessThan(
      runner.indexOf('Id "default_tests"'),
    );
    expect(runner).toContain("Invoke-FinalizerBestEffort");
    expect(runner).toContain('gate_conclusion = "NOT PASSED"');
  });
});

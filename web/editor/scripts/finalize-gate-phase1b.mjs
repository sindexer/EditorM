import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptPath = fileURLToPath(import.meta.url);
const scriptRoot = path.dirname(scriptPath);
const defaultWorkspace = path.resolve(scriptRoot, "../../..");
export const EXPECTED_BRANCH = "main";
const VALID_STATUSES = new Set(["PASS", "FAIL", "UNVERIFIED"]);

const RELATIVE_PATHS = {
  manifest: "docs/verification/PHASE_1B_GATE_RUN.json",
  status: "docs/verification/PHASE_1B_GATE_STATUS.json",
  phase1aRecord: "docs/verification/PHASE_1B_GATE_PHASE1A_RERUN.json",
  phase1aProof: "docs/verification/PHASE_1A_BROWSER_PROOF.json",
  phase1aFailure: "docs/verification/PHASE_1A_BROWSER_FAILURE.json",
  browserProof: "docs/verification/PHASE_1B_BROWSER_PROOF.json",
  browserFailure: "docs/verification/PHASE_1B_BROWSER_FAILURE.json",
  pixels: "docs/verification/PHASE_1B_PIXEL_READBACK.json",
  browserMetrics: "docs/PHASE_1B_METRICS.json",
  directWasm: "docs/verification/PHASE_1B_DIRECT_WASM_PROOF.json",
  engineMetrics: "docs/PHASE_1B_METRICS_ENGINE_ONLY.json",
  harness: "web/editor/scripts/browser-proof-phase1b.mjs",
  packageJson: "web/editor/package.json",
};

function normalizeRepoPath(value) {
  return String(value ?? "").replaceAll("\\", "/").replace(/^\.\//, "");
}

export function calculateSummary(items) {
  const summary = { pass: 0, fail: 0, unverified: 0 };
  for (const item of items) {
    if (!VALID_STATUSES.has(item.status)) {
      throw new Error(`invalid gate item status ${item.status} for ${item.id}`);
    }
    summary[item.status.toLowerCase()] += 1;
  }
  return summary;
}

export function calculateConclusion(items) {
  const summary = calculateSummary(items);
  const required = items.filter((item) => item.required !== false);
  return summary.fail === 0 &&
    summary.unverified === 0 &&
    required.length > 0 &&
    required.every((item) => item.status === "PASS")
    ? "PASSED"
    : "NOT PASSED";
}

export function isAllowedEvidencePath(value) {
  const pathname = normalizeRepoPath(value);
  return (
    pathname.startsWith("docs/verification/") ||
    pathname === "docs/PHASE_1B_METRICS.json" ||
    pathname === "docs/PHASE_1B_METRICS_ENGINE_ONLY.json" ||
    pathname === "docs/REVIEW_PACKET_1B.md" ||
    /^docs\/PHASE_1B_GATE_[^/]+$/.test(pathname)
  );
}

export function validateBinding(artifact, context) {
  if (!artifact || !context?.gateRunId || !context?.sourceCommit || !context?.branch) {
    return false;
  }
  return (
    artifact.gate_run_id === context.gateRunId &&
    artifact.tested_source_commit === context.sourceCommit &&
    artifact.tested_branch === context.branch
  );
}

function readJson(workspace, relativePath) {
  const absolutePath = path.join(workspace, relativePath);
  if (!existsSync(absolutePath)) {
    return { exists: false, data: null, error: null, relativePath };
  }
  try {
    return {
      exists: true,
      data: JSON.parse(readFileSync(absolutePath, "utf8")),
      error: null,
      relativePath,
    };
  } catch (error) {
    return { exists: true, data: null, error: error.message, relativePath };
  }
}

function git(workspace, args, options = {}) {
  try {
    return execFileSync("git", args, {
      cwd: workspace,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      ...options,
    }).trim();
  } catch {
    return null;
  }
}

function changedWorkingTreePaths(workspace) {
  const commands = [
    ["diff", "--name-only"],
    ["diff", "--cached", "--name-only"],
    ["ls-files", "--others", "--exclude-standard"],
  ];
  const paths = new Set();
  for (const args of commands) {
    const output = git(workspace, args);
    if (output === null) return null;
    for (const entry of output.split(/\r?\n/).filter(Boolean)) {
      paths.add(normalizeRepoPath(entry));
    }
  }
  return [...paths].sort();
}

function validateSourceRelation(workspace, context, manifest) {
  if (!context.gateRunId || !context.sourceCommit || !context.branch) {
    return { valid: false, reason: "No active Gate run context is available.", currentHead: null };
  }
  if (context.branch !== EXPECTED_BRANCH) {
    return {
      valid: false,
      reason: `Gate 1B must run on ${EXPECTED_BRANCH}, not ${context.branch}.`,
      currentHead: git(workspace, ["rev-parse", "HEAD"]),
    };
  }
  const currentHead = git(workspace, ["rev-parse", "HEAD"]);
  const currentBranch = git(workspace, ["branch", "--show-current"]);
  if (!currentHead || currentBranch !== context.branch) {
    return {
      valid: false,
      reason: `Current branch ${currentBranch ?? "unknown"} does not match tested branch ${context.branch}.`,
      currentHead,
    };
  }

  let committedEvidencePaths = [];
  if (currentHead !== context.sourceCommit) {
    const ancestor = git(workspace, ["merge-base", "--is-ancestor", context.sourceCommit, currentHead]);
    if (ancestor === null) {
      return {
        valid: false,
        reason: `${context.sourceCommit} is not an ancestor of current HEAD ${currentHead}.`,
        currentHead,
      };
    }
    const diff = git(workspace, ["diff", "--name-only", `${context.sourceCommit}..${currentHead}`]);
    if (diff === null) {
      return { valid: false, reason: "Could not inspect commits after the tested source.", currentHead };
    }
    committedEvidencePaths = diff.split(/\r?\n/).filter(Boolean).map(normalizeRepoPath);
    const disallowed = committedEvidencePaths.filter((entry) => !isAllowedEvidencePath(entry));
    if (disallowed.length > 0) {
      return {
        valid: false,
        reason: `Production or harness files changed after the tested commit: ${disallowed.join(", ")}`,
        currentHead,
        committedEvidencePaths,
        disallowedPaths: disallowed,
      };
    }
  }

  const workingTreePaths = changedWorkingTreePaths(workspace);
  if (workingTreePaths === null) {
    return { valid: false, reason: "Could not inspect the current working tree.", currentHead };
  }
  const disallowedWorkingTreePaths = workingTreePaths.filter(
    (entry) => !isAllowedEvidencePath(entry),
  );
  const initialUnexpected = manifest?.source_tree?.unexpected_paths ?? [];
  if (initialUnexpected.length > 0 || disallowedWorkingTreePaths.length > 0) {
    const disallowed = [...new Set([...initialUnexpected, ...disallowedWorkingTreePaths])];
    return {
      valid: false,
      reason: `Unexpected source-tree changes invalidate the proof: ${disallowed.join(", ")}`,
      currentHead,
      committedEvidencePaths,
      workingTreePaths,
      disallowedPaths: disallowed,
    };
  }

  return {
    valid: true,
    reason:
      currentHead === context.sourceCommit
        ? "Current HEAD is the tested source commit; only generated evidence is dirty."
        : "Every commit after the tested source changes verification evidence only.",
    currentHead,
    committedEvidencePaths,
    workingTreePaths,
    disallowedPaths: [],
  };
}

function stepFor(manifest, id) {
  return manifest?.steps?.find((step) => step.id === id) ?? null;
}

function isGpuUnavailableFailure(failure) {
  if (!failure) return false;
  const code = String(failure.code ?? "").toLowerCase();
  if (["webgpu_initialization_failed", "webgpu_unavailable", "software_gpu_rejected"].includes(code)) {
    return true;
  }
  if (code !== "application_readiness_timeout") return false;
  const haystack = JSON.stringify({
    message: failure.message,
    details: failure.details,
    browser: failure.browser,
    console_errors: failure.console_errors,
    chrome_stderr_tail: failure.chrome_stderr_tail,
  }).toLowerCase();
  return /(instance dropped|failed to (request|create).*?(adapter|device)|no compatible.*?(adapter|device)|navigator_gpu[^a-z]+false|gpu process.*?(failed|crash)|device (?:was )?lost)/.test(haystack);
}

function result(status, reason, artifacts = []) {
  return { status, reason, artifacts };
}

function assessBoundArtifact({ record, failureRecord, context, step, validate, gpuDependent }) {
  const bound = validateBinding(record.data, context);
  if (bound) {
    const errors = record.error ? [record.error] : validate(record.data);
    return errors.length === 0
      ? result("PASS", "Current Gate run evidence passed every required check.", [record.relativePath])
      : result("FAIL", errors.join(" "), [record.relativePath]);
  }

  const boundFailure = validateBinding(failureRecord?.data, context);
  if (boundFailure) {
    const status = gpuDependent && isGpuUnavailableFailure(failureRecord.data)
      ? "UNVERIFIED"
      : "FAIL";
    return result(
      status,
      `${failureRecord.data.code ?? "execution_failed"}: ${failureRecord.data.message ?? "Gate command failed"}`,
      [failureRecord.relativePath],
    );
  }

  if (step?.status === "FAIL") {
    return result(
      "FAIL",
      `${step.id} exited with ${step.exit_code}; no correctly bound failure artifact was produced.`,
      failureRecord?.exists ? [failureRecord.relativePath] : [],
    );
  }
  if (step?.status === "PASS") {
    return result(
      "FAIL",
      `${step.id} passed but its evidence is missing or belongs to another Gate run.`,
      record.exists ? [record.relativePath] : [],
    );
  }
  return result("UNVERIFIED", "No evidence from the current Gate run is available.");
}

function phase1aErrors(proof) {
  const errors = [];
  if (proof.phase !== "1A") errors.push("Phase 1A proof has the wrong phase.");
  if (proof.proof_kind !== "actual-hardware-browser") errors.push("Phase 1A proof is not hardware browser evidence.");
  if (!/^Chrome\//.test(proof.browser ?? "")) errors.push("Phase 1A proof did not use real Chrome.");
  if (proof.initial?.worker_runtime_owner !== "dedicated-worker") errors.push("Phase 1A did not use a Dedicated Worker.");
  if (proof.initial?.wasm_initialized !== true) errors.push("Phase 1A WASM was not initialized.");
  if (proof.initial?.actual_webgpu !== true) errors.push("Phase 1A did not use actual WebGPU.");
  if (proof.all_passed !== true) errors.push("Phase 1A browser regression did not pass.");
  return errors;
}

function phase1bBrowserErrors(proof) {
  const errors = [];
  if (proof.phase !== "1B") errors.push("Phase 1B proof has the wrong phase.");
  if (proof.proof_kind !== "actual-hardware-browser") errors.push("Phase 1B proof is not hardware browser evidence.");
  if (!/^Chrome\//.test(proof.browser ?? "")) errors.push("Phase 1B proof did not use real Chrome.");
  if (proof.initial?.worker_runtime_owner !== "dedicated-worker") errors.push("Phase 1B did not use a Dedicated Worker.");
  if (proof.initial?.wasm_initialized !== true) errors.push("Phase 1B WASM was not initialized.");
  if (proof.initial?.actual_webgpu !== true) errors.push("Phase 1B did not use actual WebGPU.");
  if (proof.gpu?.software_renderer !== false) errors.push("Phase 1B used or failed to exclude a software renderer.");
  if (proof.execution?.software_gpu_allowed !== false) errors.push("Software GPU diagnostics cannot be Gate evidence.");
  if (proof.all_passed !== true) errors.push("Phase 1B browser proof did not pass every assertion.");
  return errors;
}

function pixelErrors(pixels) {
  return pixels.phase === "1B" && pixels.all_passed === true
    ? []
    : ["Phase 1B pixel readback is missing required passing assertions."];
}

export function benchmarkErrors(metrics, { engineOnly = false } = {}) {
  const errors = [];
  if (metrics?.phase !== "1B" || metrics?.all_passed !== true || !Array.isArray(metrics?.series)) {
    return ["Phase 1B benchmark is malformed or did not pass its declared checks."];
  }
  const budget = Number(metrics.frame_budget_ms ?? 16.7);
  for (const objects of [10, 100, 1000]) {
    const series = metrics.series.filter((entry) => entry.objects === objects);
    if (series.length !== 2 || !series.some((entry) => entry.snapping === false) || !series.some((entry) => entry.snapping === true)) {
      errors.push(`Missing snapped or unsnapped ${objects}-object series.`);
      continue;
    }
    for (const entry of series) {
      if (!Array.isArray(entry.raw_frame_ms) || entry.raw_frame_ms.length !== entry.measured_iterations) {
        errors.push(`${objects}-object series does not contain all raw samples.`);
      }
      if (objects <= 100 && Number(entry.p95_ms) > budget) {
        errors.push(`${objects}-object p95 exceeds the existing ${budget} ms Gate threshold.`);
      }
      if (engineOnly && entry.dirty_slots !== objects) {
        errors.push(`${objects}-object engine series dirty-slot count is not bounded to selection size.`);
      }
    }
  }
  return errors;
}

function directWasmErrors(proof) {
  const checks = Array.isArray(proof?.checks) ? proof.checks : [];
  return proof?.phase === "1B" &&
    proof?.proof_kind === "actual-wasm-direct-node" &&
    proof?.all_passed === true &&
    proof?.checks_total === 46 &&
    proof?.checks_passed === 46 &&
    checks.length === 46 &&
    checks.every((check) => check.passed === true)
    ? []
    : ["Direct WASM proof is not a passing 46/46 run."];
}

function behaviorResult(base, proof, requiredChecks) {
  if (base.status !== "PASS") return base;
  const failed = requiredChecks.filter((name) => proof?.checks?.[name] !== true);
  return failed.length === 0
    ? base
    : result("FAIL", `Browser proof is missing or failed: ${failed.join(", ")}`, base.artifacts);
}

function firstFalseKey(value) {
  return Object.entries(value ?? {}).find(([, passed]) => passed !== true)?.[0] ?? null;
}

function makeItem(id, requirement, assessment, options = {}) {
  return {
    id,
    requirement,
    required: true,
    requires_gpu: options.requiresGpu === true,
    status: assessment.status,
    evidence: assessment.reason,
    artifacts: assessment.artifacts ?? [],
  };
}

export function finalizeGate({ workspace = defaultWorkspace } = {}) {
  const manifestRecord = readJson(workspace, RELATIVE_PATHS.manifest);
  const manifest = manifestRecord.data;
  const context = {
    gateRunId: process.env.PHASE1B_GATE_RUN_ID ?? manifest?.gate_run_id ?? null,
    sourceCommit:
      process.env.PHASE1B_GATE_SOURCE_COMMIT ?? manifest?.tested_source_commit ?? null,
    branch: process.env.PHASE1B_GATE_SOURCE_BRANCH ?? manifest?.tested_branch ?? null,
  };
  const contextMatchesManifest = Boolean(
    manifest &&
      context.gateRunId === manifest.gate_run_id &&
      context.sourceCommit === manifest.tested_source_commit &&
      context.branch === manifest.tested_branch,
  );

  const records = Object.fromEntries(
    Object.entries(RELATIVE_PATHS).map(([key, relativePath]) => [key, readJson(workspace, relativePath)]),
  );
  const sourceRelation = contextMatchesManifest
    ? validateSourceRelation(workspace, context, manifest)
    : { valid: false, reason: "Gate environment and run manifest are missing or inconsistent.", currentHead: git(workspace, ["rev-parse", "HEAD"]) };

  const sourceAssessment = sourceRelation.valid
    ? result("PASS", sourceRelation.reason, [RELATIVE_PATHS.manifest])
    : result(context.gateRunId ? "FAIL" : "UNVERIFIED", sourceRelation.reason, manifestRecord.exists ? [RELATIVE_PATHS.manifest] : []);

  const phase1a = assessBoundArtifact({
    record: records.phase1aProof,
    failureRecord: records.phase1aFailure,
    context,
    step: stepFor(manifest, "phase1a_browser"),
    validate: phase1aErrors,
    gpuDependent: true,
  });
  const browser = assessBoundArtifact({
    record: records.browserProof,
    failureRecord: records.browserFailure,
    context,
    step: stepFor(manifest, "phase1b_browser"),
    validate: phase1bBrowserErrors,
    gpuDependent: true,
  });
  const pixels = assessBoundArtifact({
    record: records.pixels,
    failureRecord: records.browserFailure,
    context,
    step: stepFor(manifest, "phase1b_browser"),
    validate: pixelErrors,
    gpuDependent: true,
  });
  const browserMetrics = assessBoundArtifact({
    record: records.browserMetrics,
    failureRecord: records.browserFailure,
    context,
    step: stepFor(manifest, "phase1b_browser"),
    validate: (value) => benchmarkErrors(value),
    gpuDependent: true,
  });
  const directWasm = assessBoundArtifact({
    record: records.directWasm,
    context,
    step: stepFor(manifest, "direct_wasm"),
    validate: directWasmErrors,
    gpuDependent: false,
  });
  const engineMetrics = assessBoundArtifact({
    record: records.engineMetrics,
    context,
    step: stepFor(manifest, "engine_benchmark"),
    validate: (value) => benchmarkErrors(value, { engineOnly: true }),
    gpuDependent: false,
  });

  const requiredSteps = [
    "cargo_fmt",
    "cargo_clippy",
    "cargo_build",
    "cargo_test",
    "wasm_build",
    "npm_ci",
    "phase1a_browser",
    "phase1b_browser",
    "direct_wasm",
    "engine_benchmark",
    "editor_build",
  ];
  const stepStates = requiredSteps.map((id) => stepFor(manifest, id));
  const workspaceAssessment = manifest?.first_failure || stepStates.some((step) => step?.status === "FAIL")
    ? result("FAIL", `Gate runner failed at ${manifest?.first_failure?.step_id ?? stepStates.find((step) => step?.status === "FAIL")?.id}.`, [RELATIVE_PATHS.manifest])
    : stepStates.every((step) => step?.status === "PASS")
      ? result("PASS", "Every required repository, toolchain, dependency, and build step passed.", [RELATIVE_PATHS.manifest])
      : result("UNVERIFIED", "The complete runner prerequisite sequence has not passed.", manifestRecord.exists ? [RELATIVE_PATHS.manifest] : []);

  const harnessSource = existsSync(path.join(workspace, RELATIVE_PATHS.harness))
    ? readFileSync(path.join(workspace, RELATIVE_PATHS.harness), "utf8")
    : "";
  const packageData = records.packageJson.data;
  const harnessAssessment = harnessSource && packageData?.scripts?.["test:browser:phase1b"]
    ? result("PASS", "The Phase 1B hardware harness and one-command npm entry point exist.", [RELATIVE_PATHS.harness, RELATIVE_PATHS.packageJson])
    : result("FAIL", "The Phase 1B browser harness or npm entry point is missing.");
  const realPathAssessment = ["--enable-unsafe-webgpu", "dedicated-worker", "no mock", "no Canvas2D"].every((needle) => harnessSource.includes(needle))
    ? result("PASS", "Harness source requires actual Chrome, Dedicated Worker, shipped WASM, and WebGPU.", [RELATIVE_PATHS.harness])
    : result("FAIL", "Harness source no longer demonstrates the required real execution path.", [RELATIVE_PATHS.harness]);

  const items = [
    makeItem("harness_exists", "A Phase 1B browser/WebGPU proof harness exists and runs with one command", harnessAssessment),
    makeItem("real_path_only", "The harness drives Chrome -> React -> Dedicated Worker -> WASM -> WebGPU without substitutes", realPathAssessment),
    makeItem("source_evidence_binding", "All Gate evidence is bound to one run, branch, and tested source commit", sourceAssessment),
    makeItem("phase1a_regression", "The current Gate run passes the Phase 1A browser regression", phase1a, { requiresGpu: true }),
    makeItem("shift_multi_selection", "Shift multi-selection is verified in the browser", behaviorResult(browser, records.browserProof.data, ["shift_click_selects_two", "multiple_selection_draws_every_outline"]), { requiresGpu: true }),
    makeItem("marquee_selection", "Rubber-band selection is verified in the browser", behaviorResult(browser, records.browserProof.data, ["marquee_selects_intersecting_nodes"]), { requiresGpu: true }),
    makeItem("select_all", "Ctrl/Cmd+A is verified in the browser", behaviorResult(browser, records.browserProof.data, ["select_all_selects_every_top_level_node"]), { requiresGpu: true }),
    makeItem("multi_object_drag", "Multi-object drag is verified in the browser", behaviorResult(browser, records.browserProof.data, ["multi_drag_moves_every_node_by_one_delta", "multi_drag_leaves_unselected_nodes_alone"]), { requiresGpu: true }),
    makeItem("coalesced_drag_no_drift", "Coalesced drag frames do not drift", behaviorResult(browser, records.browserProof.data, ["coalesced_drag_has_no_drift", "coalesced_drag_coalesced_requests"]), { requiresGpu: true }),
    makeItem("edge_center_snapping", "Edge and center snapping are verified in the browser", behaviorResult(browser, records.browserProof.data, ["snapping_aligns_edges_exactly"]), { requiresGpu: true }),
    makeItem("alt_snap_suspend", "Alt snap suspension is verified in the browser", behaviorResult(browser, records.browserProof.data, ["alt_suspends_snapping"]), { requiresGpu: true }),
    makeItem("snap_toggle", "The snap toggle is verified in the browser", behaviorResult(browser, records.browserProof.data, ["snap_toggle_off_suspends_snapping", "snap_toggle_restored"]), { requiresGpu: true }),
    makeItem("snap_guide_rendered", "The snap guide is proven by actual pixels", behaviorResult(browser, records.browserProof.data, ["snap_guide_reported_and_rendered"]), { requiresGpu: true }),
    makeItem("align_six_ways", "All six alignment operations are verified in the browser", behaviorResult(browser, records.browserProof.data, ["align_left_aligned", "align_horizontal_center_aligned", "align_right_aligned", "align_top_aligned", "align_vertical_center_aligned", "align_bottom_aligned"]), { requiresGpu: true }),
    makeItem("distribute_both_axes", "Horizontal and vertical distribution are verified", behaviorResult(browser, records.browserProof.data, ["distribute_horizontal_equal_gaps", "distribute_vertical_equal_gaps"]), { requiresGpu: true }),
    makeItem("arrange_undo", "Arrange operations restore through one undo", behaviorResult(browser, records.browserProof.data, ["align_left_undo_restored", "align_horizontal_center_undo_restored", "align_right_undo_restored", "align_top_undo_restored", "align_vertical_center_undo_restored", "align_bottom_undo_restored", "distribute_horizontal_undo_restored", "distribute_vertical_undo_restored"]), { requiresGpu: true }),
    makeItem("multi_drag_undo", "A multi-object drag restores through one undo", behaviorResult(browser, records.browserProof.data, ["multi_drag_undo_restores_every_node"]), { requiresGpu: true }),
    makeItem("pixel_readback_overlays", "Pixel evidence passes for selections, marquee, guide, and moved content", pixels, { requiresGpu: true }),
    makeItem("browser_multi_drag_benchmark", "Browser multi-drag has 10/100/1,000 series and keeps existing 10/100 thresholds", browserMetrics, { requiresGpu: true }),
    makeItem("engine_behaviour_proof", "Direct shipped-WASM behavior proof passes 46/46", directWasm),
    makeItem("engine_multi_drag_benchmark", "Engine benchmark records passing 10/100/1,000 series", engineMetrics),
    makeItem("workspace_checks", "Repository, toolchain, dependency, and build verification passes", workspaceAssessment),
  ];

  const summary = calculateSummary(items);
  const gateConclusion = calculateConclusion(items);
  const firstAssertion =
    firstFalseKey(records.phase1aProof.data?.checks) ??
    firstFalseKey(records.browserProof.data?.checks) ??
    firstFalseKey(records.pixels.data?.assertions) ??
    firstFalseKey(records.browserMetrics.data?.checks) ??
    records.directWasm.data?.checks?.find((check) => check.passed !== true)?.name ??
    manifest?.first_failure?.step_id ??
    items.find((item) => item.status !== "PASS")?.id ??
    null;
  const firstFailure = summary.fail === 0 ? null : {
    assertion: firstAssertion,
    gate_run_id: context.gateRunId,
    tested_source_commit: context.sourceCommit,
    tested_branch: context.branch,
    chrome: manifest?.environment?.chrome_executable ?? null,
    gpu: records.browserProof.data?.gpu ?? records.browserFailure.data?.browser?.proof?.gpu ?? null,
    runner_failure: manifest?.first_failure ?? null,
  };

  const phase1aRecord = {
    phase: "1B",
    record: "Phase 1A browser regression result for the current Gate 1B run",
    gate_run_id: context.gateRunId,
    tested_source_commit: context.sourceCommit,
    tested_branch: context.branch,
    captured_at_utc: new Date().toISOString(),
    status: phase1a.status,
    reason: phase1a.reason,
    artifacts: phase1a.artifacts,
  };
  writeFileSync(
    path.join(workspace, RELATIVE_PATHS.phase1aRecord),
    `${JSON.stringify(phase1aRecord, null, 2)}\n`,
    "utf8",
  );

  const status = {
    phase: "1B",
    gate: "Gate 1B",
    generated_at_utc: new Date().toISOString(),
    gate_run_id: context.gateRunId,
    tested_source_commit: context.sourceCommit,
    tested_branch: context.branch,
    current_head: sourceRelation.currentHead ?? null,
    gate_conclusion: gateConclusion,
    runner_status: manifest?.status ?? null,
    browser_proof_present: records.browserProof.exists,
    current_browser_proof_accepted: browser.status === "PASS",
    hardware_run_instructions: "docs/PHASE_1B_HARDWARE_RUN.md",
    environment: manifest?.environment ?? null,
    source_relation: sourceRelation,
    items,
    summary,
    first_failure: firstFailure,
    first_unverified:
      summary.unverified === 0
        ? null
        : items.find((item) => item.status === "UNVERIFIED")?.id ?? null,
    findings: [
      "The 1,000-object drag result is measured performance debt and has no new Gate threshold.",
      "Primitive creation currently places shapes as siblings of the default Frame; marquee exclude_ids avoids selecting its own backdrop, while Frame containment remains future work.",
    ],
  };
  writeFileSync(
    path.join(workspace, RELATIVE_PATHS.status),
    `${JSON.stringify(status, null, 2)}\n`,
    "utf8",
  );
  return status;
}

const isMain = process.argv[1] && path.resolve(process.argv[1]).toLowerCase() === scriptPath.toLowerCase();
if (isMain) {
  try {
    const status = finalizeGate();
    console.log(`Phase 1B gate finalizer: ${status.gate_conclusion}`);
    console.log(`run_id=${status.gate_run_id ?? "none"}`);
    console.log(`source_commit=${status.tested_source_commit ?? "none"}`);
    console.log(`summary=${JSON.stringify(status.summary)}`);
    if (status.gate_conclusion !== "PASSED") process.exitCode = 1;
  } catch (error) {
    console.error(`Phase 1B gate finalizer failed: ${error.stack ?? error.message ?? error}`);
    process.exitCode = 1;
  }
}

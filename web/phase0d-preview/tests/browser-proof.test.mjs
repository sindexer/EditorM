import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import path from "node:path";
import { fileURLToPath } from "node:url";

const proofPath = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../../../docs/verification/PHASE_0D_R1_BROWSER_PROOF.json",
);

test("R1 browser proof uses actual WebGPU pixels, DOM input, and ordered Worker frames", async () => {
  const proof = JSON.parse(await readFile(proofPath, "utf8"));
  assert.equal(proof.phase, "0D-R1");
  assert.equal(proof.proof_kind, "actual-hardware-browser");
  assert.equal(proof.all_passed, true);
  assert.equal(proof.harness.health.ok, true);
  for (const asset of Object.values(proof.harness.assets)) assert.equal(asset.status, 200);
  assert.equal(proof.runtime.wasm_initialized, true);
  assert.equal(proof.runtime.worker_runtime_owner, "dedicated-worker");
  assert.equal(proof.runtime.main_thread_document_mutation_api, false);
  assert.equal(proof.webgpu.actual_webgpu, true);
  assert.equal(proof.webgpu.gpu_validation_errors, 0);
  assert.equal(proof.checks.actual_pixel_readback, true);
  assert.equal(proof.checks.actual_dom_controls, true);
  assert.equal(proof.checks.camera_burst_coalesced, true);
  assert.equal(proof.checks.heartbeat_does_not_replace_frame, true);
  assert.equal(proof.checks.restart_rejects_waiter, true);
  assert.equal(proof.checks.ordered_gpu_frames, true);
  assert.equal(proof.checks.f32_omission_and_recovery, true);
  assert.equal(proof.checks.ten_thousand_instanced, true);
  assert.equal(proof.checks.sparse_hundred_thousand_pruned, true);
  assert.equal(proof.checks.browser_console_errors_zero, true);
  assert.equal(proof.runtime.response_gpu_sequence_match, true);
  assert.equal(proof.runtime.gpu_frame_sequence, proof.runtime.engine_sequence);
  assert.equal(proof.runtime.gpu.frame_sequence, proof.runtime.engine_sequence);
});
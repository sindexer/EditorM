import assert from "node:assert/strict";
import test from "node:test";
import {
  assertEngineResponse,
  createRequest,
  PROTOCOL_VERSION,
  ProtocolFailure,
  splitF64,
} from "../src/protocol.js";
import {
  DIRTY_STRIDE,
  INSTANCE_STRIDE,
  RENDER_BINARY_SCHEMA,
  RENDER_BINARY_SCHEMA_VERSION,
  SHADER_SOURCE,
} from "../src/render_contract.js";
import {
  EXPECTED_PREVIEW_ASSETS,
  HarnessFailure,
  classifyApplicationFailure,
  validateAssetResponse,
} from "../scripts/harness-support.mjs";

test("versioned requests carry unique correlation IDs", () => {
  const first = createRequest("heartbeat");
  const second = createRequest("heartbeat");
  assert.equal(first.protocol_version, PROTOCOL_VERSION);
  assert.notEqual(first.request_id, second.request_id);
});

test("response validation rejects protocol, render schema, and sequence drift", () => {
  assert.throws(
    () => assertEngineResponse({ type: "engine_response", protocol_version: 99, request_id: "x" }),
    (error) => error instanceof ProtocolFailure && error.code === "protocol_version_mismatch",
  );
  assert.throws(
    () =>
      assertEngineResponse({
        type: "engine_response",
        protocol_version: PROTOCOL_VERSION,
        render_binary_schema_version: 999,
        engine_sequence: 1,
        request_id: "x",
      }),
    (error) => error instanceof ProtocolFailure && error.code === "render_schema_version_mismatch",
  );
  assert.throws(
    () =>
      assertEngineResponse({
        type: "engine_response",
        protocol_version: PROTOCOL_VERSION,
        render_binary_schema_version: RENDER_BINARY_SCHEMA_VERSION,
        engine_sequence: 0,
        request_id: "x",
      }),
    (error) => error instanceof ProtocolFailure && error.code === "invalid_engine_sequence",
  );
});

test("high and low f32 camera split preserves a large finite coordinate", () => {
  const value = 10_000_000_000.125;
  const [high, low] = splitF64(value);
  assert.ok(Number.isFinite(high));
  assert.ok(Number.isFinite(low));
  assert.ok(Math.abs(high + low - value) < 0.001);
});

test("shared render schema fixes binary offsets, strides, and primitive values", () => {
  assert.equal(RENDER_BINARY_SCHEMA_VERSION, 1);
  assert.equal(INSTANCE_STRIDE, 48);
  assert.equal(DIRTY_STRIDE, 52);
  assert.equal(RENDER_BINARY_SCHEMA.endianness, "little");
  assert.deepEqual(RENDER_BINARY_SCHEMA.primitive, { rectangle: 0, ellipse: 1 });
  assert.deepEqual(RENDER_BINARY_SCHEMA.instance_fields, {
    linear: 0,
    translation_hi: 16,
    translation_lo: 24,
    size: 32,
    opacity: 40,
    primitive: 44,
  });
});

test("WGSL is geometry-aware and uses instanced visible slots", () => {
  assert.match(SHADER_SOURCE, /visible_slots\[visible_index\]/);
  assert.match(SHADER_SOURCE, /input\.primitive == 1u/);
  assert.match(SHADER_SOURCE, /discard/);
  assert.match(SHADER_SOURCE, /switch vertex_index/);
});

test("self-contained harness classifies asset status and MIME failures", () => {
  assert.deepEqual([...EXPECTED_PREVIEW_ASSETS], [
    ["/index.html", "text/html"],
    ["/src/worker.js", "text/javascript"],
    ["/pkg/engine_host.js", "text/javascript"],
    ["/pkg/engine_host_bg.wasm", "application/wasm"],
  ]);
  assert.deepEqual(
    validateAssetResponse("/pkg/engine_host_bg.wasm", "application/wasm", {
      ok: true,
      status: 200,
      contentType: "application/wasm",
    }),
    { status: 200, content_type: "application/wasm" },
  );
  assert.throws(
    () =>
      validateAssetResponse("/src/worker.js", "text/javascript", {
        ok: false,
        status: 404,
        contentType: "text/plain",
      }),
    (error) => error instanceof HarnessFailure && error.code === "preview_asset_http_error",
  );
  assert.throws(
    () =>
      validateAssetResponse("/pkg/engine_host_bg.wasm", "application/wasm", {
        ok: true,
        status: 200,
        contentType: "application/octet-stream",
      }),
    (error) => error instanceof HarnessFailure && error.code === "preview_asset_mime_error",
  );
  assert.equal(
    classifyApplicationFailure({ code: "wasm_initialization_failed", message: "boot" }).code,
    "wasm_boot_failed",
  );
  assert.equal(
    classifyApplicationFailure({ code: "worker_request_failed", message: "request" }).code,
    "worker_failed",
  );
  assert.equal(
    classifyApplicationFailure({ code: "adapter_unavailable", message: "gpu" }).code,
    "webgpu_initialization_failed",
  );
});
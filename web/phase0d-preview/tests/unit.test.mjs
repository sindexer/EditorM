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
  PATH_INSTANCE_STRIDE,
  PATH_VERTEX_STRIDE,
  RENDER_BINARY_SCHEMA_VERSION,
  SHADER_SOURCE,
} from "../src/render_contract.js";
import { RendererFailure, srgbViewFormat } from "../src/renderer.js";
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
  assert.equal(RENDER_BINARY_SCHEMA_VERSION, 3);
  assert.equal(INSTANCE_STRIDE, 112);
  assert.equal(DIRTY_STRIDE, 116);
  assert.equal(RENDER_BINARY_SCHEMA.endianness, "little");
  assert.equal(RENDER_BINARY_SCHEMA.alpha_contract, "premultiplied-linear");
  assert.equal(RENDER_BINARY_SCHEMA.color_input, "srgb");
  assert.equal(RENDER_BINARY_SCHEMA.stroke_alignment, "center");
  assert.deepEqual(RENDER_BINARY_SCHEMA.primitive, { rectangle: 0, ellipse: 1, path: 2 });
  assert.deepEqual(RENDER_BINARY_SCHEMA.instance_fields, {
    linear: 0,
    translation_hi: 16,
    translation_lo: 24,
    size: 32,
    opacity: 40,
    primitive: 44,
    fill_linear: 48,
    corner_radii: 64,
    stroke_linear: 80,
    stroke_width: 96,
    padding: 100,
  });
});

test("schema version 3 publishes the path layout the path pipeline reads", () => {
  assert.equal(PATH_INSTANCE_STRIDE, 80);
  assert.equal(PATH_VERTEX_STRIDE, 32);
  // The CPU hit test and the GPU triangles share this one fill rule.
  assert.equal(RENDER_BINARY_SCHEMA.path_fill_rule, "even-odd");
  assert.deepEqual(RENDER_BINARY_SCHEMA.path_vertex_kind, { fill: 0, stroke: 1 });
  assert.deepEqual(RENDER_BINARY_SCHEMA.path_instance_fields, {
    linear: 0,
    translation_hi: 16,
    translation_lo: 24,
    fill_linear: 32,
    stroke_linear: 48,
    opacity: 64,
    padding: 68,
  });
  assert.deepEqual(RENDER_BINARY_SCHEMA.path_vertex_fields, {
    position: 0,
    normal: 8,
    coverage: 16,
    kind: 20,
    path_index: 24,
    padding: 28,
  });
  assert.match(SHADER_SOURCE, /fn vs_path/);
  assert.match(SHADER_SOURCE, /fn fs_path/);
  // Paths antialias by expanding a feather in screen space, never by discarding fragments.
  assert.doesNotMatch(SHADER_SOURCE, /discard/);
});

test("WGSL is geometry-aware and uses instanced analytic coverage", () => {
  assert.match(SHADER_SOURCE, /visible_slots\[visible_index\]/);
  assert.match(SHADER_SOURCE, /input\.primitive == 1u/);
  assert.match(SHADER_SOURCE, /fwidth\(distance\)/);
  assert.match(SHADER_SOURCE, /smoothstep\(-width, width, distance\)/);
  assert.doesNotMatch(SHADER_SOURCE, /ellipse_axis_ratio/);
  assert.match(SHADER_SOURCE, /linear\.x \* local\.x \+ item\.linear\.y \* local\.y/);
  assert.match(SHADER_SOURCE, /linear\.z \* local\.x \+ item\.linear\.w \* local\.y/);
  assert.match(SHADER_SOURCE, /gradient_length/);
  assert.match(SHADER_SOURCE, /vec4<f32>\(premultiplied, alpha\) \* input\.opacity/);
  assert.doesNotMatch(SHADER_SOURCE, /fill_alpha \* \(1\.0 - stroke_alpha\)/);
  assert.doesNotMatch(SHADER_SOURCE, /stroke_alpha \+ fill_alpha \* \(1\.0 - stroke_alpha\)/);
  assert.doesNotMatch(SHADER_SOURCE, /\bdiscard\b/);
  assert.match(SHADER_SOURCE, /switch vertex_index/);
});

test("sRGB render view mapping is explicit and fail-closed", () => {
  assert.equal(srgbViewFormat("bgra8unorm"), "bgra8unorm-srgb");
  assert.equal(srgbViewFormat("rgba8unorm"), "rgba8unorm-srgb");
  assert.throws(
    () => srgbViewFormat("rgba16float"),
    (error) =>
      error instanceof RendererFailure &&
      error.code === "srgb_view_format_unavailable",
  );
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
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { describe, expect, test } from "vitest";

const editorRoot = path.resolve(process.cwd());
const workspace = path.resolve(editorRoot, "../..");

function read(relativePath: string) {
  return readFileSync(path.join(workspace, relativePath), "utf8");
}

// Runs in the default suite, with no GPU. It guards the two Phase 2A contracts that can be
// checked from source: paths reach the screen only through the Rust/WASM/WebGPU path, and the
// browser never re-implements path geometry.
describe("Phase 2A path rendering contract", () => {
  test("the shared render contract publishes a path pipeline at schema version 3", () => {
    const schema = JSON.parse(read("shared/render_binary_schema.json"));
    expect(schema.version).toBe(3);
    expect(schema.path_instance_stride_bytes).toBe(80);
    expect(schema.path_vertex_stride_bytes).toBe(32);
    expect(schema.path_fill_rule).toBe("even-odd");
    expect(schema.primitive.path).toBe(2);

    const shader = read("shared/render_contract.wgsl");
    expect(shader).toContain("fn vs_path");
    expect(shader).toContain("fn fs_path");
    expect(shader).toContain("path_vertices[vertex_index]");
    // Path antialiasing is a screen-space feather, never MSAA or a discarded fragment.
    expect(shader).not.toContain("discard");
  });

  test("the generated browser contract matches the shared source of truth", () => {
    const generated = read("web/phase0d-preview/src/render_contract.js");
    const shader = read("shared/render_contract.wgsl");
    expect(generated).toContain(JSON.stringify(shader));
    expect(generated).toContain("PATH_INSTANCE_STRIDE");
    expect(generated).toContain("PATH_VERTEX_STRIDE");
  });

  test("the worker forwards path buffers and the renderer draws them on WebGPU", () => {
    const worker = read("web/editor/public/worker.js");
    expect(worker).toContain("takePathInstances");
    expect(worker).toContain("takePathVertices");

    const renderer = read("web/phase0d-preview/src/renderer.js");
    expect(renderer).toContain("entryPoint: \"vs_path\"");
    expect(renderer).toContain("pathVertexBuffer");
    // Ordered batches are what keep paths and primitives in scene order.
    expect(renderer).toContain("encodeDrawBatches");
  });

  test("no path geometry is computed or drawn in the browser", () => {
    const sources = readdirSync(path.join(editorRoot, "src")).filter((file) =>
      /\.(ts|tsx)$/.test(file),
    );
    expect(sources.length).toBeGreaterThan(0);
    for (const file of sources) {
      const contents = read(`web/editor/src/${file}`);
      // Rust owns every curve: no Bezier evaluation, flattening or anchor maths in the browser.
      for (const forbidden of [
        "bezierCurveTo",
        "quadraticCurveTo",
        "handle_out",
        "handle_in",
        "flatten",
      ]) {
        expect(contents, `${file} must not compute path geometry (${forbidden})`).not.toContain(
          forbidden,
        );
      }
      // Document content is never drawn with Canvas2D.
      expect(contents, `${file} must not use a 2D canvas context`).not.toContain(
        "getContext(\"2d\")",
      );
    }
  });

  // The first Windows hardware run failed here, not in the product: a click opens an interaction
  // transaction, and the editor closes it asynchronously after the pointer is released, so a
  // request sent the moment CDP resolved mouseReleased was correctly rejected by the engine with
  // "operation dispatch is not allowed while a transaction is active". These assertions keep the
  // harness synchronising on state instead of on elapsed time.
  test("the browser harness waits for interaction quiescence rather than sleeping", () => {
    const harness = read("web/editor/scripts/browser-proof-phase2a.mjs");

    // The helper exists and checks every signal that says an interaction is over.
    expect(harness).toContain("async function waitForInteractionIdle(");
    for (const signal of [
      "proof.fsm",
      "proof.history?.transaction_active",
      "proof.interaction_active",
      "interaction_queue?.in_flight",
      "interaction_queue?.scheduled",
    ]) {
      expect(harness, `quiescence must consider ${signal}`).toContain(signal);
    }

    // A click is not complete until the interaction it opened is finished.
    const clickPoint = harness.slice(
      harness.indexOf("async function clickPoint("),
      harness.indexOf("async function clickSelector(") > 0
        ? harness.indexOf("async function clickSelector(")
        : harness.indexOf("async function capture("),
    );
    expect(clickPoint).toContain("waitForInteractionIdle");

    // Direct command dispatch crosses the interaction boundary, so it asserts quiescence first.
    for (const boundary of [
      "before dispatching create_path",
      "before dispatching set_path_geometry",
      "before undo",
      "before redo",
      "before save_document",
      "before load_document",
    ]) {
      expect(harness, `missing idle boundary: ${boundary}`).toContain(boundary);
    }

    // A distinct failure code keeps the next failure classifiable as harness race vs product leak.
    expect(harness).toContain("interaction_quiescence_timeout");
    for (const detail of [
      "fsm:",
      "transaction_active:",
      "interaction_active:",
      "interaction_queue:",
      "engine_sequence:",
      "gpu_frame_sequence:",
    ]) {
      expect(harness, `timeout evidence must record ${detail}`).toContain(detail);
    }

    // Synchronisation is state-based: inside the proof body, sleep may only be a polling
    // interval. The final `finally` block is process cleanup, not synchronisation, so it is
    // excluded rather than exempted by a magic number.
    const cleanupIndex = harness.lastIndexOf("} finally {");
    expect(cleanupIndex).toBeGreaterThan(0);
    const proofBody = harness.slice(0, cleanupIndex);
    const sleepCalls = [...proofBody.matchAll(/await sleep\((\d+)\)/g)].map((match) =>
      Number(match[1]),
    );
    expect(sleepCalls.length).toBeGreaterThan(0);
    for (const milliseconds of sleepCalls) {
      expect(
        milliseconds,
        `await sleep(${milliseconds}) is a fixed delay, not a polling interval`,
      ).toBeLessThanOrEqual(100);
    }
  });

  test("a Phase 2A browser proof is never claimed without the artifact", () => {
    const proofPath = path.join(workspace, "docs/verification/PHASE_2A_BROWSER_PROOF.json");
    const record = read("docs/verification/PHASE_2A_PATH_RENDERING.md");
    if (!existsSync(proofPath)) {
      expect(record).toContain("UNVERIFIED");
      expect(record).not.toMatch(/hardware browser proof:\s*PASS/i);
      return;
    }
    const proof = JSON.parse(readFileSync(proofPath, "utf8"));
    expect(proof.phase).toBe("2A");
    expect(proof.gpu.software_renderer).toBe(false);
    expect(proof.all_passed).toBe(true);
  });
});

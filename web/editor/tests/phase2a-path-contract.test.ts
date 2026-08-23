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

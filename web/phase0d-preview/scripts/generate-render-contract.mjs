import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");
const webRoot = path.join(root, "web", "phase0d-preview");
const shader = await readFile(path.join(root, "shared", "render_contract.wgsl"), "utf8");
const schema = JSON.parse(
  await readFile(path.join(root, "shared", "render_binary_schema.json"), "utf8"),
);
const generated = `// Generated from shared/render_contract.wgsl and render_binary_schema.json.\n` +
  `export const RENDER_BINARY_SCHEMA = Object.freeze(${JSON.stringify(schema, null, 2)});\n` +
  `export const RENDER_BINARY_SCHEMA_VERSION = RENDER_BINARY_SCHEMA.version;\n` +
  `export const INSTANCE_STRIDE = RENDER_BINARY_SCHEMA.instance_stride_bytes;\n` +
  `export const DIRTY_STRIDE = RENDER_BINARY_SCHEMA.dirty_record_stride_bytes;\n` +
  `export const PATH_INSTANCE_STRIDE = RENDER_BINARY_SCHEMA.path_instance_stride_bytes;\n` +
  `export const PATH_VERTEX_STRIDE = RENDER_BINARY_SCHEMA.path_vertex_stride_bytes;\n` +
  `export const SHADER_SOURCE = ${JSON.stringify(shader)};\n`;
const destination = path.join(webRoot, "src", "render_contract.js");
if (process.argv.includes("--check")) {
  const current = await readFile(destination, "utf8").catch(() => "");
  if (current !== generated) {
    throw new Error("render_contract.js differs from the authoritative shared render contract");
  }
} else {
  await writeFile(destination, generated, "utf8");
}

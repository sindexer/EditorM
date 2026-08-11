import { cp, mkdir, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const dist = path.join(root, "dist");
await rm(dist, { recursive: true, force: true });
await mkdir(dist, { recursive: true });
for (const entry of ["index.html", "styles.css", "src", "pkg"]) {
  const source = path.join(root, entry);
  await stat(source);
  await cp(source, path.join(dist, entry), { recursive: true });
}
const required = [
  "index.html",
  "styles.css",
  "src/app.js",
  "src/worker.js",
  "src/renderer.js",
  "src/protocol.js",
  "src/render_contract.js",
  "pkg/engine_host.js",
  "pkg/engine_host_bg.wasm",
];
const files = [];
for (const relative of required) {
  const bytes = await readFile(path.join(dist, relative));
  files.push({
    path: relative,
    bytes: bytes.byteLength,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  });
}
await writeFile(
  path.join(dist, "build-manifest.json"),
  `${JSON.stringify({ phase: "0D", files }, null, 2)}\n`,
  "utf8",
);
console.log(`Phase 0D preview build: ${files.length} verified assets -> ${dist}`);
console.log(`dist entries: ${(await readdir(dist)).join(", ")}`);
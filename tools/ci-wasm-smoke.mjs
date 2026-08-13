import crypto from "node:crypto";
import fs from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

const root = path.resolve(import.meta.dirname, "..");
const approvedPkg = path.join(root, "web", "editor", "public", "pkg");
const pkgArgumentIndex = process.argv.indexOf("--pkg");
const pkg = pkgArgumentIndex >= 0 ? path.resolve(root, process.argv[pkgArgumentIndex + 1]) : approvedPkg;
const approvedMode = path.resolve(pkg) === path.resolve(approvedPkg);
const gluePath = path.join(pkg, "engine_host.js");
const wasmPath = path.join(pkg, "engine_host_bg.wasm");
const bytes = await fs.readFile(wasmPath);
const sha256 = crypto.createHash("sha256").update(bytes).digest("hex");

if (approvedMode) {
  const expectedHash = "bbad837fbe7be3d21733d2318a596adec7798c45ef6fb300b54665672cd99367";
  const expectedBytes = 1229366;
  if (sha256 !== expectedHash || bytes.byteLength !== expectedBytes) {
    throw new Error(`approved WASM mismatch: sha256=${sha256}, bytes=${bytes.byteLength}`);
  }
}

const bindingFiles = ["engine_host.js", "engine_host.d.ts", "engine_host_bg.wasm.d.ts"];
const bindingChecks = {};
for (const name of bindingFiles) {
  const actual = await fs.readFile(path.join(pkg, name));
  const approved = await fs.readFile(path.join(approvedPkg, name));
  bindingChecks[name] = crypto.createHash("sha256").update(actual).digest("hex") ===
    crypto.createHash("sha256").update(approved).digest("hex");
}
if (Object.values(bindingChecks).some((value) => !value)) {
  throw new Error(`generated WASM bindings differ from the approved interface: ${JSON.stringify(bindingChecks)}`);
}

const module = new WebAssembly.Module(bytes);
const glue = await import(pathToFileURL(gluePath).href + `?ci=${Date.now()}`);
glue.initSync({ module });
const host = new glue.EngineHost();
const response = JSON.parse(host.handleJson(JSON.stringify({
  protocol_version: 1,
  request_id: "ci-wasm-smoke",
  type: "initialize",
})));
if (!response.ok) throw new Error(`WASM initialize failed: ${JSON.stringify(response.error)}`);
if (glue.EngineHost.protocolVersion() !== 1 || glue.EngineHost.renderBinarySchemaVersion() !== 1) {
  throw new Error("generated WASM protocol or render binary schema version drifted");
}

console.log(JSON.stringify({
  mode: approvedMode ? "approved-checked-in-package" : "fresh-pinned-build",
  wasm_sha256: sha256,
  wasm_bytes: bytes.byteLength,
  binding_checks: bindingChecks,
  webassembly_module_created: module instanceof WebAssembly.Module,
  engine_host_created: host instanceof glue.EngineHost,
  protocol_version: glue.EngineHost.protocolVersion(),
  render_binary_schema_version: glue.EngineHost.renderBinarySchemaVersion(),
  initialize_ok: response.ok,
  byte_identity_claimed: approvedMode,
  all_passed: true,
}));

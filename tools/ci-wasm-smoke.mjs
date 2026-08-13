import crypto from "node:crypto";
import fs from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

const root = path.resolve(import.meta.dirname, "..");
const checkedInPkg = path.join(root, "web", "editor", "public", "pkg");
const pkgArgumentIndex = process.argv.indexOf("--pkg");
const pkg = pkgArgumentIndex >= 0 ? path.resolve(root, process.argv[pkgArgumentIndex + 1]) : checkedInPkg;
const checkedInMode = path.resolve(pkg) === path.resolve(checkedInPkg);
const gluePath = path.join(pkg, "engine_host.js");
const wasmPath = path.join(pkg, "engine_host_bg.wasm");
const bytes = await fs.readFile(wasmPath);
const sha256 = crypto.createHash("sha256").update(bytes).digest("hex");
const renderSchema = JSON.parse(
  await fs.readFile(path.join(root, "shared", "render_binary_schema.json"), "utf8"),
);

if (
  bytes.byteLength < 8 ||
  bytes[0] !== 0x00 ||
  bytes[1] !== 0x61 ||
  bytes[2] !== 0x73 ||
  bytes[3] !== 0x6d
) {
  throw new Error(`checked WASM package is not a valid WebAssembly binary: sha256=${sha256}, bytes=${bytes.byteLength}`);
}

const bindingFiles = ["engine_host.js", "engine_host.d.ts", "engine_host_bg.wasm.d.ts"];
const bindingChecks = {};
for (const name of bindingFiles) {
  const actual = await fs.readFile(path.join(pkg, name));
  const checkedIn = await fs.readFile(path.join(checkedInPkg, name));
  bindingChecks[name] = crypto.createHash("sha256").update(actual).digest("hex") ===
    crypto.createHash("sha256").update(checkedIn).digest("hex");
}
if (Object.values(bindingChecks).some((value) => !value)) {
  throw new Error(`generated WASM bindings differ from the checked-in interface: ${JSON.stringify(bindingChecks)}`);
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
if (
  glue.EngineHost.protocolVersion() !== 1 ||
  glue.EngineHost.renderBinarySchemaVersion() !== renderSchema.version
) {
  throw new Error(
    `generated WASM protocol or render binary schema version drifted: protocol=${glue.EngineHost.protocolVersion()}, render=${glue.EngineHost.renderBinarySchemaVersion()}, expected_render=${renderSchema.version}`,
  );
}

console.log(JSON.stringify({
  mode: checkedInMode ? "checked-in-package" : "fresh-pinned-build",
  wasm_sha256: sha256,
  wasm_bytes: bytes.byteLength,
  binding_checks: bindingChecks,
  webassembly_module_created: module instanceof WebAssembly.Module,
  engine_host_created: host instanceof glue.EngineHost,
  protocol_version: glue.EngineHost.protocolVersion(),
  render_binary_schema_version: glue.EngineHost.renderBinarySchemaVersion(),
  initialize_ok: response.ok,
  byte_identity_claimed: false,
  all_passed: true,
}));
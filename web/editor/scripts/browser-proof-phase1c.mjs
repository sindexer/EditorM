// Phase 1C final actual-browser Gate. This file is intentionally not part of the development
// test loop: it owns one fresh Chrome process and proves direct editing through real DOM input.
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, rm, stat, writeFile } from "node:fs/promises";
import { createServer as createNetServer } from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptRoot = path.dirname(fileURLToPath(import.meta.url));
const webRoot = path.resolve(scriptRoot, "..");
const workspace = path.resolve(webRoot, "../..");
const verificationRoot = path.join(workspace, "docs", "verification");
const proofPath = path.join(verificationRoot, "PHASE_1C_BROWSER_PROOF.json");
const gateRunId = process.env.PHASE1C_GATE_RUN_ID ?? "unbound-run";
const safeGateRunId = gateRunId.replace(/[^a-zA-Z0-9._-]+/g, "-");
const failurePath = path.join(verificationRoot, `PHASE_1C_BROWSER_FAILURE_${safeGateRunId}.json`);
const screenshotPath = path.join(verificationRoot, "phase1c-direct-editing.png");
const wasmPath = path.join(webRoot, "public", "pkg", "engine_host_bg.wasm");
const chrome = process.env.PHASE0E_CHROME ?? "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";
const profile = path.join(os.tmpdir(), `phase1c-chrome-${process.pid}-${Date.now()}`);
const allowSoftwareGpu = process.env.PHASE1C_ALLOW_SOFTWARE_GPU === "1";
const startedAt = new Date().toISOString();
const consoleErrors = [];
let chromeStderr = "";
let browserClient;
let pageClient;
let chromeProcess;
let preview;

class GateFailure extends Error {
  constructor(code, message, details = {}) {
    super(message);
    this.name = "GateFailure";
    this.code = code;
    this.details = details;
  }
}

class CdpClient {
  constructor(url) { this.url = url; this.nextId = 0; this.pending = new Map(); this.listeners = new Map(); }
  async connect() {
    this.socket = new WebSocket(this.url);
    await new Promise((resolve, reject) => {
      this.socket.addEventListener("open", resolve, { once: true });
      this.socket.addEventListener("error", reject, { once: true });
    });
    this.socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      if (message.id) {
        const pending = this.pending.get(message.id);
        if (!pending) return;
        this.pending.delete(message.id);
        if (message.error) pending.reject(new Error(`${message.error.code}: ${message.error.message}`));
        else pending.resolve(message.result);
        return;
      }
      for (const listener of this.listeners.get(message.method) ?? []) listener(message.params);
    });
  }
  send(method, params = {}) {
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }
  on(method, listener) {
    const listeners = this.listeners.get(method) ?? [];
    listeners.push(listener);
    this.listeners.set(method, listeners);
  }
  close() { this.socket?.close(); }
}

const sleep = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

async function reservePort() {
  return new Promise((resolve, reject) => {
    const listener = createNetServer();
    listener.once("error", reject);
    listener.listen(0, "127.0.0.1", () => {
      const address = listener.address();
      const port = typeof address === "object" && address ? address.port : 0;
      listener.close((error) => error ? reject(error) : resolve(port));
    });
  });
}

async function waitForFile(file, timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try { return await readFile(file, "utf8"); } catch { await sleep(100); }
  }
  throw new GateFailure("chrome_debug_port_timeout", `Timed out waiting for ${file}`);
}

async function preparePreview() {
  const port = await reservePort();
  const url = `http://127.0.0.1:${port}/`;
  const state = { exited: false, exit_code: null, stdout: "", stderr: "" };
  const server = spawn(process.execPath, ["server.mjs", `--port=${port}`], {
    cwd: webRoot, stdio: ["ignore", "pipe", "pipe"], windowsHide: true,
  });
  for (const [stream, key] of [[server.stdout, "stdout"], [server.stderr, "stderr"]]) {
    stream.setEncoding("utf8");
    stream.on("data", (chunk) => { state[key] = `${state[key]}${chunk}`.slice(-10000); });
  }
  server.once("exit", (code) => { state.exited = true; state.exit_code = code; });
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    if (state.exited) throw new GateFailure("preview_server_exited", "Preview server exited", state);
    try {
      const response = await fetch(`${url}__health`, { signal: AbortSignal.timeout(1000) });
      const body = response.ok ? await response.json() : null;
      if (body?.ok === true) {
        const assets = {};
        for (const [name, mime] of [["worker.js", "text/javascript"], ["pkg/engine_host.js", "text/javascript"], ["pkg/engine_host_bg.wasm", "application/wasm"]]) {
          const asset = await fetch(`${url}${name}`);
          assets[name] = { status: asset.status, content_type: asset.headers.get("content-type") };
          if (!asset.ok || !assets[name].content_type?.startsWith(mime)) throw new GateFailure("asset_contract_failed", name, assets[name]);
        }
        return { url, server, state, health: body, assets };
      }
    } catch (error) {
      if (error instanceof GateFailure) throw error;
    }
    await sleep(100);
  }
  throw new GateFailure("preview_connection_failed", "Preview health endpoint was not ready", state);
}

async function evaluate(expression) {
  const result = await pageClient.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true, userGesture: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
  return result.result.value;
}

async function waitFor(expression, timeoutMs = 30000, code = "condition_timeout") {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try { if (await evaluate(`Boolean(${expression})`)) return; } catch (error) { lastError = error; }
    await sleep(50);
  }
  throw new GateFailure(code, `Timed out waiting for ${expression}`, { last_error: lastError?.message });
}

async function proof() { return evaluate("JSON.parse(JSON.stringify(window.__PHASE0E_PROOF__))"); }
async function send(type, payload = {}) { return evaluate(`window.__phase0eSend(${JSON.stringify(type)}, ${JSON.stringify(payload)})`); }
async function nodes() {
  const response = await send("get_ui_snapshot");
  return new Map(response.projection.upserts.map((node) => [node.id, node]));
}
async function selection() { return (await proof()).selection ?? []; }
async function boundsFor(selector) {
  const result = await evaluate(`(() => { const e=document.querySelector(${JSON.stringify(selector)}); if(!e)return null; const r=e.getBoundingClientRect(); return {x:r.left,y:r.top,width:r.width,height:r.height,center_x:r.left+r.width/2,center_y:r.top+r.height/2}; })()`);
  if (!result) throw new GateFailure("selector_missing", `Missing ${selector}`);
  return result;
}
async function clickPoint(point, modifiers = 0) {
  const x = point.x ?? point.center_x; const y = point.y ?? point.center_y;
  await pageClient.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", buttons: 1, clickCount: 1, modifiers });
  await pageClient.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", buttons: 0, clickCount: 1, modifiers });
}
async function click(selector, modifiers = 0) { await clickPoint(await boundsFor(selector), modifiers); }
async function mouse(type, point, modifiers = 0) {
  await pageClient.send("Input.dispatchMouseEvent", { type, x: point.x, y: point.y, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1, modifiers });
}
async function drag(from, to, { modifiers = 0, ready = null, steps = 12 } = {}) {
  await mouse("mousePressed", from, modifiers);
  if (ready) await waitFor(ready, 30000, "gesture_did_not_start");
  for (let index = 1; index <= steps; index += 1) {
    await mouse("mouseMoved", { x: from.x + (to.x - from.x) * index / steps, y: from.y + (to.y - from.y) * index / steps }, modifiers);
  }
  await sleep(100);
  await mouse("mouseReleased", to, modifiers);
  await waitFor("window.__PHASE0E_PROOF__?.history?.transaction_active === false");
}
async function key(key, code, keyCode, modifiers = 0) {
  await pageClient.send("Input.dispatchKeyEvent", { type: "rawKeyDown", key, code, windowsVirtualKeyCode: keyCode, modifiers });
  await pageClient.send("Input.dispatchKeyEvent", { type: "keyUp", key, code, windowsVirtualKeyCode: keyCode, modifiers });
}
async function replaceNumeric(label, value, waitForResponse = true) {
  const selector = `input[aria-label=${JSON.stringify(label)}]`;
  const before = (await proof()).engine_sequence;
  await click(selector);
  await key("a", "KeyA", 65, 2);
  await pageClient.send("Input.insertText", { text: String(value) });
  await key("Enter", "Enter", 13);
  if (waitForResponse) await waitFor(`window.__PHASE0E_PROOF__?.engine_sequence > ${before}`);
}
function center(bounds) { return [(bounds.min[0] + bounds.max[0]) / 2, (bounds.min[1] + bounds.max[1]) / 2]; }
async function clientForWorld(world) {
  const state = await proof(); const canvas = await boundsFor(".webgpu-canvas");
  return { x: canvas.x + (world[0] - state.camera.center[0]) * state.camera.zoom + state.camera.viewport[0] / 2, y: canvas.y + (world[1] - state.camera.center[1]) * state.camera.zoom + state.camera.viewport[1] / 2 };
}
async function clientForNode(id) { const node = (await nodes()).get(id); return clientForWorld(center(node.world_bounds)); }
async function createRectangle(slideBounds, index) {
  await click("[data-testid='tool-rectangle']");
  await waitFor("window.__PHASE0E_PROOF__?.tool === 'rectangle'", 30000, "rectangle_tool_not_active");
  const start = await clientForWorld([slideBounds.min[0] + 140 + index * 320, slideBounds.min[1] + 140 + index * 100]);
  const end = { x: start.x + 110, y: start.y + 70 };
  await drag(start, end, { ready: "window.__PHASE0E_PROOF__?.fsm === 'CreatingRectangle'", steps: 8 });
  await waitFor("window.__PHASE0E_PROOF__?.primary_node?.kind === 'rectangle'");
  return (await proof()).primary_node.id;
}
function rectDelta(before, after) { return [after.min[0] - before.min[0], after.min[1] - before.min[1]]; }
function rotationDegrees(node) { return Math.atan2(node.world_transform[2], node.world_transform[0]) * 180 / Math.PI; }
function close(left, right, epsilon = 1e-4) { return Math.abs(left - right) <= epsilon; }

try {
  await stat(chrome).catch(() => { throw new GateFailure("chrome_not_found", chrome); });
  preview = await preparePreview();
  chromeProcess = spawn(chrome, [
    "--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profile}`,
    "--no-first-run", "--no-default-browser-check", "--disable-background-networking",
    "--disable-component-update", "--disable-gpu-sandbox", "--enable-unsafe-webgpu",
    "--ignore-gpu-blocklist", ...(allowSoftwareGpu ? ["--enable-unsafe-swiftshader", "--use-angle=swiftshader", "--use-gl=angle"] : []),
    "--window-size=1600,1000", "about:blank",
  ], { stdio: ["ignore", "ignore", "pipe"], windowsHide: true });
  chromeProcess.stderr.setEncoding("utf8");
  chromeProcess.stderr.on("data", (chunk) => { chromeStderr = `${chromeStderr}${chunk}`.slice(-20000); });
  const debugPort = (await waitForFile(path.join(profile, "DevToolsActivePort"))).trim().split(/\r?\n/)[0];
  const debugBase = `http://127.0.0.1:${debugPort}`;
  const version = await (await fetch(`${debugBase}/json/version`)).json();
  browserClient = new CdpClient(version.webSocketDebuggerUrl); await browserClient.connect();
  const gpuInfo = await browserClient.send("SystemInfo.getInfo");
  const target = await (await fetch(`${debugBase}/json/new?${encodeURIComponent(preview.url)}`, { method: "PUT" })).json();
  pageClient = new CdpClient(target.webSocketDebuggerUrl); await pageClient.connect();
  pageClient.on("Runtime.exceptionThrown", ({ exceptionDetails }) => consoleErrors.push(exceptionDetails.exception?.description ?? exceptionDetails.text));
  pageClient.on("Runtime.consoleAPICalled", (entry) => { if (["error", "assert"].includes(entry.type)) consoleErrors.push(entry.args.map((item) => item.value ?? item.description).join(" ")); });
  pageClient.on("Log.entryAdded", ({ entry }) => { if (entry.level === "error") consoleErrors.push(entry.text); });
  await pageClient.send("Runtime.enable"); await pageClient.send("Page.enable"); await pageClient.send("Log.enable"); await pageClient.send("Page.bringToFront");
  await pageClient.send("Emulation.setDeviceMetricsOverride", { width: 1600, height: 1000, deviceScaleFactor: 1, mobile: false });
  await waitFor("document.body?.dataset.ready === 'true' && window.__PHASE0E_PROOF__?.actual_webgpu === true && window.__PHASE0E_PROOF__?.heartbeat >= 1", 45000, "application_not_ready");
  const initial = await proof();
  const initialSnapshot = await send("get_ui_snapshot");
  const activeDevice = gpuInfo.gpu?.devices?.find((device) => device.active) ?? gpuInfo.gpu?.devices?.[0] ?? {};
  const adapter = String(initial.adapter ?? activeDevice.deviceString ?? "");
  const software = /swiftshader|llvmpipe|software|lavapipe|basic render/i.test(`${adapter} ${activeDevice.deviceString ?? ""}`);
  if (software && !allowSoftwareGpu) throw new GateFailure("software_gpu_rejected", adapter, activeDevice);
  const beforeFitSequence = (await proof()).engine_sequence;
  await send("camera", { camera: { kind: "fit" } });
  await waitFor(`window.__PHASE0E_PROOF__?.engine_sequence > ${beforeFitSequence}`, 30000, "fit_document_failed");
  const initialNodes = await nodes();
  const slide = initialSnapshot.active_root;
  const slideNode = initialNodes.get(slide);
  if (!slideNode || slideNode.kind !== "frame") throw new GateFailure("active_slide_missing", String(slide));
  const slideRootExcluded = await evaluate(`!document.querySelector('[data-node-id=${JSON.stringify(slide)}]')`);

  const rectangles = [await createRectangle(slideNode.world_bounds, 0), await createRectangle(slideNode.world_bounds, 1), await createRectangle(slideNode.world_bounds, 2)];
  await click("[data-testid='tool-select']");
  await waitFor("window.__PHASE0E_PROOF__?.tool === 'select'", 30000, "select_tool_not_active");
  await clickPoint(await clientForNode(rectangles[0]));
  await clickPoint(await clientForNode(rectangles[1]), 8);
  await waitFor("window.__PHASE0E_PROOF__?.selection_count === 2");
  const shiftSelection = await selection();
  const layersSynchronized = await evaluate("document.querySelectorAll('.layer-row.is-selected').length === 2");

  const tableForMarquee = await nodes();
  const firstBounds = tableForMarquee.get(rectangles[0]).world_bounds;
  const secondBounds = tableForMarquee.get(rectangles[1]).world_bounds;
  const bandStart = await clientForWorld([firstBounds.min[0] - 25, firstBounds.min[1] - 25]);
  const bandEnd = await clientForWorld([secondBounds.max[0] + 25, secondBounds.max[1] + 25]);
  await drag(bandStart, bandEnd, { ready: "window.__PHASE0E_PROOF__?.fsm === 'MarqueeSelecting'" });
  await waitFor("window.__PHASE0E_PROOF__?.selection_count === 2");
  const marqueeSelection = await selection();

  const moveBeforeNodes = await nodes(); const moveHistory = (await proof()).history.undo_depth;
  const moveFrom = await clientForNode(rectangles[0]); const moveTo = { x: moveFrom.x + 42, y: moveFrom.y + 24 };
  await drag(moveFrom, moveTo, { ready: "window.__PHASE0E_PROOF__?.fsm === 'Moving'" });
  const moveAfterNodes = await nodes();
  const moveDeltas = rectangles.slice(0, 2).map((id) => rectDelta(moveBeforeNodes.get(id).world_bounds, moveAfterNodes.get(id).world_bounds));
  const moveOneUndo = (await proof()).history.undo_depth === moveHistory + 1;
  await key("z", "KeyZ", 90, 2); await waitFor(`window.__PHASE0E_PROOF__?.history?.undo_depth === ${moveHistory}`);

  const escapeBefore = await nodes(); const escapeHistory = (await proof()).history.undo_depth;
  const escapeFrom = await clientForNode(rectangles[0]);
  await mouse("mousePressed", escapeFrom); await waitFor("window.__PHASE0E_PROOF__?.fsm === 'Moving'");
  await mouse("mouseMoved", { x: escapeFrom.x + 50, y: escapeFrom.y + 20 }); await sleep(120);
  await key("Escape", "Escape", 27); await mouse("mouseReleased", { x: escapeFrom.x + 50, y: escapeFrom.y + 20 });
  await waitFor("window.__PHASE0E_PROOF__?.history?.transaction_active === false");
  const escapeAfter = await nodes();
  const escapeRestored = rectangles.slice(0, 2).every((id) => JSON.stringify(escapeBefore.get(id).world_transform) === JSON.stringify(escapeAfter.get(id).world_transform)) && (await proof()).history.undo_depth === escapeHistory;

  const resizeBefore = await nodes(); const resizeHistory = (await proof()).history.undo_depth;
  const resizeHandle = await boundsFor("[data-resize-handle='se']");
  await drag({ x: resizeHandle.center_x, y: resizeHandle.center_y }, { x: resizeHandle.center_x + 70, y: resizeHandle.center_y + 45 }, { ready: "window.__PHASE0E_PROOF__?.fsm === 'Resizing'" });
  const resizeAfter = await nodes();
  const resizeChangedEveryNode = rectangles.slice(0, 2).every((id) => {
    const before = resizeBefore.get(id).world_bounds; const after = resizeAfter.get(id).world_bounds;
    return after.max[0] - after.min[0] > before.max[0] - before.min[0] && after.max[1] - after.min[1] > before.max[1] - before.min[1];
  });
  const resizeOneUndo = (await proof()).history.undo_depth === resizeHistory + 1;
  await key("z", "KeyZ", 90, 2); await waitFor(`window.__PHASE0E_PROOF__?.history?.undo_depth === ${resizeHistory}`);

  const rotateBefore = await nodes(); const rotateHistory = (await proof()).history.undo_depth;
  const rotateHandle = await boundsFor("[data-testid='rotate-handle']"); const union = await boundsFor("[data-testid='selection-union']");
  const radius = Math.hypot(rotateHandle.center_x - union.center_x, rotateHandle.center_y - union.center_y);
  await drag({ x: rotateHandle.center_x, y: rotateHandle.center_y }, { x: union.center_x + radius, y: union.center_y }, { ready: "window.__PHASE0E_PROOF__?.fsm === 'Rotating'" });
  const rotateAfter = await nodes();
  const clockwisePositive = rectangles.slice(0, 2).every((id) => close(rotationDegrees(rotateAfter.get(id)) - rotationDegrees(rotateBefore.get(id)), 90, 2));
  const rotateOneUndo = (await proof()).history.undo_depth === rotateHistory + 1;
  await key("z", "KeyZ", 90, 2); await waitFor(`window.__PHASE0E_PROOF__?.history?.undo_depth === ${rotateHistory}`);
  const shiftHandle = await boundsFor("[data-testid='rotate-handle']"); const shiftUnion = await boundsFor("[data-testid='selection-union']");
  const shiftRadius = Math.hypot(shiftHandle.center_x - shiftUnion.center_x, shiftHandle.center_y - shiftUnion.center_y);
  const angle = -Math.PI / 4;
  await drag({ x: shiftHandle.center_x, y: shiftHandle.center_y }, { x: shiftUnion.center_x + Math.cos(angle) * shiftRadius, y: shiftUnion.center_y + Math.sin(angle) * shiftRadius }, { modifiers: 8, ready: "window.__PHASE0E_PROOF__?.fsm === 'Rotating'" });
  const shifted = await nodes(); const shiftRotation = rotationDegrees(shifted.get(rectangles[0]));
  const rotationShift15 = close(shiftRotation / 15, Math.round(shiftRotation / 15), 1e-5);
  await key("z", "KeyZ", 90, 2); await waitFor(`window.__PHASE0E_PROOF__?.history?.undo_depth === ${rotateHistory}`);

  const alignHistory = (await proof()).history.undo_depth; await click("[data-testid='align-left']"); await waitFor(`window.__PHASE0E_PROOF__?.history?.undo_depth === ${alignHistory + 1}`);
  const aligned = await nodes(); const alignmentExact = close(aligned.get(rectangles[0]).world_bounds.min[0], aligned.get(rectangles[1]).world_bounds.min[0]);
  await key("z", "KeyZ", 90, 2); await waitFor(`window.__PHASE0E_PROOF__?.history?.undo_depth === ${alignHistory}`);
  await clickPoint(await clientForNode(rectangles[2]), 8); await waitFor("window.__PHASE0E_PROOF__?.selection_count === 3");
  const distributeHistory = (await proof()).history.undo_depth; await click("[data-testid='distribute-horizontal']"); await waitFor(`window.__PHASE0E_PROOF__?.history?.undo_depth === ${distributeHistory + 1}`);
  const distributed = await nodes(); const ordered = rectangles.map((id) => distributed.get(id).world_bounds).sort((a, b) => a.min[0] - b.min[0]);
  const gaps = [ordered[1].min[0] - ordered[0].max[0], ordered[2].min[0] - ordered[1].max[0]];
  const distributionExact = close(gaps[0], gaps[1]);

  const preGroup = await nodes(); const groupHistory = (await proof()).history.undo_depth;
  await key("g", "KeyG", 71, 2); await waitFor("window.__PHASE0E_PROOF__?.primary_node?.kind === 'group'");
  const groupProof = await proof(); const groupId = groupProof.primary_node.id; const grouped = await nodes();
  const groupPreserved = rectangles.every((id) => JSON.stringify(grouped.get(id).world_transform) === JSON.stringify(preGroup.get(id).world_transform)) && grouped.get(groupId).world_bounds != null && groupProof.history.undo_depth === groupHistory + 1;
  const hierarchyVisible = await evaluate(`Boolean(document.querySelector('[data-node-id=${JSON.stringify(groupId)}]'))`);
  await key("g", "KeyG", 71, 10); await waitFor("window.__PHASE0E_PROOF__?.selection_count === 3 && window.__PHASE0E_PROOF__?.primary_node?.kind !== 'group'");
  const ungrouped = await nodes(); const ungroupPreserved = rectangles.every((id) => JSON.stringify(ungrouped.get(id).world_transform) === JSON.stringify(preGroup.get(id).world_transform)) && !ungrouped.has(groupId) && (await proof()).history.undo_depth === groupHistory + 2;

  await clickPoint(await clientForNode(rectangles[0])); await clickPoint(await clientForNode(rectangles[1]), 8); await waitFor("window.__PHASE0E_PROOF__?.selection_count === 2");
  const mixedShown = await evaluate("document.querySelector(\"input[aria-label='X']\")?.dataset.mixed === 'true'");
  const mixedHistory = (await proof()).history.undo_depth; await replaceNumeric("X", 320); await waitFor(`window.__PHASE0E_PROOF__?.history?.undo_depth === ${mixedHistory + 1}`);
  const mixedAppliedNodes = await nodes(); const mixedApplied = rectangles.slice(0, 2).every((id) => close(mixedAppliedNodes.get(id).local_transform[4], 320));
  const invalidRevision = (await proof()).revisions.document; const invalidSequence = (await proof()).engine_sequence;
  await replaceNumeric("W", "NaN", false); await sleep(150);
  const invalidFailClosed = await evaluate("document.querySelector(\"input[aria-label='W']\")?.dataset.validationCode === 'numeric_non_finite'") && (await proof()).revisions.document === invalidRevision && (await proof()).engine_sequence === invalidSequence;

  const dprZoomMatrix = [];
  for (const [dpr, zoom] of [[1, 0.5], [2, 2]]) {
    await pageClient.send("Emulation.setDeviceMetricsOverride", { width: 1600, height: 1000, deviceScaleFactor: dpr, mobile: false });
    const canvasMetrics = await boundsFor(".webgpu-canvas");
    await send("camera", { camera: { kind: "resize", width: canvasMetrics.width, height: canvasMetrics.height, dpr } });
    await waitFor(`window.__PHASE0E_PROOF__?.camera?.dpr === ${dpr}`, 30000, "camera_dpr_not_applied");
    const camera = await proof();
    await send("camera", { camera: { kind: "zoom", x: camera.camera.viewport[0] / 2, y: camera.camera.viewport[1] / 2, zoom } });
    const matrixNodes = await nodes(); const first = matrixNodes.get(rectangles[0]); const anchor = matrixNodes.get(rectangles[1]);
    await send("selection", { mode: "replace", target: rectangles[0] });
    const matrixHistory = (await proof()).history.undo_depth;
    const from = await clientForNode(rectangles[0]); const requestedCss = (anchor.world_bounds.min[0] - first.world_bounds.min[0]) * zoom - 5;
    await drag(from, { x: from.x + requestedCss, y: from.y }, { ready: "window.__PHASE0E_PROOF__?.fsm === 'Moving'" });
    const snapped = await nodes(); const error = snapped.get(rectangles[0]).world_bounds.min[0] - snapped.get(rectangles[1]).world_bounds.min[0];
    dprZoomMatrix.push({ dpr, zoom, engine_dpr: (await proof()).camera.dpr, snap_error_world: error, passed: close(error, 0) });
    if ((await proof()).history.undo_depth > matrixHistory) {
      await key("z", "KeyZ", 90, 2);
      await waitFor(`window.__PHASE0E_PROOF__?.history?.undo_depth === ${matrixHistory}`);
    }
  }

  await pageClient.send("Emulation.setDeviceMetricsOverride", { width: 1600, height: 1000, deviceScaleFactor: 1, mobile: false });
  const screenshot = await pageClient.send("Page.captureScreenshot", { format: "png", captureBeyondViewport: false, fromSurface: true });
  await writeFile(screenshotPath, Buffer.from(screenshot.data, "base64"));
  const final = await proof(); const wasm = await readFile(wasmPath);
  const checks = {
    actual_chrome: Boolean(chromeProcess.pid) && version.Browser.includes("Chrome"),
    actual_nvidia_gtx_970: !software && /GTX 970/i.test(`${adapter} ${activeDevice.deviceString ?? ""}`),
    dedicated_worker_wasm: initial.worker_runtime_owner === "dedicated-worker" && initial.wasm_initialized === true,
    actual_webgpu_no_fallback: initial.actual_webgpu === true && Number(final.fallback_rebuild_count ?? 0) === 0,
    active_slide_root_excluded: slideRootExcluded,
    shift_click_selection: shiftSelection.length === 2,
    layers_canvas_synchronized: layersSynchronized,
    marquee_selection: marqueeSelection.length === 2,
    multi_move_relative_position_and_one_undo: moveOneUndo && close(moveDeltas[0][0], moveDeltas[1][0]) && close(moveDeltas[0][1], moveDeltas[1][1]),
    escape_rolls_back: escapeRestored,
    aggregate_resize_and_one_undo: resizeChangedEveryNode && resizeOneUndo,
    clockwise_rotation_positive_and_one_undo: clockwisePositive && rotateOneUndo,
    shift_rotation_15_degrees: rotationShift15,
    alignment_one_undo: alignmentExact,
    distribution_equal_gaps: distributionExact,
    group_and_ungroup_preserve_world: groupPreserved && ungroupPreserved && hierarchyVisible,
    inspector_mixed_apply: mixedShown && mixedApplied,
    invalid_numeric_fail_closed: invalidFailClosed,
    dpr_zoom_snap_matrix: dprZoomMatrix.every((entry) => entry.passed),
    console_errors_zero: consoleErrors.length === 0,
    gpu_validation_errors_zero: Number(final.gpu_validation_errors ?? final.gpu?.validation_errors ?? 0) === 0,
    response_gpu_overlay_sequences_match: final.response_gpu_overlay_sequence_match === true,
  };
  const result = {
    phase: "1C", proof_kind: allowSoftwareGpu ? "software-gpu-diagnostic-not-gate-evidence" : "actual-hardware-browser",
    gate_run_id: gateRunId,
    tested_source_commit: process.env.PHASE1C_GATE_SOURCE_COMMIT ?? null,
    tested_branch: process.env.PHASE1C_GATE_SOURCE_BRANCH ?? null,
    started_at_utc: startedAt, finished_at_utc: new Date().toISOString(),
    command: "npm run test:browser:phase1c", browser: version.Browser,
    gpu: { adapter, active_device: activeDevice, software_renderer: software },
    wasm: { path: "web/editor/public/pkg/engine_host_bg.wasm", bytes: wasm.length, sha256: createHash("sha256").update(wasm).digest("hex"), protocol_version: 1, render_binary_schema_version: 2 },
    preview: { url: preview.url, health: preview.health, assets: preview.assets },
    interactions: { rectangles, move_deltas: moveDeltas, rotation_degrees: rectangles.slice(0, 2).map((id) => rotationDegrees(rotateAfter.get(id))), distribution_gaps: gaps, dpr_zoom_matrix: dprZoomMatrix },
    screenshot: { path: path.relative(workspace, screenshotPath).replaceAll("\\", "/"), bytes: (await stat(screenshotPath)).size },
    console_errors: consoleErrors, chrome_stderr_tail: chromeStderr, checks,
    fail_count: Object.values(checks).filter((passed) => !passed).length,
    unverified_count: 0, all_passed: Object.values(checks).every(Boolean),
  };
  await writeFile(proofPath, `${JSON.stringify(result, null, 2)}\n`, "utf8");
  console.log(`Phase 1C actual browser proof: ${result.all_passed ? "PASS" : "FAIL"}`);
  console.log(`gpu=${adapter}`); console.log(`proof=${proofPath}`);
  if (!result.all_passed) { console.log(JSON.stringify(checks, null, 2)); process.exitCode = 1; }
} catch (error) {
  const browser_diagnostics = pageClient ? await evaluate(`(() => {
    const shell = document.querySelector('[data-testid="canvas-shell"]')?.getBoundingClientRect();
    return {
      tool: window.__PHASE0E_PROOF__?.tool ?? null,
      fsm: window.__PHASE0E_PROOF__?.fsm ?? null,
      camera: window.__PHASE0E_PROOF__?.camera ?? null,
      shell: shell ? { x: shell.left, y: shell.top, width: shell.width, height: shell.height } : null,
    };
  })()`).catch((diagnosticError) => ({ diagnostic_error: diagnosticError.message })) : null;
  const failure = {
    phase: "1C", proof_kind: "actual-browser-gate-failure",
    gate_run_id: gateRunId,
    tested_source_commit: process.env.PHASE1C_GATE_SOURCE_COMMIT ?? null,
    tested_branch: process.env.PHASE1C_GATE_SOURCE_BRANCH ?? null,
    execution: { command: "npm run test:browser:phase1c", started_at_utc: startedAt, finished_at_utc: new Date().toISOString(), exit_status: 1 },
    code: error.code ?? "unexpected_error", message: error.message,
    details: error.details ?? null, browser_diagnostics,
    console_errors: consoleErrors, chrome_stderr_tail: chromeStderr,
  };
  await writeFile(failurePath, `${JSON.stringify(failure, null, 2)}\n`, "utf8").catch(() => undefined);
  console.error(`Phase 1C browser proof failed: ${failure.code}: ${failure.message}`);
  process.exitCode = 1;
} finally {
  pageClient?.close(); browserClient?.close();
  if (chromeProcess && !chromeProcess.killed) chromeProcess.kill();
  if (preview?.server && !preview.server.killed) preview.server.kill();
  await rm(profile, { recursive: true, force: true }).catch(() => undefined);
}

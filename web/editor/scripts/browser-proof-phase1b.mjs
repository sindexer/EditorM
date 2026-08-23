// Phase 1B actual-browser proof.
//
// Structure follows scripts/browser-proof-phase1a.mjs: this harness owns port allocation, the
// preview server, a new Chrome process with a throwaway profile, the CDP session, evidence
// capture, and cleanup. Everything it asserts is driven through real pointer and keyboard input
// against the built Editor, so every behaviour crosses React -> Dedicated Worker -> WASM
// EngineHost -> actual WebGPU. Nothing here is verified against React state alone, and there is
// no mock, Canvas2D, or replayed-JSON substitute for the engine.
//
// Two kinds of pixel evidence are produced:
//   * WebGPU surface readback through window.__phase0eReadPixel, for document content;
//   * composited screenshot sampling, for the SVG interaction overlay (selection outline, union
//     bounds, rubber band, snap guide), which is not part of the WebGPU surface. The screenshot
//     is decoded with a 2D canvas purely to measure it; the document itself is never rendered
//     with Canvas2D.

import { spawn } from "node:child_process";
import { readFile, rm, stat, writeFile } from "node:fs/promises";
import { createServer as createNetServer } from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptRoot = path.dirname(fileURLToPath(import.meta.url));
const webRoot = path.resolve(scriptRoot, "..");
const workspace = path.resolve(webRoot, "../..");
const verificationRoot = path.join(workspace, "docs", "verification");
const gateEvidence = {
  gate_run_id: process.env.PHASE1B_GATE_RUN_ID ?? null,
  tested_source_commit: process.env.PHASE1B_GATE_SOURCE_COMMIT ?? null,
  tested_branch: process.env.PHASE1B_GATE_SOURCE_BRANCH ?? null,
};
const allowSoftwareGpu = process.env.PHASE1B_ALLOW_SOFTWARE_GPU === "1";
// A software-GPU run can exercise the harness end to end, but it is never gate evidence, so it
// is written to separate diagnostic paths and never overwrites the hardware proof artifacts.
const proofPath = path.join(
  verificationRoot,
  allowSoftwareGpu ? "PHASE_1B_BROWSER_SOFTWARE_RUN.json" : "PHASE_1B_BROWSER_PROOF.json",
);
const pixelPath = path.join(
  verificationRoot,
  allowSoftwareGpu
    ? "PHASE_1B_PIXEL_READBACK_SOFTWARE_RUN.json"
    : "PHASE_1B_PIXEL_READBACK.json",
);
const failurePath = path.join(
  verificationRoot,
  allowSoftwareGpu
    ? "PHASE_1B_BROWSER_SOFTWARE_RUN_FAILURE.json"
    : "PHASE_1B_BROWSER_FAILURE.json",
);
const metricsPath = path.join(
  workspace,
  "docs",
  allowSoftwareGpu ? "PHASE_1B_METRICS_SOFTWARE_RUN.json" : "PHASE_1B_METRICS.json",
);
const screenshots = {
  multi_selection: path.join(verificationRoot, "phase1b-multi-selection.png"),
  marquee: path.join(verificationRoot, "phase1b-marquee.png"),
  snap_guide: path.join(verificationRoot, "phase1b-snap-guide.png"),
  aligned: path.join(verificationRoot, "phase1b-aligned.png"),
  distributed: path.join(verificationRoot, "phase1b-distributed.png"),
};
const chrome =
  process.env.PHASE0E_CHROME ??
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";
const profile = path.join(
  os.tmpdir(),
  `phase1b-chrome-${process.pid}-${Date.now()}`,
);

const PRIMARY_RGB = [0, 102, 255];
const GUIDE_RGB = [255, 45, 85];
const SNAP_THRESHOLD_PX = 8;
const WORLD_EPSILON = 1e-6;
// Chrome may quantise dispatched pointer coordinates to whole CSS pixels, so any assertion about
// a requested drag distance is compared in world units scaled from a 1.5 CSS-pixel allowance.
// Assertions that must be exact (snapped edges, undo restoration, equal deltas between nodes)
// use WORLD_EPSILON instead and are unaffected by pointer quantisation.
const POINTER_TOLERANCE_CSS_PX = 1.5;

const consoleErrors = [];
let chromeStderr = "";
let browserClient;
let pageClient;
let chromeProcess;
let preview;
let maxFallbackRebuildCountSeen = 0;
const fallbackCheckpoints = [];
const pixelCases = [];
const browserProofStartedAtUtc = new Date().toISOString();

function fixtureNodeId(index) {
  const hex = (BigInt(index) + 1n).toString(16).padStart(32, "0");
  return (
    hex.slice(0, 8) +
    "-" +
    hex.slice(8, 12) +
    "-" +
    hex.slice(12, 16) +
    "-" +
    hex.slice(16, 20) +
    "-" +
    hex.slice(20)
  );
}

class HarnessFailure extends Error {
  constructor(code, message, details = {}) {
    super(message);
    this.name = "HarnessFailure";
    this.code = code;
    this.details = details;
  }
}

class CdpClient {
  constructor(url) {
    this.url = url;
    this.nextId = 0;
    this.pending = new Map();
    this.listeners = new Map();
  }

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
        if (message.error)
          pending.reject(
            new Error(`${message.error.code}: ${message.error.message}`),
          );
        else pending.resolve(message.result);
        return;
      }
      for (const listener of this.listeners.get(message.method) ?? [])
        listener(message.params);
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
    const current = this.listeners.get(method) ?? [];
    current.push(listener);
    this.listeners.set(method, current);
  }

  close() {
    this.socket?.close();
  }
}

function sleep(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function reservePort() {
  return new Promise((resolve, reject) => {
    const listener = createNetServer();
    listener.once("error", reject);
    listener.listen(0, "127.0.0.1", () => {
      const address = listener.address();
      const port = typeof address === "object" && address ? address.port : 0;
      listener.close((error) => (error ? reject(error) : resolve(port)));
    });
  });
}

async function waitForFile(file, timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      return await readFile(file, "utf8");
    } catch {
      await sleep(100);
    }
  }
  throw new HarnessFailure(
    "chrome_debug_port_timeout",
    `Timed out waiting for ${file}`,
  );
}

async function waitForHealth(url, processState, timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    if (processState.exited) {
      throw new HarnessFailure(
        "preview_server_exited",
        "Preview server exited before health check",
        processState,
      );
    }
    try {
      const response = await fetch(new URL("/__health", url), {
        signal: AbortSignal.timeout(1000),
      });
      if (!response.ok) {
        throw new HarnessFailure(
          "preview_health_http_error",
          `Health returned HTTP ${response.status}`,
        );
      }
      const body = await response.json();
      if (body.ok === true && body.phase === "0E")
        return { ok: true, status: response.status, body };
      throw new HarnessFailure(
        "preview_health_payload_error",
        "Health payload was not the editor preview",
        { body },
      );
    } catch (error) {
      if (error instanceof HarnessFailure) throw error;
      lastError = error;
      await sleep(100);
    }
  }
  throw new HarnessFailure(
    "preview_connection_failed",
    "Preview health endpoint did not become reachable",
    { last_error: lastError?.message, server: processState },
  );
}

async function verifyAssets(url) {
  const expected = [
    ["/index.html", "text/html"],
    ["/worker.js", "text/javascript"],
    ["/pkg/engine_host.js", "text/javascript"],
    ["/pkg/engine_host_bg.wasm", "application/wasm"],
  ];
  const result = {};
  for (const [pathname, mime] of expected) {
    let response;
    try {
      response = await fetch(new URL(pathname, url), {
        signal: AbortSignal.timeout(3000),
      });
    } catch (error) {
      throw new HarnessFailure(
        "asset_connection_failed",
        `${pathname} could not be fetched`,
        { message: error.message },
      );
    }
    const contentType = response.headers.get("content-type") ?? "";
    if (!response.ok) {
      throw new HarnessFailure(
        "asset_http_error",
        `${pathname} returned HTTP ${response.status}`,
        { pathname, status: response.status },
      );
    }
    if (!contentType.startsWith(mime)) {
      throw new HarnessFailure(
        "asset_mime_error",
        `${pathname} returned ${contentType}`,
        { pathname, expected: mime, actual: contentType },
      );
    }
    result[pathname] = { status: response.status, content_type: contentType };
  }
  return result;
}

async function preparePreview() {
  const port = await reservePort();
  const url = new URL(`http://127.0.0.1:${port}/`);
  const processState = { exited: false, exit_code: null, stdout: "", stderr: "" };
  const server = spawn(process.execPath, ["server.mjs", `--port=${port}`], {
    cwd: webRoot,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  server.stdout.setEncoding("utf8");
  server.stderr.setEncoding("utf8");
  server.stdout.on("data", (chunk) => {
    processState.stdout = `${processState.stdout}${chunk}`.slice(-20000);
  });
  server.stderr.on("data", (chunk) => {
    processState.stderr = `${processState.stderr}${chunk}`.slice(-20000);
  });
  server.once("exit", (code) => {
    processState.exited = true;
    processState.exit_code = code;
  });
  try {
    const health = await waitForHealth(url, processState);
    const assets = await verifyAssets(url);
    return { url: url.href, port, server, processState, health, assets };
  } catch (error) {
    server.kill();
    throw error;
  }
}

async function evaluate(expression) {
  const result = await pageClient.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
    userGesture: true,
  });
  if (result.exceptionDetails) {
    throw new Error(
      result.exceptionDetails.exception?.description ??
        result.exceptionDetails.text,
    );
  }
  return result.result.value;
}

async function waitFor(expression, timeoutMs = 30000, code = "condition_timeout") {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      if (await evaluate(`Boolean(${expression})`)) return;
    } catch (error) {
      lastError = error;
    }
    await sleep(50);
  }
  throw new HarnessFailure(code, `Timed out waiting for ${expression}`, {
    last_error: lastError?.message,
  });
}

async function waitForReady() {
  const deadline = Date.now() + 45000;
  let status;
  while (Date.now() < deadline) {
    status = await evaluate(`({
      body_ready: document.body?.dataset.ready ?? null,
      state: window.__phase0eState ?? null,
      proof: window.__PHASE0E_PROOF__ ?? null,
      navigator_gpu: Boolean(navigator.gpu),
    })`).catch((error) => ({ evaluation_error: error.message }));
    if (status?.state?.lastError) {
      const message = String(status.state.lastError);
      const code =
        message.includes("webgpu") ||
        message.includes("adapter") ||
        message.includes("device")
          ? "webgpu_initialization_failed"
          : message.includes("wasm")
            ? "wasm_initialization_failed"
            : message.includes("worker")
              ? "worker_initialization_failed"
              : "application_initialization_failed";
      throw new HarnessFailure(code, message, status);
    }
    if (
      status?.body_ready === "true" &&
      status?.navigator_gpu === true &&
      status?.proof?.actual_webgpu === true &&
      status?.proof?.heartbeat >= 1 &&
      status?.proof?.response_gpu_overlay_sequence_match === true
    )
      return status;
    await sleep(100);
  }
  if (status?.navigator_gpu === false) {
    throw new HarnessFailure(
      "webgpu_unavailable",
      "navigator.gpu was not exposed before the readiness deadline",
      status,
    );
  }
  throw new HarnessFailure(
    "application_readiness_timeout",
    "Worker/WASM/WebGPU app did not become ready",
    status,
  );
}

async function getProof() {
  const proof = await evaluate(
    "JSON.parse(JSON.stringify(window.__PHASE0E_PROOF__))",
  );
  const fallback = Number(proof?.fallback_rebuild_count ?? 0);
  maxFallbackRebuildCountSeen = Math.max(maxFallbackRebuildCountSeen, fallback);
  fallbackCheckpoints.push({
    engine_sequence: proof?.engine_sequence ?? null,
    fixture: proof?.fixture ?? null,
    fallback_rebuild_count: fallback,
  });
  if (fallback !== 0) {
    throw new HarnessFailure(
      "fallback_rebuild_detected",
      "A Scene/Render fallback rebuild occurred",
      { proof, max_fallback_rebuild_count_seen: maxFallbackRebuildCountSeen },
    );
  }
  return proof;
}

async function send(type, payload = {}) {
  return evaluate(
    `window.__phase0eSend(${JSON.stringify(type)}, ${JSON.stringify(payload)})`,
  );
}

async function boundsForSelector(selector) {
  const bounds = await evaluate(`(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    return { x: rect.left, y: rect.top, width: rect.width, height: rect.height,
      center_x: rect.left + rect.width / 2, center_y: rect.top + rect.height / 2 };
  })()`);
  if (!bounds)
    throw new HarnessFailure(
      "dom_selector_missing",
      `No element matched ${selector}`,
    );
  return bounds;
}

async function elementCount(selector) {
  return evaluate(
    `document.querySelectorAll(${JSON.stringify(selector)}).length`,
  );
}

async function clickPoint(point, { modifiers = 0, clickCount = 1 } = {}) {
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: point.center_x ?? point.x,
    y: point.center_y ?? point.y,
    button: "left",
    buttons: 1,
    clickCount,
    modifiers,
  });
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: point.center_x ?? point.x,
    y: point.center_y ?? point.y,
    button: "left",
    buttons: 0,
    clickCount,
    modifiers,
  });
}

async function clickSelector(selector, options) {
  await clickPoint(await boundsForSelector(selector), options);
}

async function boundsForText(selector, text) {
  const bounds = await evaluate(`(() => {
    const expected = ${JSON.stringify(text)};
    const element = [...document.querySelectorAll(${JSON.stringify(selector)})]
      .find((candidate) => candidate.textContent?.includes(expected));
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    return { x: rect.left, y: rect.top, width: rect.width, height: rect.height,
      center_x: rect.left + rect.width / 2, center_y: rect.top + rect.height / 2 };
  })()`);
  if (!bounds)
    throw new HarnessFailure("dom_text_missing", `No ${selector} contained ${text}`);
  return bounds;
}

async function clickText(selector, text, options) {
  await clickPoint(await boundsForText(selector, text), options);
}

async function mouseDown(point, modifiers = 0) {
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: point.x,
    y: point.y,
    button: "left",
    buttons: 1,
    clickCount: 1,
    modifiers,
  });
}

async function mouseMove(point, modifiers = 0) {
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x: point.x,
    y: point.y,
    button: "left",
    buttons: 1,
    modifiers,
  });
}

async function mouseUp(point, modifiers = 0) {
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: point.x,
    y: point.y,
    button: "left",
    buttons: 0,
    clickCount: 1,
    modifiers,
  });
}

/// Presses at `from`, walks to `from + delta` in `steps` moves, optionally runs `midDrag` while
/// the button is still down, then releases. Real pointer events only.
///
/// `readyExpression` is awaited after the press and before the first move. The editor's pointer-
/// down handler is asynchronous (hit test, selection, begin transaction), so moves dispatched
/// before it finishes would be dropped and the drag would come up short.
async function dragPointer(
  from,
  delta,
  { steps = 16, modifiers = 0, midDrag = null, readyExpression = null } = {},
) {
  await mouseDown(from, modifiers);
  if (readyExpression) await waitFor(readyExpression, 30000, "drag_did_not_start");
  for (let index = 1; index <= steps; index += 1) {
    await mouseMove(
      {
        x: from.x + (delta.x * index) / steps,
        y: from.y + (delta.y * index) / steps,
      },
      modifiers,
    );
  }
  let midDragResult = null;
  if (midDrag) {
    // The editor coalesces drag frames across two animation frames; wait for the engine to
    // catch up before measuring what is on screen.
    await sleep(120);
    midDragResult = await midDrag();
  }
  await mouseUp({ x: from.x + delta.x, y: from.y + delta.y }, modifiers);
  return midDragResult;
}

async function pressKey(key, code, keyCode, modifiers = 0) {
  await pageClient.send("Input.dispatchKeyEvent", {
    type: "rawKeyDown",
    key,
    code,
    windowsVirtualKeyCode: keyCode,
    modifiers,
  });
  await pageClient.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key,
    code,
    windowsVirtualKeyCode: keyCode,
    modifiers,
  });
}

async function replaceNumeric(label, value) {
  const selector = `input[aria-label='${label}']`;
  const before = (await getProof()).engine_sequence;
  await clickSelector(selector);
  await pageClient.send("Input.dispatchKeyEvent", {
    type: "rawKeyDown",
    key: "a",
    code: "KeyA",
    windowsVirtualKeyCode: 65,
    modifiers: 2,
  });
  await pageClient.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: "a",
    code: "KeyA",
    windowsVirtualKeyCode: 65,
    modifiers: 2,
  });
  await pageClient.send("Input.insertText", { text: String(value) });
  await pressKey("Enter", "Enter", 13);
  await waitFor(
    `window.__PHASE0E_PROOF__?.engine_sequence > ${before}`,
    30000,
    "engine_response_timeout",
  );
}

async function capture(pathname) {
  const screenshot = await pageClient.send("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
    fromSurface: true,
  });
  if (pathname) await writeFile(pathname, Buffer.from(screenshot.data, "base64"));
  return screenshot.data;
}

/// Decodes one screenshot inside the page and returns the RGBA of every requested CSS point.
/// `radius` searches a small neighbourhood, because overlay strokes are one CSS pixel wide.
async function sampleScreenshot(base64, points, radius = 2) {
  return evaluate(`(async () => {
    const response = await fetch("data:image/png;base64," + ${JSON.stringify(base64)});
    const bitmap = await createImageBitmap(await response.blob());
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    const context = canvas.getContext("2d", { willReadFrequently: true });
    context.drawImage(bitmap, 0, 0);
    const scale = bitmap.width / window.innerWidth;
    const points = ${JSON.stringify(points)};
    const radius = ${radius};
    return points.map((point) => {
      const centerX = Math.round(point.x * scale);
      const centerY = Math.round(point.y * scale);
      const size = radius * 2 + 1;
      const left = Math.max(0, centerX - radius);
      const top = Math.max(0, centerY - radius);
      const data = context.getImageData(left, top, size, size).data;
      const pixels = [];
      for (let index = 0; index < data.length; index += 4) {
        pixels.push([data[index], data[index + 1], data[index + 2], data[index + 3]]);
      }
      return { label: point.label ?? null, x: point.x, y: point.y, device_scale: scale, pixels };
    });
  })()`);
}

function colorDistance(left, right) {
  return Math.sqrt(
    (left[0] - right[0]) ** 2 + (left[1] - right[1]) ** 2 + (left[2] - right[2]) ** 2,
  );
}

/// True when any pixel in the sampled neighbourhood is within `tolerance` of `target`.
function neighbourhoodHasColor(sample, target, tolerance = 60) {
  return sample.pixels.some((pixel) => colorDistance(pixel, target) <= tolerance);
}

function minimumDistance(sample, target) {
  return Math.min(...sample.pixels.map((pixel) => colorDistance(pixel, target)));
}

function recordPixelCase(entry) {
  pixelCases.push(entry);
  return entry;
}

function percentile(values, fraction) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.ceil((sorted.length - 1) * fraction)];
}

/// Live projection of every node the engine currently reports, keyed by ID.
async function nodeTable() {
  const response = await send("get_ui_snapshot");
  const table = {};
  for (const node of response.projection.upserts) table[node.id] = node;
  return table;
}

async function worldBounds(id) {
  const table = await nodeTable();
  const node = table[id];
  if (!node?.world_bounds)
    throw new HarnessFailure("node_bounds_missing", `Node ${id} has no world bounds`);
  return node.world_bounds;
}

function boundsCenter(bounds) {
  return [
    (bounds.min[0] + bounds.max[0]) / 2,
    (bounds.min[1] + bounds.max[1]) / 2,
  ];
}

/// Converts a world point into client coordinates using the live camera and canvas rectangle.
async function clientPointForWorld(world) {
  const proof = await getProof();
  const canvas = await boundsForSelector(".webgpu-canvas");
  return {
    x:
      canvas.x +
      (world[0] - proof.camera.center[0]) * proof.camera.zoom +
      proof.camera.viewport[0] / 2,
    y:
      canvas.y +
      (world[1] - proof.camera.center[1]) * proof.camera.zoom +
      proof.camera.viewport[1] / 2,
  };
}

async function clientPointForNodeCenter(id) {
  return clientPointForWorld(boundsCenter(await worldBounds(id)));
}

async function readCanvasPixelAtWorld(world) {
  const proof = await getProof();
  const viewportX =
    (world[0] - proof.camera.center[0]) * proof.camera.zoom +
    proof.camera.viewport[0] / 2;
  const viewportY =
    (world[1] - proof.camera.center[1]) * proof.camera.zoom +
    proof.camera.viewport[1] / 2;
  return evaluate(
    `window.__phase0eReadPixel(${viewportX}, ${viewportY})`,
  );
}

async function selectionIds() {
  const proof = await getProof();
  return proof.selection ?? [];
}

async function selectOnly(id) {
  await send("selection", { target: id, mode: "replace" });
  await waitFor(`window.__PHASE0E_PROOF__?.selection_count === 1`);
}

async function addToSelection(id, expectedCount) {
  await send("selection", { target: id, mode: "toggle" });
  await waitFor(`window.__PHASE0E_PROOF__?.selection_count === ${expectedCount}`);
}

async function currentZoom() {
  return (await getProof()).camera.zoom;
}

async function fitDocument() {
  const before = (await getProof()).engine_sequence;
  await clickText(".canvas-toolbar-actions button", "Fit document");
  await waitFor(
    `window.__PHASE0E_PROOF__?.engine_sequence > ${before}`,
    30000,
    "engine_response_timeout",
  );
}

/// Creates one rectangle by dragging with the rectangle tool, then sets its exact geometry
/// through the Inspector. Both paths are real UI input.
async function createRectangle({ x, y, width, height, index }) {
  await clickSelector("[data-testid='tool-rectangle']");
  await waitFor("window.__PHASE0E_PROOF__?.tool === 'rectangle'");
  const shell = await boundsForSelector("[data-testid='canvas-shell']");
  const start = {
    x: shell.x + 120 + index * 30,
    y: shell.y + 120 + index * 30,
  };
  await dragPointer(start, { x: 90, y: 60 }, {
    steps: 8,
    readyExpression: "window.__PHASE0E_PROOF__?.fsm === 'CreatingRectangle'",
  });
  await waitFor(
    `window.__PHASE0E_PROOF__?.primary_node?.kind === 'rectangle' && window.__PHASE0E_PROOF__?.fsm === 'Idle'`,
  );
  await replaceNumeric("X", x);
  await replaceNumeric("Y", y);
  await replaceNumeric("W", width);
  await replaceNumeric("H", height);
  const proof = await getProof();
  return proof.primary_node.id;
}

async function runBenchmark(adapterLabel) {
  // The benchmark runs against the flat 1,000-object fixture so that selection size is the only
  // variable. Drag frames are issued from the page through the same Worker/WASM/WebGPU request
  // path the pointer handler uses; timing excludes CDP round-trips by measuring in-page.
  await send("load_fixture", { fixture: "bench-a" });
  await waitFor("window.__PHASE0E_PROOF__?.fixture === 'BENCH-A'");
  await send("camera", { camera: { kind: "fit" } });

  const series = [];
  for (const count of [10, 100, 1000]) {
    for (const snap of [false, true]) {
      const ids = Array.from({ length: count }, (_, index) => fixtureNodeId(index + 1));
      const measurement = await evaluate(`(async () => {
        const ids = ${JSON.stringify(ids)};
        const framesPerDrag = 5;
        const warmups = 5;
        const measured = 20;
        const samples = [];
        let lastResponse = null;
        await window.__phase0eSend("selection", { mode: "set", targets: ids });
        for (let iteration = 0; iteration < warmups + measured; iteration += 1) {
          await window.__phase0eSend("begin_transaction");
          const started = performance.now();
          for (let frame = 1; frame <= framesPerDrag; frame += 1) {
            lastResponse = await window.__phase0eSend("translate_selection", {
              dx: frame * 2,
              dy: frame,
              snap: ${snap},
              snap_threshold_px: ${SNAP_THRESHOLD_PX},
            });
          }
          const perFrame = (performance.now() - started) / framesPerDrag;
          await window.__phase0eSend("commit_transaction");
          await window.__phase0eSend("undo");
          if (iteration >= warmups) samples.push(perFrame);
        }
        return {
          samples,
          warmups,
          measured,
          frames_per_drag: framesPerDrag,
          moved: lastResponse?.result?.moved ?? null,
          dirty_slots: lastResponse?.render_delta?.dirty_slots ?? null,
          upload_bytes: lastResponse?.render_delta?.upload_bytes ?? null,
          scene_full_rebuilds: lastResponse?.render_delta?.scene_full_rebuilds ?? null,
          render_full_rebuilds: lastResponse?.render_delta?.render_full_rebuilds ?? null,
          gpu_frame_sequence: window.__PHASE0E_PROOF__?.gpu_frame_sequence ?? null,
        };
      })()`);
      const samples = measurement.samples;
      series.push({
        objects: count,
        snapping: snap,
        moved_nodes: measurement.moved,
        frames_per_drag: measurement.frames_per_drag,
        warmup_iterations: measurement.warmups,
        measured_iterations: measurement.measured,
        raw_frame_ms: samples,
        median_ms: percentile(samples, 0.5),
        p95_ms: percentile(samples, 0.95),
        maximum_ms: Math.max(...samples),
        dirty_slots: measurement.dirty_slots,
        upload_bytes: measurement.upload_bytes,
        scene_full_rebuilds: measurement.scene_full_rebuilds,
        render_full_rebuilds: measurement.render_full_rebuilds,
        gpu_frame_sequence: measurement.gpu_frame_sequence,
      });
    }
  }

  const findSeries = (objects, snapping) =>
    series.find((entry) => entry.objects === objects && entry.snapping === snapping);
  const checks = {
    ten_objects_within_frame_budget: findSeries(10, false).p95_ms <= 16.7,
    ten_objects_snapped_within_frame_budget: findSeries(10, true).p95_ms <= 16.7,
    hundred_objects_within_frame_budget: findSeries(100, false).p95_ms <= 16.7,
    hundred_objects_snapped_within_frame_budget: findSeries(100, true).p95_ms <= 16.7,
    thousand_objects_measured: findSeries(1000, false).raw_frame_ms.length === 20,
    every_drag_moved_every_selected_node: series.every(
      (entry) => entry.moved_nodes === entry.objects,
    ),
    no_full_rebuilds: series.every(
      (entry) => entry.scene_full_rebuilds === 0 && entry.render_full_rebuilds === 0,
    ),
  };
  const metrics = {
    phase: "1B",
    ...gateEvidence,
    proof_kind: "actual-browser-worker-wasm-webgpu-multi-drag",
    captured_at_utc: new Date().toISOString(),
    adapter: adapterLabel,
    fixture: "BENCH-A",
    frame_budget_ms: 16.7,
    budget_note:
      "The 16.7 ms budget is asserted for 10 and 100 simultaneously dragged objects. The 1,000-object series is recorded without a gate threshold; Phase 1B does not claim a large-selection drag budget.",
    series,
    checks,
    all_passed: Object.values(checks).every(Boolean),
  };
  await writeFile(metricsPath, `${JSON.stringify(metrics, null, 2)}\n`, "utf8");
  return metrics;
}

try {
  await stat(chrome).catch(() => {
    throw new HarnessFailure(
      "chrome_not_found",
      `Chrome executable was not found: ${chrome}`,
      { hint: "Set PHASE0E_CHROME to the Chrome executable." },
    );
  });
  preview = await preparePreview();
  chromeProcess = spawn(
    chrome,
    [
      "--headless=new",
      "--remote-debugging-port=0",
      `--user-data-dir=${profile}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-background-networking",
      "--disable-component-update",
      "--disable-gpu-sandbox",
      "--enable-unsafe-webgpu",
      "--ignore-gpu-blocklist",
      // Container runs execute as root, where Chrome refuses to start sandboxed. This never
      // affects a normal hardware run and is recorded in the proof.
      ...(process.env.PHASE1B_NO_SANDBOX === "1" ? ["--no-sandbox"] : []),
      // Diagnostic-only: lets the harness run where no GPU exists. The result is written to the
      // software-run paths and can never satisfy the gate.
      ...(allowSoftwareGpu
        ? ["--enable-unsafe-swiftshader", "--use-angle=swiftshader", "--use-gl=angle"]
        : []),
      "--window-size=1600,1000",
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"], windowsHide: true },
  );
  chromeProcess.stderr.setEncoding("utf8");
  chromeProcess.stderr.on("data", (chunk) => {
    chromeStderr = (chromeStderr + chunk).slice(-20000);
  });

  const activePortFile = path.join(profile, "DevToolsActivePort");
  const debugPort = (await waitForFile(activePortFile)).trim().split(/\r?\n/)[0];
  const debugBase = `http://127.0.0.1:${debugPort}`;
  const version = await (await fetch(`${debugBase}/json/version`)).json();
  browserClient = new CdpClient(version.webSocketDebuggerUrl);
  await browserClient.connect();
  const gpuInfo = await browserClient.send("SystemInfo.getInfo");

  const target = await (
    await fetch(`${debugBase}/json/new?${encodeURIComponent(preview.url)}`, {
      method: "PUT",
    })
  ).json();
  pageClient = new CdpClient(target.webSocketDebuggerUrl);
  await pageClient.connect();
  pageClient.on("Runtime.exceptionThrown", ({ exceptionDetails }) => {
    consoleErrors.push(
      exceptionDetails.exception?.description ?? exceptionDetails.text,
    );
  });
  pageClient.on("Runtime.consoleAPICalled", (details) => {
    if (details.type === "error" || details.type === "assert") {
      consoleErrors.push(
        details.args
          .map((argument) => argument.value ?? argument.description)
          .join(" "),
      );
    }
  });
  pageClient.on("Log.entryAdded", ({ entry }) => {
    if (entry.level === "error") consoleErrors.push(entry.text);
  });
  await pageClient.send("Runtime.enable");
  await pageClient.send("Page.enable");
  await pageClient.send("Page.bringToFront");
  await pageClient.send("Log.enable");
  await pageClient.send("Emulation.setDeviceMetricsOverride", {
    width: 1600,
    height: 1000,
    deviceScaleFactor: 1,
    mobile: false,
  });

  await waitForReady();
  const initial = await getProof();

  const activeDevice =
    gpuInfo.gpu?.devices?.find((device) => device.active) ??
    gpuInfo.gpu?.devices?.[0] ??
    {};
  const adapterLabel = String(initial.adapter ?? activeDevice.deviceString ?? "");
  const softwareRenderer = /swiftshader|llvmpipe|software|lavapipe|basic render/i.test(
    `${adapterLabel} ${activeDevice.deviceString ?? ""}`,
  );
  if (softwareRenderer && !allowSoftwareGpu) {
    throw new HarnessFailure(
      "software_gpu_rejected",
      `Adapter "${adapterLabel}" is a software renderer; Gate 1B requires actual GPU hardware`,
      {
        adapter: adapterLabel,
        active_device: activeDevice,
        hint: "Run on a machine with a real GPU, or set PHASE1B_ALLOW_SOFTWARE_GPU=1 to capture non-gate diagnostic evidence.",
      },
    );
  }

  await waitFor("window.__PHASE0E_PROOF__?.fixture === 'EDITOR'");
  const frameId = Object.values(await nodeTable()).find((node) => node.kind === "frame")?.id;
  if (!frameId)
    throw new HarnessFailure("editor_frame_missing", "The editor fixture has no Frame node");

  // Three rectangles created through the rectangle tool, then positioned exactly. The tool
  // creates them as siblings of the Frame, so Inspector X/Y are world coordinates.
  const layout = [
    { x: -700, y: -300, width: 160, height: 100 },
    { x: -300, y: -100, width: 160, height: 100 },
    { x: 200, y: 150, width: 160, height: 100 },
  ];
  const rectangles = [];
  for (const [index, entry] of layout.entries()) {
    rectangles.push(await createRectangle({ ...entry, index }));
  }
  await clickSelector("[data-testid='tool-select']");
  await fitDocument();
  const createdNodes = await nodeTable();

  // ---------------------------------------------------------------- shift multi-selection
  await selectOnly(rectangles[0]);
  const singleSelectionShot = await capture(null);
  const singleSelectionSamples = await sampleScreenshot(
    singleSelectionShot,
    [{ label: "single-selection-union-probe", x: 8, y: 8 }],
  );
  const unionAbsentWithOneSelected = (await elementCount("[data-testid='selection-union']")) === 0;

  const secondCenter = await clientPointForNodeCenter(rectangles[1]);
  await clickPoint(secondCenter, { modifiers: 8 }); // 8 = Shift
  await waitFor("window.__PHASE0E_PROOF__?.selection_count === 2");
  const shiftSelection = await selectionIds();
  const unionBounds = await boundsForSelector("[data-testid='selection-union']");
  const multiSelectionShot = await capture(screenshots.multi_selection);
  const outlineCount = await elementCount(".selection-outline");
  const unionSamples = await sampleScreenshot(multiSelectionShot, [
    { label: "union-top-edge", x: unionBounds.x + unionBounds.width / 2, y: unionBounds.y },
    { label: "union-right-edge-0.25", x: unionBounds.x + unionBounds.width, y: unionBounds.y + unionBounds.height * 0.25 },
    { label: "union-right-edge-0.5", x: unionBounds.x + unionBounds.width, y: unionBounds.y + unionBounds.height * 0.5 },
    { label: "union-right-edge-0.75", x: unionBounds.x + unionBounds.width, y: unionBounds.y + unionBounds.height * 0.75 },
    { label: "union-bottom-edge", x: unionBounds.x + unionBounds.width / 2, y: unionBounds.y + unionBounds.height },
    { label: "union-interior", x: unionBounds.center_x, y: unionBounds.center_y },
  ]);
  const outlineBounds = await boundsForSelector(".selection-outline");
  const outlineSamples = await sampleScreenshot(multiSelectionShot, [
    { label: "outline-top-edge", x: outlineBounds.center_x, y: outlineBounds.y },
    { label: "outline-left-edge", x: outlineBounds.x, y: outlineBounds.center_y },
  ]);
  recordPixelCase({
    case: "selection_outline_rendered",
    method: "composited screenshot neighbourhood sampling",
    expected_srgb: PRIMARY_RGB,
    samples: outlineSamples.map((sample) => ({
      label: sample.label,
      minimum_distance: minimumDistance(sample, PRIMARY_RGB),
      matched: neighbourhoodHasColor(sample, PRIMARY_RGB),
    })),
  });
  recordPixelCase({
    case: "selection_union_rendered",
    method: "composited screenshot neighbourhood sampling",
    expected_srgb: PRIMARY_RGB,
    samples: unionSamples.map((sample) => ({
      label: sample.label,
      minimum_distance: minimumDistance(sample, PRIMARY_RGB),
      matched: neighbourhoodHasColor(sample, PRIMARY_RGB),
    })),
  });

  // ---------------------------------------------------------------- rubber band selection
  await clickSelector("[data-testid='tool-select']");
  const firstBounds = await worldBounds(rectangles[0]);
  const secondBounds = await worldBounds(rectangles[1]);
  const bandStartWorld = [firstBounds.min[0] - 60, firstBounds.min[1] - 60];
  const bandEndWorld = [secondBounds.max[0] + 40, secondBounds.max[1] + 40];
  const bandStart = await clientPointForWorld(bandStartWorld);
  const bandEnd = await clientPointForWorld(bandEndWorld);
  const marqueeEvidence = await dragPointer(
    bandStart,
    { x: bandEnd.x - bandStart.x, y: bandEnd.y - bandStart.y },
    {
      steps: 18,
      readyExpression: "window.__PHASE0E_PROOF__?.fsm === 'MarqueeSelecting'",
      midDrag: async () => {
        const marqueeRect = await boundsForSelector("[data-testid='marquee']");
        const shot = await capture(screenshots.marquee);
        const insideY = marqueeRect.y + marqueeRect.height / 2;
        const samples = await sampleScreenshot(
          shot,
          [
            { label: "band-top-edge", x: marqueeRect.center_x, y: marqueeRect.y },
            { label: "band-left-edge", x: marqueeRect.x, y: insideY },
            { label: "band-interior", x: marqueeRect.center_x, y: insideY },
            { label: "outside-band", x: marqueeRect.x + marqueeRect.width + 40, y: insideY },
          ],
          2,
        );
        const marqueeActive = await evaluate(
          "window.__PHASE0E_PROOF__?.marquee_active === true",
        );
        return { rect: marqueeRect, samples, marquee_active: marqueeActive };
      },
    },
  );
  await waitFor("window.__PHASE0E_PROOF__?.selection_count === 2");
  const marqueeSelection = await selectionIds();
  const bandInterior = marqueeEvidence.samples.find((sample) => sample.label === "band-interior");
  const bandOutside = marqueeEvidence.samples.find((sample) => sample.label === "outside-band");
  const averageBlue = (sample) =>
    sample.pixels.reduce((total, pixel) => total + pixel[2], 0) / sample.pixels.length;
  const averageRed = (sample) =>
    sample.pixels.reduce((total, pixel) => total + pixel[0], 0) / sample.pixels.length;
  recordPixelCase({
    case: "marquee_band_rendered",
    method: "composited screenshot: band edge colour plus interior/exterior blue comparison",
    expected_srgb: PRIMARY_RGB,
    marquee_active: marqueeEvidence.marquee_active,
    samples: marqueeEvidence.samples.map((sample) => ({
      label: sample.label,
      minimum_distance: minimumDistance(sample, PRIMARY_RGB),
      matched: neighbourhoodHasColor(sample, PRIMARY_RGB),
      average_blue: averageBlue(sample),
    })),
    interior_average_blue: averageBlue(bandInterior),
    exterior_average_blue: averageBlue(bandOutside),
    interior_average_red: averageRed(bandInterior),
    exterior_average_red: averageRed(bandOutside),
  });

  // ---------------------------------------------------------------- select all
  const canvasShell = await boundsForSelector("[data-testid='canvas-shell']");
  await clickPoint({ x: canvasShell.x + 12, y: canvasShell.y + canvasShell.height - 12 });
  await pressKey("a", "KeyA", 65, 2); // 2 = Ctrl
  // The document root holds the Frame and the three rectangles, so select-all reports four.
  await waitFor("window.__PHASE0E_PROOF__?.selection_count === 4");
  const selectAllSelection = await selectionIds();

  // ---------------------------------------------------------------- multi-object drag
  const snapToggleActive = async () =>
    evaluate(
      "document.querySelector(\"[data-testid='snap-toggle']\")?.getAttribute('aria-pressed') === 'true'",
    );
  if (await snapToggleActive()) await clickSelector("[data-testid='snap-toggle']");
  const snapDisabled = (await snapToggleActive()) === false;

  await selectOnly(rectangles[0]);
  await addToSelection(rectangles[1], 2);
  const beforeMultiDrag = await nodeTable();
  const zoom = await currentZoom();
  const dragWorldDelta = [90, 60];
  const dragStart = await clientPointForNodeCenter(rectangles[0]);
  await dragPointer(
    dragStart,
    { x: dragWorldDelta[0] * zoom, y: dragWorldDelta[1] * zoom },
    { steps: 24, readyExpression: "window.__PHASE0E_PROOF__?.fsm === 'Moving'" },
  );
  await waitFor("window.__PHASE0E_PROOF__?.fsm === 'Idle'");
  const afterMultiDrag = await nodeTable();
  const draggedPair = [rectangles[0], rectangles[1]];
  const multiDragDeltas = draggedPair.map((id) => [
    afterMultiDrag[id].world_bounds.min[0] - beforeMultiDrag[id].world_bounds.min[0],
    afterMultiDrag[id].world_bounds.min[1] - beforeMultiDrag[id].world_bounds.min[1],
  ]);
  const untouchedRectangleStayed =
    Math.abs(
      afterMultiDrag[rectangles[2]].world_bounds.min[0] -
        beforeMultiDrag[rectangles[2]].world_bounds.min[0],
    ) < WORLD_EPSILON;
  const movedPixel = await readCanvasPixelAtWorld(
    boundsCenter(afterMultiDrag[rectangles[0]].world_bounds),
  );
  const vacatedPixel = await readCanvasPixelAtWorld(
    boundsCenter(beforeMultiDrag[rectangles[0]].world_bounds),
  );
  recordPixelCase({
    case: "dragged_rectangle_moved_on_webgpu_surface",
    method: "WebGPU surface readback at the vacated and occupied centres",
    moved_pixel: movedPixel,
    vacated_pixel: vacatedPixel,
    distinct: colorDistance(movedPixel.rgba, vacatedPixel.rgba) > 30,
  });

  // multi-drag undo
  const beforeMultiDragUndo = await getProof();
  await clickSelector("[data-testid='undo']");
  await waitFor(
    `window.__PHASE0E_PROOF__?.history?.undo_depth === ${beforeMultiDragUndo.history.undo_depth - 1}`,
  );
  const afterMultiDragUndo = await nodeTable();
  const multiDragUndoRestored = draggedPair.every(
    (id) =>
      Math.abs(
        afterMultiDragUndo[id].world_bounds.min[0] - beforeMultiDrag[id].world_bounds.min[0],
      ) < WORLD_EPSILON &&
      Math.abs(
        afterMultiDragUndo[id].world_bounds.min[1] - beforeMultiDrag[id].world_bounds.min[1],
      ) < WORLD_EPSILON,
  );

  // ---------------------------------------------------------------- coalesced drag drift
  await selectOnly(rectangles[2]);
  const beforeDrift = await worldBounds(rectangles[2]);
  const driftZoom = await currentZoom();
  const driftWorldDelta = [120, -45];
  const driftStart = await clientPointForNodeCenter(rectangles[2]);
  await dragPointer(
    driftStart,
    { x: driftWorldDelta[0] * driftZoom, y: driftWorldDelta[1] * driftZoom },
    { steps: 64, readyExpression: "window.__PHASE0E_PROOF__?.fsm === 'Moving'" },
  );
  await waitFor("window.__PHASE0E_PROOF__?.fsm === 'Idle'");
  const afterDrift = await worldBounds(rectangles[2]);
  const driftError = [
    afterDrift.min[0] - beforeDrift.min[0] - driftWorldDelta[0],
    afterDrift.min[1] - beforeDrift.min[1] - driftWorldDelta[1],
  ];
  const driftProof = await getProof();
  await clickSelector("[data-testid='undo']");
  await waitFor("window.__PHASE0E_PROOF__?.fsm === 'Idle'");

  // ---------------------------------------------------------------- snapping
  if (!(await snapToggleActive())) await clickSelector("[data-testid='snap-toggle']");
  const snapEnabled = await snapToggleActive();
  const anchorBounds = await worldBounds(rectangles[0]);
  await selectOnly(rectangles[1]);
  const snapSubjectBounds = await worldBounds(rectangles[1]);
  const snapZoom = await currentZoom();
  // Land ten world units right of the anchor's left edge: inside the snap radius at this zoom,
  // and far enough outside pointer quantisation to distinguish a snapped result from a raw one.
  const snapOffsetWorld = 10;
  const snapWorldDeltaX =
    anchorBounds.min[0] + snapOffsetWorld - snapSubjectBounds.min[0];
  const snapStart = await clientPointForNodeCenter(rectangles[1]);
  const snapEvidence = await dragPointer(
    snapStart,
    { x: snapWorldDeltaX * snapZoom, y: 0 },
    {
      steps: 20,
      readyExpression: "window.__PHASE0E_PROOF__?.fsm === 'Moving'",
      midDrag: async () => {
        const guideCount = await elementCount("[data-testid='snap-guide-horizontal']");
        const guides = await evaluate(
          "window.__PHASE0E_PROOF__?.snap_guides ?? 0",
        );
        let samples = [];
        let guideRect = null;
        if (guideCount > 0) {
          guideRect = await boundsForSelector("[data-testid='snap-guide-horizontal']");
          const points = [0.25, 0.5, 0.75].map((fraction) => ({
            label: `guide-${fraction}`,
            x: guideRect.x + guideRect.width * fraction,
            y: guideRect.y + guideRect.height / 2,
          }));
          points.push({
            label: "guide-offset-control",
            x: guideRect.center_x,
            y: guideRect.y + guideRect.height / 2 + 40,
          });
          samples = await sampleScreenshot(await capture(screenshots.snap_guide), points, 2);
        }
        return { guide_elements: guideCount, engine_guides: guides, guide_rect: guideRect, samples };
      },
    },
  );
  await waitFor("window.__PHASE0E_PROOF__?.fsm === 'Idle'");
  const snappedBounds = await worldBounds(rectangles[1]);
  const snapError = snappedBounds.min[0] - anchorBounds.min[0];
  const guideSamples = snapEvidence.samples ?? [];
  recordPixelCase({
    case: "snap_guide_rendered",
    method: "composited screenshot neighbourhood sampling along the engine-reported guide",
    expected_srgb: GUIDE_RGB,
    guide_elements: snapEvidence.guide_elements,
    engine_reported_guides: snapEvidence.engine_guides,
    samples: guideSamples.map((sample) => ({
      label: sample.label,
      minimum_distance: minimumDistance(sample, GUIDE_RGB),
      matched: neighbourhoodHasColor(sample, GUIDE_RGB),
    })),
  });
  await clickSelector("[data-testid='undo']");
  await waitFor("window.__PHASE0E_PROOF__?.fsm === 'Idle'");

  // ---------------------------------------------------------------- Alt suspends snapping
  await selectOnly(rectangles[1]);
  const altSubjectBounds = await worldBounds(rectangles[1]);
  const altZoom = await currentZoom();
  const altWorldDeltaX = anchorBounds.min[0] + snapOffsetWorld - altSubjectBounds.min[0];
  const altStart = await clientPointForNodeCenter(rectangles[1]);
  const altEvidence = await dragPointer(
    altStart,
    { x: altWorldDeltaX * altZoom, y: 0 },
    {
      steps: 20,
      modifiers: 1, // 1 = Alt
      readyExpression: "window.__PHASE0E_PROOF__?.fsm === 'Moving'",
      midDrag: async () => ({
        guide_elements: await elementCount("[data-testid='snap-guide-vertical']"),
        engine_guides: await evaluate("window.__PHASE0E_PROOF__?.snap_guides ?? 0"),
      }),
    },
  );
  await waitFor("window.__PHASE0E_PROOF__?.fsm === 'Idle'");
  const altBounds = await worldBounds(rectangles[1]);
  const altOffsetError =
    altBounds.min[0] - (anchorBounds.min[0] + snapOffsetWorld);
  await clickSelector("[data-testid='undo']");
  await waitFor("window.__PHASE0E_PROOF__?.fsm === 'Idle'");

  // ---------------------------------------------------------------- snap toggle off
  await clickSelector("[data-testid='snap-toggle']");
  const snapToggledOff = (await snapToggleActive()) === false;
  await selectOnly(rectangles[1]);
  const toggleSubjectBounds = await worldBounds(rectangles[1]);
  const toggleZoom = await currentZoom();
  const toggleWorldDeltaX =
    anchorBounds.min[0] + snapOffsetWorld - toggleSubjectBounds.min[0];
  const toggleStart = await clientPointForNodeCenter(rectangles[1]);
  const toggleEvidence = await dragPointer(
    toggleStart,
    { x: toggleWorldDeltaX * toggleZoom, y: 0 },
    {
      steps: 20,
      readyExpression: "window.__PHASE0E_PROOF__?.fsm === 'Moving'",
      midDrag: async () => ({
        guide_elements: await elementCount("[data-testid='snap-guide-vertical']"),
        engine_guides: await evaluate("window.__PHASE0E_PROOF__?.snap_guides ?? 0"),
      }),
    },
  );
  await waitFor("window.__PHASE0E_PROOF__?.fsm === 'Idle'");
  const toggleBounds = await worldBounds(rectangles[1]);
  const toggleOffsetError =
    toggleBounds.min[0] - (anchorBounds.min[0] + snapOffsetWorld);
  await clickSelector("[data-testid='undo']");
  await waitFor("window.__PHASE0E_PROOF__?.fsm === 'Idle'");
  await clickSelector("[data-testid='snap-toggle']");

  // ---------------------------------------------------------------- align and distribute
  await selectOnly(rectangles[0]);
  for (const [index, id] of rectangles.slice(1).entries()) {
    await addToSelection(id, index + 2);
  }
  const beforeArrange = await nodeTable();
  const arrangeResults = {};
  const arrangeChecks = {};
  const arrangeOperations = [
    ["align-left", (bounds) => bounds.map((entry) => entry.min[0])],
    ["align-horizontal-center", (bounds) => bounds.map((entry) => (entry.min[0] + entry.max[0]) / 2)],
    ["align-right", (bounds) => bounds.map((entry) => entry.max[0])],
    ["align-top", (bounds) => bounds.map((entry) => entry.min[1])],
    ["align-vertical-center", (bounds) => bounds.map((entry) => (entry.min[1] + entry.max[1]) / 2)],
    ["align-bottom", (bounds) => bounds.map((entry) => entry.max[1])],
  ];
  for (const [testId, project] of arrangeOperations) {
    const undoDepthBefore = (await getProof()).history.undo_depth;
    await clickSelector(`[data-testid='${testId}']`);
    await waitFor(
      `window.__PHASE0E_PROOF__?.history?.undo_depth === ${undoDepthBefore + 1}`,
      30000,
      "arrange_did_not_commit",
    );
    const table = await nodeTable();
    const values = project(rectangles.map((id) => table[id].world_bounds));
    const aligned = values.every((value) => Math.abs(value - values[0]) < 1e-6);
    if (testId === "align-left") await capture(screenshots.aligned);
    await clickSelector("[data-testid='undo']");
    await waitFor(
      `window.__PHASE0E_PROOF__?.history?.undo_depth === ${undoDepthBefore}`,
      30000,
      "arrange_undo_did_not_apply",
    );
    const restoredTable = await nodeTable();
    const restored = rectangles.every(
      (id) =>
        Math.abs(
          restoredTable[id].world_bounds.min[0] - beforeArrange[id].world_bounds.min[0],
        ) < WORLD_EPSILON &&
        Math.abs(
          restoredTable[id].world_bounds.min[1] - beforeArrange[id].world_bounds.min[1],
        ) < WORLD_EPSILON,
    );
    arrangeResults[testId] = { values, aligned, undo_restored: restored };
    arrangeChecks[`${testId.replaceAll("-", "_")}_aligned`] = aligned;
    arrangeChecks[`${testId.replaceAll("-", "_")}_undo_restored`] = restored;
  }

  for (const [testId, axis] of [
    ["distribute-horizontal", 0],
    ["distribute-vertical", 1],
  ]) {
    const undoDepthBefore = (await getProof()).history.undo_depth;
    await clickSelector(`[data-testid='${testId}']`);
    await waitFor(
      `window.__PHASE0E_PROOF__?.history?.undo_depth === ${undoDepthBefore + 1}`,
      30000,
      "distribute_did_not_commit",
    );
    const table = await nodeTable();
    const ordered = rectangles
      .map((id) => table[id].world_bounds)
      .sort((left, right) => left.min[axis] - right.min[axis]);
    const gaps = [
      ordered[1].min[axis] - ordered[0].max[axis],
      ordered[2].min[axis] - ordered[1].max[axis],
    ];
    const equalGaps = Math.abs(gaps[0] - gaps[1]) < 1e-6;
    if (testId === "distribute-horizontal") await capture(screenshots.distributed);
    await clickSelector("[data-testid='undo']");
    await waitFor(
      `window.__PHASE0E_PROOF__?.history?.undo_depth === ${undoDepthBefore}`,
      30000,
      "distribute_undo_did_not_apply",
    );
    const restoredTable = await nodeTable();
    const restored = rectangles.every(
      (id) =>
        Math.abs(
          restoredTable[id].world_bounds.min[axis] - beforeArrange[id].world_bounds.min[axis],
        ) < WORLD_EPSILON,
    );
    arrangeResults[testId] = { gaps, equal_gaps: equalGaps, undo_restored: restored };
    arrangeChecks[`${testId.replaceAll("-", "_")}_equal_gaps`] = equalGaps;
    arrangeChecks[`${testId.replaceAll("-", "_")}_undo_restored`] = restored;
  }

  const beforeBenchmark = await getProof();
  const metrics = await runBenchmark(adapterLabel);
  const finalProof = await getProof();

  const pixelAssertions = {
    selection_outline_rendered: pixelCases
      .find((entry) => entry.case === "selection_outline_rendered")
      .samples.every((sample) => sample.matched),
    selection_union_rendered: pixelCases
      .find((entry) => entry.case === "selection_union_rendered")
      .samples.filter((sample) => sample.label !== "union-interior")
      .every((sample, _index, samples) =>
        sample.label.startsWith("union-right-edge-")
          ? samples
              .filter((candidate) => candidate.label.startsWith("union-right-edge-"))
              .some((candidate) => candidate.matched)
          : sample.matched,
      ),
    union_absent_for_single_selection: unionAbsentWithOneSelected,
    marquee_band_rendered: (() => {
      const entry = pixelCases.find((item) => item.case === "marquee_band_rendered");
      const edges = entry.samples.filter((sample) => sample.label.startsWith("band-"));
      return (
        entry.marquee_active === true &&
        edges.some((sample) => sample.matched) &&
        entry.exterior_average_red > entry.interior_average_red + 4
      );
    })(),
    snap_guide_rendered: (() => {
      const entry = pixelCases.find((item) => item.case === "snap_guide_rendered");
      const onGuide = entry.samples.filter((sample) => sample.label.startsWith("guide-0"));
      const control = entry.samples.find((sample) => sample.label === "guide-offset-control");
      return (
        entry.guide_elements >= 1 &&
        entry.engine_reported_guides >= 1 &&
        onGuide.length >= 3 &&
        onGuide.every((sample) => sample.matched) &&
        control !== undefined &&
        control.matched === false
      );
    })(),
    dragged_rectangle_moved_on_webgpu_surface: pixelCases.find(
      (entry) => entry.case === "dragged_rectangle_moved_on_webgpu_surface",
    ).distinct,
  };
  const pixelReadback = {
    phase: "1B",
    ...gateEvidence,
    proof_kind: "actual-hardware-webgpu-and-composited-overlay-readback",
    captured_at_utc: new Date().toISOString(),
    surface_base_format: initial.surface_base_format,
    pipeline_view_format: initial.pipeline_view_format,
    readback_view_format: initial.readback_view_format,
    overlay_sampling_note:
      "Interaction overlays are SVG in the composited page, not WebGPU surface content. They are measured by decoding a Page.captureScreenshot in the page; document rendering itself remains WebGPU only.",
    neighbourhood_radius_css_px: 2,
    colour_tolerance: 60,
    expected_primary_srgb: PRIMARY_RGB,
    expected_guide_srgb: GUIDE_RGB,
    cases: pixelCases,
    assertions: pixelAssertions,
    assertion_count: Object.keys(pixelAssertions).length,
    passed_assertion_count: Object.values(pixelAssertions).filter(Boolean).length,
    all_passed: Object.values(pixelAssertions).every(Boolean),
  };
  await writeFile(pixelPath, `${JSON.stringify(pixelReadback, null, 2)}\n`, "utf8");

  const checks = {
    server_health: preview.health.ok === true,
    asset_http_and_wasm_mime: Object.values(preview.assets).every(
      (asset) => asset.status === 200,
    ),
    new_chrome_process: Boolean(chromeProcess.pid),
    temporary_chrome_profile: profile.includes("phase1b-chrome-"),
    navigator_gpu: initial.actual_webgpu === true,
    hardware_gpu_adapter: softwareRenderer === false,
    dedicated_worker_wasm:
      initial.worker_runtime_owner === "dedicated-worker" &&
      initial.wasm_initialized === true &&
      initial.heartbeat >= 1,
    schema_v2:
      initial.render_binary_schema_version === 3 &&
      initial.resources.instance_stride_bytes === 112 &&
      initial.resources.dirty_record_stride_bytes === 116,
    three_rectangles_created_through_ui:
      rectangles.length === 3 &&
      rectangles.every((id) => createdNodes[id]?.kind === "rectangle"),
    shift_click_selects_two:
      shiftSelection.length === 2 &&
      shiftSelection.includes(rectangles[0]) &&
      shiftSelection.includes(rectangles[1]),
    multiple_selection_draws_every_outline: outlineCount >= 2,
    marquee_selects_intersecting_nodes:
      marqueeSelection.length === 2 &&
      marqueeSelection.includes(rectangles[0]) &&
      marqueeSelection.includes(rectangles[1]),
    select_all_selects_every_top_level_node:
      selectAllSelection.length === 4 &&
      selectAllSelection.includes(frameId) &&
      rectangles.every((id) => selectAllSelection.includes(id)),
    snap_toggle_disables_snapping: snapDisabled,
    multi_drag_moves_every_node_by_one_delta:
      // Both selected nodes must move by exactly the same delta, and that delta must match the
      // pointer travel within one quantised CSS pixel.
      Math.abs(multiDragDeltas[0][0] - multiDragDeltas[1][0]) < WORLD_EPSILON &&
      Math.abs(multiDragDeltas[0][1] - multiDragDeltas[1][1]) < WORLD_EPSILON &&
      multiDragDeltas.every(
        (delta) =>
          Math.abs(delta[0] - dragWorldDelta[0]) <= POINTER_TOLERANCE_CSS_PX / zoom &&
          Math.abs(delta[1] - dragWorldDelta[1]) <= POINTER_TOLERANCE_CSS_PX / zoom,
      ),
    multi_drag_undo_restores_every_node: multiDragUndoRestored,
    multi_drag_leaves_unselected_nodes_alone: untouchedRectangleStayed,
    coalesced_drag_has_no_drift:
      // Sixty-four pointer frames were sent. Accumulated drift would scale with that count, so a
      // single-pixel allowance still fails any per-frame accumulation.
      Math.abs(driftError[0]) <= POINTER_TOLERANCE_CSS_PX / driftZoom &&
      Math.abs(driftError[1]) <= POINTER_TOLERANCE_CSS_PX / driftZoom,
    coalesced_drag_coalesced_requests:
      driftProof.ui.pointerRawIntents > driftProof.ui.pointerRequestsSent,
    snapping_aligns_edges_exactly: Math.abs(snapError) < 1e-6,
    snap_guide_reported_and_rendered: pixelAssertions.snap_guide_rendered,
    alt_suspends_snapping:
      Math.abs(altOffsetError) <= POINTER_TOLERANCE_CSS_PX / altZoom &&
      Math.abs(altBounds.min[0] - anchorBounds.min[0]) >= snapOffsetWorld / 2 &&
      altEvidence.guide_elements === 0 &&
      altEvidence.engine_guides === 0,
    snap_toggle_off_suspends_snapping:
      snapToggledOff &&
      Math.abs(toggleOffsetError) <= POINTER_TOLERANCE_CSS_PX / toggleZoom &&
      Math.abs(toggleBounds.min[0] - anchorBounds.min[0]) >= snapOffsetWorld / 2 &&
      toggleEvidence.guide_elements === 0,
    snap_toggle_restored: (await snapToggleActive()) === true,
    ...arrangeChecks,
    benchmark_all_passed: metrics.all_passed,
    actual_pixel_readback: pixelReadback.all_passed,
    console_errors_zero: consoleErrors.length === 0,
    gpu_validation_errors_zero:
      Number(
        finalProof.gpu_validation_errors ?? finalProof.gpu?.validation_errors ?? 0,
      ) === 0,
    fallback_rebuild_zero: maxFallbackRebuildCountSeen === 0,
    latest_sequences_match: finalProof.response_gpu_overlay_sequence_match === true,
  };

  const screenshotInfo = {};
  for (const [name, pathname] of Object.entries(screenshots)) {
    const info = await stat(pathname).catch(() => null);
    screenshotInfo[name] = info
      ? {
          path: path.relative(workspace, pathname).replaceAll("\\", "/"),
          bytes: info.size,
          modified_utc: info.mtime.toISOString(),
        }
      : null;
  }

  const browserProofFinishedAtUtc = new Date().toISOString();
  const proof = {
    phase: "1B",
    ...gateEvidence,
    proof_kind: allowSoftwareGpu
      ? "software-gpu-diagnostic-run-not-gate-evidence"
      : "actual-hardware-browser",
    captured_at_utc: browserProofFinishedAtUtc,
    browser: version.Browser,
    browser_mode: "actual Chrome; no mock; no Canvas2D fallback",
    execution: {
      command: "npm run test:browser:phase1b",
      started_at_utc: browserProofStartedAtUtc,
      finished_at_utc: browserProofFinishedAtUtc,
      software_gpu_allowed: allowSoftwareGpu,
      chrome_sandbox_disabled: process.env.PHASE1B_NO_SANDBOX === "1",
    },
    user_agent: await evaluate("navigator.userAgent"),
    chrome_process_id: chromeProcess.pid,
    chrome_profile: profile,
    gpu: {
      adapter: initial.adapter,
      backend: initial.backend,
      software_renderer: softwareRenderer,
      surface_base_format: initial.surface_base_format,
      pipeline_view_format: initial.pipeline_view_format,
      readback_view_format: initial.readback_view_format,
      active_device: activeDevice,
    },
    preview: {
      url: preview.url,
      health: preview.health,
      assets: preview.assets,
      stdout: preview.processState.stdout,
      stderr: preview.processState.stderr,
    },
    initial,
    scene: {
      frame_id: frameId,
      rectangles,
      layout,
    },
    interactions: {
      shift_selection: shiftSelection,
      marquee_selection: marqueeSelection,
      select_all_selection: selectAllSelection,
      multi_drag_requested_world_delta: dragWorldDelta,
      multi_drag_world_deltas: multiDragDeltas,
      multi_drag_undo_restored: multiDragUndoRestored,
      pointer_tolerance_css_px: POINTER_TOLERANCE_CSS_PX,
      coalesced_drag: {
        requested_world_delta: driftWorldDelta,
        pointer_frames: 64,
        error: driftError,
        pointer_raw_intents: driftProof.ui.pointerRawIntents,
        pointer_requests_sent: driftProof.ui.pointerRequestsSent,
        pointer_requests_coalesced: driftProof.ui.pointerRequestsCoalesced,
      },
      snapping: {
        anchor_left_edge: anchorBounds.min[0],
        requested_offset_world: snapOffsetWorld,
        snapped_left_edge: snappedBounds.min[0],
        error: snapError,
        guide_elements: snapEvidence.guide_elements,
        engine_guides: snapEvidence.engine_guides,
      },
      alt_suspend: {
        expected_left_edge: anchorBounds.min[0] + snapOffsetWorld,
        actual_left_edge: altBounds.min[0],
        error: altOffsetError,
        guide_elements: altEvidence.guide_elements,
      },
      snap_toggle_off: {
        expected_left_edge: anchorBounds.min[0] + snapOffsetWorld,
        actual_left_edge: toggleBounds.min[0],
        error: toggleOffsetError,
        guide_elements: toggleEvidence.guide_elements,
      },
      arrange: arrangeResults,
      benchmark_started_at_engine_sequence: beforeBenchmark.engine_sequence,
    },
    pixel_readback_artifact: path.relative(workspace, pixelPath).replaceAll("\\", "/"),
    performance_artifact: path.relative(workspace, metricsPath).replaceAll("\\", "/"),
    screenshots: screenshotInfo,
    console_errors: consoleErrors,
    chrome_stderr_tail: chromeStderr,
    max_fallback_rebuild_count_seen: maxFallbackRebuildCountSeen,
    fallback_checkpoints: fallbackCheckpoints,
    checks,
    assertion_count: Object.keys(checks).length,
    passed_assertion_count: Object.values(checks).filter(Boolean).length,
    all_passed: Object.values(checks).every(Boolean),
  };
  await writeFile(proofPath, `${JSON.stringify(proof, null, 2)}\n`, "utf8");

  console.log(`Phase 1B actual browser proof: ${proof.all_passed ? "PASS" : "FAIL"}`);
  console.log(`browser=${version.Browser}`);
  console.log(`gpu=${adapterLabel || "unknown"}`);
  console.log(`proof=${proofPath}`);
  console.log(`metrics=${metricsPath}`);
  if (!proof.all_passed) {
    console.log(JSON.stringify(checks, null, 2));
    process.exitCode = 1;
  }
} catch (error) {
  const browserDiagnostics = pageClient
    ? await evaluate(
        "({body_ready: document.body?.dataset.ready ?? null, state: window.__phase0eState ?? null, proof: window.__PHASE0E_PROOF__ ?? null, navigator_gpu: Boolean(navigator.gpu)})",
      ).catch((diagnosticError) => ({ diagnostic_error: diagnosticError.message }))
    : null;
  const failure = {
    phase: "1B",
    ...gateEvidence,
    captured_at_utc: new Date().toISOString(),
    execution: {
      command: "npm run test:browser:phase1b",
      started_at_utc: browserProofStartedAtUtc,
      finished_at_utc: new Date().toISOString(),
      chrome_executable: chrome,
      host_platform: `${process.platform}-${process.arch}`,
      node_version: process.version,
    },
    code: error?.code ?? "browser_test_failed",
    message: error?.message ?? String(error),
    details: error?.details ?? null,
    stack: error?.stack ?? null,
    preview_url: preview?.url ?? null,
    preview_health: preview?.health ?? null,
    preview_assets: preview?.assets ?? null,
    preview_server: preview?.processState ?? null,
    browser: browserDiagnostics,
    console_errors: consoleErrors,
    chrome_stderr_tail: chromeStderr,
    gate_status: "UNVERIFIED",
  };
  await writeFile(failurePath, `${JSON.stringify(failure, null, 2)}\n`, "utf8").catch(
    () => {},
  );
  console.error(JSON.stringify(failure, null, 2));
  process.exitCode = 1;
} finally {
  pageClient?.close();
  browserClient?.close();
  chromeProcess?.kill();
  preview?.server?.kill();
  await sleep(300);
  await rm(profile, { recursive: true, force: true }).catch(() => {});
}

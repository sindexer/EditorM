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
const proofPath = path.join(verificationRoot, "PHASE_0E_R1_BROWSER_PROOF.json");
const pixelPath = path.join(
  verificationRoot,
  "PHASE_0E_R1_PIXEL_READBACK.json",
);
const failurePath = path.join(
  verificationRoot,
  "PHASE_0E_R1_BROWSER_FAILURE.json",
);
const screenshots = {
  default: path.join(verificationRoot, "phase0e-r1-editor-default.png"),
  selection: path.join(verificationRoot, "phase0e-r1-editor-selection.png"),
  nested_group: path.join(
    verificationRoot,
    "phase0e-r1-editor-nested-group.png",
  ),
  component_showcase: path.join(
    verificationRoot,
    "phase0e-r1-component-showcase.png",
  ),
  hundred_k_debug: path.join(verificationRoot, "phase0e-r1-100k-structure.png"),
};
const chrome =
  process.env.PHASE0E_CHROME ??
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";
const profile = path.join(
  os.tmpdir(),
  `phase0e-chrome-${process.pid}-${Date.now()}`,
);
const consoleErrors = [];
let chromeStderr = "";
let browserClient;
let pageClient;
let chromeProcess;
let preview;
let maxFallbackRebuildCountSeen = 0;
const fallbackCheckpoints = [];

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
        "Health payload was not Phase 0E",
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
    {
      last_error: lastError?.message,
      server: processState,
    },
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
        {
          message: error.message,
        },
      );
    }
    const contentType = response.headers.get("content-type") ?? "";
    if (!response.ok) {
      throw new HarnessFailure(
        "asset_http_error",
        `${pathname} returned HTTP ${response.status}`,
        {
          pathname,
          status: response.status,
        },
      );
    }
    if (!contentType.startsWith(mime)) {
      throw new HarnessFailure(
        "asset_mime_error",
        `${pathname} returned ${contentType}`,
        {
          pathname,
          expected: mime,
          actual: contentType,
        },
      );
    }
    result[pathname] = { status: response.status, content_type: contentType };
  }
  return result;
}

async function preparePreview() {
  const port = await reservePort();
  const url = new URL(`http://127.0.0.1:${port}/`);
  const processState = {
    exited: false,
    exit_code: null,
    stdout: "",
    stderr: "",
  };
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

async function waitFor(
  expression,
  timeoutMs = 30000,
  code = "condition_timeout",
) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      if (await evaluate(`Boolean(${expression})`)) return;
    } catch (error) {
      lastError = error;
    }
    await sleep(100);
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
      {
        proof,
        max_fallback_rebuild_count_seen: maxFallbackRebuildCountSeen,
      },
    );
  }
  return proof;
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

async function boundsForText(selector, text, exact = false) {
  const bounds = await evaluate(`(() => {
    const expected = ${JSON.stringify(text)};
    const element = [...document.querySelectorAll(${JSON.stringify(selector)})].find((candidate) =>
      ${exact ? "candidate.textContent?.trim() === expected" : "candidate.textContent?.includes(expected)"});
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    return { x: rect.left, y: rect.top, width: rect.width, height: rect.height,
      center_x: rect.left + rect.width / 2, center_y: rect.top + rect.height / 2 };
  })()`);
  if (!bounds)
    throw new HarnessFailure(
      "dom_text_missing",
      `No ${selector} contained ${text}`,
    );
  return bounds;
}

async function clickPoint(point, { modifiers = 0, clickCount = 1 } = {}) {
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: point.center_x,
    y: point.center_y,
    button: "left",
    buttons: 1,
    clickCount,
    modifiers,
  });
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: point.center_x,
    y: point.center_y,
    button: "left",
    buttons: 0,
    clickCount,
    modifiers,
  });
}

async function clickSelector(selector, options) {
  await clickPoint(await boundsForSelector(selector), options);
}

async function clickText(selector, text, options, exact = false) {
  await clickPoint(await boundsForText(selector, text, exact), options);
}

async function doubleClickText(selector, text) {
  const point = await boundsForText(selector, text, true);
  await clickPoint(point, { clickCount: 1 });
  await clickPoint(point, { clickCount: 2 });
}

async function dragFrom(point, dx, dy, moveCount = 12) {
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: point.center_x,
    y: point.center_y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  for (let index = 1; index <= moveCount; index += 1) {
    await pageClient.send("Input.dispatchMouseEvent", {
      type: "mouseMoved",
      x: point.center_x + (dx * index) / moveCount,
      y: point.center_y + (dy * index) / moveCount,
      button: "left",
      buttons: 1,
    });
  }
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: point.center_x + dx,
    y: point.center_y + dy,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
}

async function replaceInput(selector, value) {
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
  await pageClient.send("Input.dispatchKeyEvent", {
    type: "rawKeyDown",
    key: "Enter",
    code: "Enter",
    windowsVirtualKeyCode: 13,
  });
  await pageClient.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: "Enter",
    code: "Enter",
    windowsVirtualKeyCode: 13,
  });
}

async function pressEscape() {
  await pageClient.send("Input.dispatchKeyEvent", {
    type: "rawKeyDown",
    key: "Escape",
    code: "Escape",
    windowsVirtualKeyCode: 27,
  });
  await pageClient.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: "Escape",
    code: "Escape",
    windowsVirtualKeyCode: 27,
  });
}

async function capture(pathname) {
  const screenshot = await pageClient.send("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
    fromSurface: true,
  });
  await writeFile(pathname, Buffer.from(screenshot.data, "base64"));
}

async function overlayBounds() {
  return boundsForSelector(".selection-outline");
}

async function readPixelAtOverlayCenter() {
  return evaluate(`(() => {
    const overlay = document.querySelector(".selection-outline").getBoundingClientRect();
    const canvas = document.querySelector(".webgpu-canvas").getBoundingClientRect();
    return window.__phase0eReadPixel(
      overlay.left + overlay.width / 2 - canvas.left,
      overlay.top + overlay.height / 2 - canvas.top
    );
  })()`);
}

async function readPixelAtOverlayCorner() {
  return evaluate(`(() => {
    const overlay = document.querySelector(".selection-outline").getBoundingClientRect();
    const canvas = document.querySelector(".webgpu-canvas").getBoundingClientRect();
    return window.__phase0eReadPixel(overlay.right - 2 - canvas.left, overlay.top + 2 - canvas.top);
  })()`);
}

function matchesColor(pixel, expected, tolerance = 28) {
  return (
    Array.isArray(pixel?.rgba) &&
    pixel.rgba
      .slice(0, 3)
      .every((value, index) => Math.abs(value - expected[index]) <= tolerance)
  );
}

async function waitForSequenceAfter(sequence, timeout = 30000) {
  await waitFor(
    `window.__PHASE0E_PROOF__?.engine_sequence > ${sequence}`,
    timeout,
    "engine_response_timeout",
  );
}

try {
  await stat(chrome).catch(() => {
    throw new HarnessFailure(
      "chrome_not_found",
      `Chrome executable was not found: ${chrome}`,
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
      "--window-size=1440,1000",
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"], windowsHide: true },
  );
  chromeProcess.stderr.setEncoding("utf8");
  chromeProcess.stderr.on("data", (chunk) => {
    chromeStderr = `${chromeStderr}${chunk}`.slice(-20000);
  });

  const activePortFile = path.join(profile, "DevToolsActivePort");
  const [debugPort] = (await waitForFile(activePortFile)).trim().split(/\r?\n/);
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
  await pageClient.send("Log.enable");
  await pageClient.send("Emulation.setDeviceMetricsOverride", {
    width: 1440,
    height: 1000,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await waitForReady();
  const initial = await getProof();
  await capture(screenshots.default);

  await clickText(".tree-row", "Proof Rectangle");
  await waitFor(
    "window.__PHASE0E_PROOF__?.selection?.length === 1 && document.querySelector('.selection-outline')",
  );
  const rectanglePixel = await readPixelAtOverlayCenter();
  await capture(screenshots.selection);
  await clickText(".tree-row", "Proof Ellipse");
  await waitFor(
    "[...document.querySelectorAll('.tree-row')].some((row) => row.textContent?.includes('Proof Ellipse') && row.getAttribute('aria-selected') === 'true') && document.querySelector('.selection-outline')",
  );
  const ellipsePixel = await readPixelAtOverlayCenter();
  const ellipseCornerPixel = await readPixelAtOverlayCorner();
  await clickText(".tree-row", "Proof Rectangle");
  await waitFor(
    "[...document.querySelectorAll('.tree-row')].some((row) => row.textContent?.includes('Proof Rectangle') && row.getAttribute('aria-selected') === 'true') && document.querySelector('.selection-outline')",
  );

  const layerStateBefore = await getProof();
  await clickSelector("button[aria-label='Hide Proof Rectangle']");
  await waitFor(
    `document.querySelector("button[aria-label='Show Proof Rectangle']")`,
  );
  const hidden = await getProof();
  await clickSelector("button[aria-label='Show Proof Rectangle']");
  await waitFor(
    `document.querySelector("button[aria-label='Hide Proof Rectangle']")`,
  );
  const shown = await getProof();
  await clickSelector("button[aria-label='Lock Proof Rectangle']");
  await waitFor(
    `document.querySelector("button[aria-label='Unlock Proof Rectangle']")`,
  );
  const locked = await getProof();
  const lockedBoundsBefore = await overlayBounds();
  await dragFrom(lockedBoundsBefore, 48, 20, 16);
  await sleep(120);
  const lockedAfterAttempt = await getProof();
  const lockedBoundsAfter = await overlayBounds();
  await clickSelector("button[aria-label='Unlock Proof Rectangle']");
  await waitFor(
    `document.querySelector("button[aria-label='Lock Proof Rectangle']")`,
  );
  const unlocked = await getProof();

  const cameraRevisionBefore = await getProof();
  const cameraShell = await boundsForSelector("[data-testid='canvas-shell']");
  await clickSelector("[data-testid='tool-hand']");
  const panBefore = await getProof();
  await dragFrom(
    {
      center_x: cameraShell.x + cameraShell.width * 0.2,
      center_y: cameraShell.y + cameraShell.height * 0.25,
    },
    52,
    34,
    24,
  );
  await waitFor(
    `window.__PHASE0E_PROOF__?.engine_sequence > ${panBefore.engine_sequence} && window.__PHASE0E_PROOF__?.fsm === 'Idle'`,
  );
  const panAfter = await getProof();
  const zoomPoint = {
    x: cameraShell.x + cameraShell.width * 0.68,
    y: cameraShell.y + cameraShell.height * 0.42,
  };
  const zoomBefore = await getProof();
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mouseWheel",
    x: zoomPoint.x,
    y: zoomPoint.y,
    deltaX: 0,
    deltaY: -220,
  });
  await waitFor(
    `window.__PHASE0E_PROOF__?.engine_sequence > ${zoomBefore.engine_sequence} && Math.abs(window.__PHASE0E_PROOF__?.camera?.zoom - ${zoomBefore.camera.zoom}) > 0.01`,
  );
  const zoomAfter = await getProof();
  await clickSelector("[data-testid='tool-select']");
  const fitSelectionBefore = await getProof();
  await clickText("button", "Fit selection", undefined, true);
  await waitForSequenceAfter(fitSelectionBefore.engine_sequence);
  const fitSelectionAfter = await getProof();
  const fitDocumentBefore = await getProof();
  await clickText("button", "Fit document", undefined, true);
  await waitForSequenceAfter(fitDocumentBefore.engine_sequence);
  const fitDocumentAfter = await getProof();

  const moveBefore = await getProof();
  const moveBoundsBefore = await overlayBounds();
  await dragFrom(moveBoundsBefore, 72, 24, 132);
  await waitFor(
    `window.__PHASE0E_PROOF__?.history?.undo_depth === ${moveBefore.history.undo_depth + 1}`,
  );
  const moveAfter = await getProof();
  const moveBoundsAfter = await overlayBounds();

  const resizeBefore = await getProof();
  await dragFrom(
    await boundsForSelector("[data-testid='resize-handle']"),
    36,
    28,
    24,
  );
  await waitFor(
    `window.__PHASE0E_PROOF__?.history?.undo_depth === ${resizeBefore.history.undo_depth + 1}`,
  );
  const resizeAfter = await getProof();

  const rotateBefore = await getProof();
  await dragFrom(
    await boundsForSelector("[data-testid='rotate-handle']"),
    42,
    18,
    24,
  );
  await waitFor(
    `window.__PHASE0E_PROOF__?.history?.undo_depth === ${rotateBefore.history.undo_depth + 1}`,
  );
  const rotateAfter = await getProof();

  const inspectorBefore = await getProof();
  await replaceInput("input[aria-label='X']", 32);
  await waitFor(
    `window.__PHASE0E_PROOF__?.history?.undo_depth === ${inspectorBefore.history.undo_depth + 1}`,
  );
  const inspectorAfter = await getProof();

  const rollbackHistory = inspectorAfter.history.undo_depth;
  const rollbackBoundsBefore = await overlayBounds();
  const rollbackHandle = await boundsForSelector(
    "[data-testid='resize-handle']",
  );
  const rollbackStartSequence = inspectorAfter.engine_sequence;
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: rollbackHandle.center_x,
    y: rollbackHandle.center_y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x: rollbackHandle.center_x + 110,
    y: rollbackHandle.center_y + 36,
    button: "left",
    buttons: 1,
  });
  await waitFor(
    `window.__PHASE0E_PROOF__?.history?.transaction_active === true && window.__PHASE0E_PROOF__?.engine_sequence > ${rollbackStartSequence}`,
  );
  await pressEscape();
  await waitFor(
    "window.__PHASE0E_PROOF__?.history?.transaction_active === false && window.__PHASE0E_PROOF__?.fsm === 'Idle'",
  );
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: rollbackHandle.center_x + 110,
    y: rollbackHandle.center_y + 36,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
  const rollbackAfter = await getProof();
  const rollbackBoundsAfter = await overlayBounds();

  const pointerCancelBefore = await getProof();
  const pointerCancelBoundsBefore = await overlayBounds();
  const pointerCancelHandle = await boundsForSelector(
    "[data-testid='resize-handle']",
  );
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: pointerCancelHandle.center_x,
    y: pointerCancelHandle.center_y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  for (let index = 1; index <= 12; index += 1) {
    await pageClient.send("Input.dispatchMouseEvent", {
      type: "mouseMoved",
      x: pointerCancelHandle.center_x + index * 8,
      y: pointerCancelHandle.center_y + index * 3,
      button: "left",
      buttons: 1,
    });
  }
  await waitFor(
    "window.__PHASE0E_PROOF__?.history?.transaction_active === true && window.__PHASE0E_PROOF__?.interaction_queue?.in_flight === true && (window.__PHASE0E_PROOF__?.interaction_queue?.latest === true || window.__PHASE0E_PROOF__?.interaction_queue?.scheduled === true)",
  );
  const pointerCancelQueued = await getProof();
  await evaluate(`(() => {
    const shell = document.querySelector("[data-testid='canvas-shell']");
    const pointerId = window.__PHASE0E_PROOF__?.interaction_active?.pointer_id ?? 1;
    shell.dispatchEvent(new PointerEvent("pointercancel", { bubbles: true, cancelable: true, pointerId, pointerType: "mouse", isPrimary: true }));
  })()`);
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: pointerCancelHandle.center_x + 96,
    y: pointerCancelHandle.center_y + 36,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
  await waitFor(
    "window.__PHASE0E_PROOF__?.history?.transaction_active === false && window.__PHASE0E_PROOF__?.interaction_queue?.in_flight === false && window.__PHASE0E_PROOF__?.interaction_queue?.scheduled === false && window.__PHASE0E_PROOF__?.interaction_queue?.latest === false && window.__PHASE0E_PROOF__?.fsm === 'Idle'",
  );
  const pointerCancelAfter = await getProof();
  await sleep(250);
  const pointerCancelSettled = await getProof();
  const pointerCancelBoundsAfter = await overlayBounds();

  await clickSelector("[data-testid='undo']");
  await waitFor(`window.__PHASE0E_PROOF__?.history?.redo_depth > 0`);
  const undo = await getProof();
  await clickSelector("[data-testid='redo']");
  await waitFor(`window.__PHASE0E_PROOF__?.history?.redo_depth === 0`);
  const redo = await getProof();

  await clickSelector("[data-testid='tool-ellipse']");
  const shell = await boundsForSelector("[data-testid='canvas-shell']");
  const createBefore = await getProof();
  await dragFrom(
    {
      center_x: shell.x + shell.width * 0.72,
      center_y: shell.y + shell.height * 0.72,
    },
    84,
    58,
    32,
  );
  await waitFor(
    `window.__PHASE0E_PROOF__?.history?.undo_depth === ${createBefore.history.undo_depth + 1}`,
  );
  const createAfter = await getProof();

  await clickText(".tree-row", "Proof Rectangle");
  await clickText(".tree-row", "Proof Ellipse", { modifiers: 8 });
  await waitFor("window.__PHASE0E_PROOF__?.selection?.length === 2");
  const groupBefore = await getProof();
  await clickSelector("[data-testid='group']");
  await waitFor(
    `window.__PHASE0E_PROOF__?.selection?.length === 1 && window.__PHASE0E_PROOF__?.history?.undo_depth === ${groupBefore.history.undo_depth + 1}`,
  );
  const grouped = await getProof();
  await doubleClickText(".tree-row", "Group");
  await waitFor("Boolean(window.__PHASE0E_PROOF__?.nested_edit_root)");
  const nested = await getProof();
  await capture(screenshots.nested_group);
  await clickText(".tree-row", "Proof Rectangle");
  await waitFor(
    "window.__PHASE0E_PROOF__?.selection?.length === 1 && document.querySelector('.selection-outline')",
  );
  const nestedMoveBefore = await getProof();
  const nestedBoundsBefore = await overlayBounds();
  await dragFrom(nestedBoundsBefore, 28, 14, 24);
  await waitFor(
    `window.__PHASE0E_PROOF__?.history?.undo_depth === ${nestedMoveBefore.history.undo_depth + 1}`,
  );
  const nestedMoveAfter = await getProof();
  const nestedBoundsAfter = await overlayBounds();
  await clickText("button", "Exit group", undefined, true);
  await waitFor("!window.__PHASE0E_PROOF__?.nested_edit_root");
  await clickText(".tree-row", "Group", undefined, true);
  await waitFor(
    `window.__PHASE0E_PROOF__?.selection?.[0] === '${grouped.selection[0]}'`,
  );
  const ungroupBefore = await getProof();
  await clickSelector("[data-testid='ungroup']");
  await waitFor(
    `window.__PHASE0E_PROOF__?.history?.undo_depth === ${ungroupBefore.history.undo_depth + 1}`,
  );
  const ungrouped = await getProof();

  await clickSelector("[data-testid='show-components']");
  await waitFor(
    "document.querySelector('[data-testid=component-showcase]') && document.activeElement?.getAttribute('aria-label') === 'Close showcase'",
  );
  await capture(screenshots.component_showcase);
  const showcaseInitial = await evaluate(`(() => {
    const dialog = document.querySelector("[data-testid='component-showcase']");
    return {
      labelled: dialog?.getAttribute("aria-labelledby") === "showcase-title" && Boolean(document.getElementById("showcase-title")),
      select: Boolean(dialog?.querySelector("select")),
      tabs: dialog?.querySelectorAll("[role='tab']").length,
      radios: dialog?.querySelectorAll("[role='radio']").length,
      menu_trigger: Boolean(dialog?.querySelector("[aria-haspopup='menu']")),
      switch_control: Boolean(dialog?.querySelector("[role='switch']")),
      checkbox: Boolean(dialog?.querySelector("input[type='checkbox']")),
      badge: Boolean(dialog?.querySelector(".badge")),
      initial_focus: document.activeElement?.getAttribute("aria-label"),
    };
  })()`);
  await pageClient.send("Input.dispatchKeyEvent", {
    type: "rawKeyDown",
    key: "Tab",
    code: "Tab",
    windowsVirtualKeyCode: 9,
    modifiers: 8,
  });
  await pageClient.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: "Tab",
    code: "Tab",
    windowsVirtualKeyCode: 9,
    modifiers: 8,
  });
  const showcaseFocusContained = await evaluate(
    "document.querySelector('[data-testid=component-showcase]').contains(document.activeElement)",
  );
  await clickText("[role='radio']", "Design", undefined, true);
  await pageClient.send("Input.dispatchKeyEvent", {
    type: "rawKeyDown",
    key: "ArrowRight",
    code: "ArrowRight",
    windowsVirtualKeyCode: 39,
  });
  await pageClient.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: "ArrowRight",
    code: "ArrowRight",
    windowsVirtualKeyCode: 39,
  });
  await waitFor(
    "document.activeElement?.textContent === 'Inspect' && document.activeElement?.getAttribute('aria-checked') === 'true' && document.activeElement?.getAttribute('tabindex') === '0'",
  );
  const showcaseRoving = await evaluate(
    "document.activeElement?.textContent === 'Inspect' && document.activeElement?.getAttribute('aria-checked') === 'true' && document.activeElement?.getAttribute('tabindex') === '0'",
  );
  await pressEscape();
  await waitFor(
    "!document.querySelector('[data-testid=component-showcase]') && document.activeElement?.getAttribute('data-testid') === 'show-components'",
  );
  const invalidNumericBefore = await getProof();
  await replaceInput("input[aria-label='X']", "not-a-number");
  await waitFor(
    `document.querySelector("input[aria-label='X']")?.getAttribute('aria-invalid') === 'true' && document.querySelector('[role=alert]')?.textContent?.includes('numeric_non_finite')`,
  );
  const invalidNumericAfter = await getProof();
  const numericValidation = await evaluate(`({
    code: document.querySelector("input[aria-label='X']")?.getAttribute("data-validation-code"),
    aria_invalid: document.querySelector("input[aria-label='X']")?.getAttribute("aria-invalid"),
    alert: document.querySelector("[role='alert']")?.textContent,
  })`);

  await clickSelector("[data-testid='debug-toggle']");
  await clickSelector("[data-testid='fixture-10k']");
  await waitFor(
    "window.__PHASE0E_PROOF__?.fixture === 'BENCH-B' && window.__PHASE0E_PROOF__?.projection_nodes === 10001",
    120000,
    "ten_k_load_timeout",
  );
  const tenK = await getProof();
  const tenKTargets = [1, 5001, 10000].map(fixtureNodeId);
  const tenKGroupId = "00000000-0000-0000-e200-000000000001";
  const tenKGroupBefore = await getProof();
  await evaluate(
    `window.__phase0eSend("command", ${JSON.stringify({ command: { kind: "group", group_id: tenKGroupId, name: "R1 10k Group", targets: tenKTargets } })})`,
  );
  await waitForSequenceAfter(tenKGroupBefore.engine_sequence, 60000);
  const tenKGrouped = await getProof();
  await evaluate(
    `window.__phase0eSend("command", ${JSON.stringify({ command: { kind: "ungroup", node_id: tenKGroupId } })})`,
  );
  await waitForSequenceAfter(tenKGrouped.engine_sequence, 60000);
  const tenKUngrouped = await getProof();
  await clickText(".tree-row", "Rectangle 0");
  await waitFor(
    "window.__PHASE0E_PROOF__?.selection?.length === 1 && window.__PHASE0E_PROOF__?.primary_node?.name === 'Rectangle 0'",
  );
  const tenKBeforeEdit = await getProof();
  const tenKOldX = tenKBeforeEdit.primary_node.local_transform[4];
  const tenKExpectedX = tenKOldX + 17;
  const tenKBoundsBefore = await overlayBounds();
  await replaceInput("input[aria-label='X']", tenKExpectedX);
  await waitFor(
    `window.__PHASE0E_PROOF__?.revisions?.document === ${tenKBeforeEdit.revisions.document + 1} && Math.abs(window.__PHASE0E_PROOF__?.primary_node?.local_transform?.[4] - ${tenKExpectedX}) < 0.001`,
    60000,
    "ten_k_real_edit_timeout",
  );
  const tenKAfterEdit = await getProof();
  const tenKBoundsAfter = await overlayBounds();

  const restartBefore = await getProof();
  await clickSelector("[data-testid='restart-worker']");
  await waitFor(
    "window.__PHASE0E_PROOF__?.fixture === 'PREVIEW' && window.__PHASE0E_PROOF__?.heartbeat >= 1 && window.__PHASE0E_PROOF__?.response_gpu_overlay_sequence_match === true",
    60000,
    "worker_restart_timeout",
  );
  const restarted = await getProof();
  await clickSelector("[data-testid='fixture-100k']");
  await waitFor(
    "window.__PHASE0E_PROOF__?.fixture === 'BENCH-C' && window.__PHASE0E_PROOF__?.projection_nodes === 100001",
    240000,
    "hundred_k_load_timeout",
  );
  const hundredK = await getProof();
  const hundredKTargets = [1, 50001, 100000].map(fixtureNodeId);
  const hundredKGroupId = "00000000-0000-0000-e300-000000000001";
  const hundredKGroupBefore = await getProof();
  await evaluate(
    `window.__phase0eSend("command", ${JSON.stringify({ command: { kind: "group", group_id: hundredKGroupId, name: "R1 100k Group", targets: hundredKTargets } })})`,
  );
  await waitForSequenceAfter(hundredKGroupBefore.engine_sequence, 120000);
  const hundredKGrouped = await getProof();
  await capture(screenshots.hundred_k_debug);
  await evaluate(
    `window.__phase0eSend("command", ${JSON.stringify({ command: { kind: "ungroup", node_id: hundredKGroupId } })})`,
  );
  await waitForSequenceAfter(hundredKGrouped.engine_sequence, 120000);
  const hundredKUngrouped = await getProof();
  await clickText(".tree-row", "Rectangle 0");
  await waitFor(
    "window.__PHASE0E_PROOF__?.selection?.length === 1 && window.__PHASE0E_PROOF__?.primary_node?.name === 'Rectangle 0'",
  );
  const hundredKBeforeEdit = await getProof();
  const hundredKOldX = hundredKBeforeEdit.primary_node.local_transform[4];
  const hundredKExpectedX = hundredKOldX + 17;
  const hundredKBoundsBefore = await overlayBounds();
  await replaceInput("input[aria-label='X']", hundredKExpectedX);
  await waitFor(
    `window.__PHASE0E_PROOF__?.revisions?.document === ${hundredKBeforeEdit.revisions.document + 1} && Math.abs(window.__PHASE0E_PROOF__?.primary_node?.local_transform?.[4] - ${hundredKExpectedX}) < 0.001`,
    60000,
    "hundred_k_real_edit_timeout",
  );
  const hundredKAfterEdit = await getProof();
  const hundredKBoundsAfter = await overlayBounds();

  const final = await getProof();
  const devices = gpuInfo.gpu?.devices ?? [];
  const activeDevice =
    devices.find((device) => device.active) ?? devices[0] ?? {};
  const activeDeviceText = JSON.stringify(activeDevice).toLowerCase();
  const actualHardware =
    devices.length > 0 &&
    !activeDeviceText.includes("swiftshader") &&
    !activeDeviceText.includes("software rasterizer");
  const expected = {
    background: [9, 14, 20],
    rectangle: [51, 148, 245],
    ellipse: [245, 115, 64],
  };
  const pixelAssertions = {
    rectangle_center_matches_shader: matchesColor(
      rectanglePixel,
      expected.rectangle,
    ),
    ellipse_center_matches_shader: matchesColor(ellipsePixel, expected.ellipse),
    ellipse_aabb_corner_is_background: matchesColor(
      ellipseCornerPixel,
      expected.background,
    ),
    alpha_is_opaque: [rectanglePixel, ellipsePixel, ellipseCornerPixel].every(
      (pixel) => pixel?.rgba?.[3] === 255,
    ),
  };
  const pixelReadback = {
    phase: "0E-R1",
    captured_at_utc: new Date().toISOString(),
    kind: "actual-webgpu-texture-readback",
    expected,
    samples: {
      rectangle_center: rectanglePixel,
      ellipse_center: ellipsePixel,
      ellipse_aabb_corner: ellipseCornerPixel,
    },
    assertions: pixelAssertions,
    all_passed: Object.values(pixelAssertions).every(Boolean),
  };
  await writeFile(
    pixelPath,
    `${JSON.stringify(pixelReadback, null, 2)}\n`,
    "utf8",
  );

  const checks = {
    self_contained_server: Boolean(preview.server) && preview.health.ok,
    actual_hardware_webgpu: initial.actual_webgpu === true && actualHardware,
    dedicated_worker_wasm_heartbeat:
      initial.worker_runtime_owner === "dedicated-worker" &&
      initial.heartbeat >= 1,
    response_gpu_overlay_ordered:
      final.response_gpu_overlay_sequence_match === true,
    actual_pixel_readback: pixelReadback.all_passed,
    layers_visibility_roundtrip:
      hidden.history.undo_depth === layerStateBefore.history.undo_depth + 1 &&
      shown.history.undo_depth === layerStateBefore.history.undo_depth + 2,
    layers_lock_blocks_canvas_mutation:
      lockedAfterAttempt.history.undo_depth === locked.history.undo_depth &&
      Math.abs(lockedBoundsAfter.center_x - lockedBoundsBefore.center_x) < 2 &&
      Math.abs(lockedBoundsAfter.center_y - lockedBoundsBefore.center_y) < 2 &&
      unlocked.history.undo_depth === locked.history.undo_depth + 1,
    camera_pan_zoom_fit_without_document_revision:
      panAfter.engine_sequence > panBefore.engine_sequence &&
      Math.abs(
        panAfter.camera.center[0] -
          panBefore.camera.center[0] +
          52 / panBefore.camera.zoom,
      ) < 1.5 &&
      Math.abs(
        panAfter.camera.center[1] -
          panBefore.camera.center[1] +
          34 / panBefore.camera.zoom,
      ) < 1.5 &&
      Math.abs(zoomAfter.camera.zoom - zoomBefore.camera.zoom) > 0.01 &&
      fitSelectionAfter.engine_sequence > fitSelectionBefore.engine_sequence &&
      fitDocumentAfter.engine_sequence > fitDocumentBefore.engine_sequence &&
      JSON.stringify(fitDocumentAfter.revisions) ===
        JSON.stringify(cameraRevisionBefore.revisions),
    zoom_around_pointer_preserves_anchor: (() => {
      const beforeWorldX =
        (zoomPoint.x - cameraShell.x - zoomBefore.camera.viewport[0] / 2) /
          zoomBefore.camera.zoom +
        zoomBefore.camera.center[0];
      const beforeWorldY =
        (zoomPoint.y - cameraShell.y - zoomBefore.camera.viewport[1] / 2) /
          zoomBefore.camera.zoom +
        zoomBefore.camera.center[1];
      const afterWorldX =
        (zoomPoint.x - cameraShell.x - zoomAfter.camera.viewport[0] / 2) /
          zoomAfter.camera.zoom +
        zoomAfter.camera.center[0];
      const afterWorldY =
        (zoomPoint.y - cameraShell.y - zoomAfter.camera.viewport[1] / 2) /
          zoomAfter.camera.zoom +
        zoomAfter.camera.center[1];
      return (
        Math.abs(beforeWorldX - afterWorldX) < 0.25 &&
        Math.abs(beforeWorldY - afterWorldY) < 0.25
      );
    })(),
    select_move_single_history:
      moveAfter.history.undo_depth === moveBefore.history.undo_depth + 1 &&
      Math.abs(moveBoundsAfter.center_x - moveBoundsBefore.center_x) > 20,
    pointer_updates_coalesced:
      moveAfter.ui.pointerRawIntents >= 128 &&
      moveAfter.ui.pointerRequestsSent < moveAfter.ui.pointerRawIntents &&
      moveAfter.ui.pointerRequestsCoalesced > 0,
    resize_single_history:
      resizeAfter.history.undo_depth === resizeBefore.history.undo_depth + 1,
    rotate_single_history:
      rotateAfter.history.undo_depth === rotateBefore.history.undo_depth + 1,
    inspector_command_history:
      inspectorAfter.history.undo_depth ===
      inspectorBefore.history.undo_depth + 1,
    escape_rolls_back_without_history:
      rollbackAfter.history.undo_depth === rollbackHistory &&
      Math.abs(rollbackBoundsAfter.center_x - rollbackBoundsBefore.center_x) <
        2 &&
      Math.abs(rollbackBoundsAfter.center_y - rollbackBoundsBefore.center_y) <
        2,
    pointercancel_serialized_rollback:
      pointerCancelQueued.interaction_queue.in_flight === true &&
      (pointerCancelQueued.interaction_queue.latest === true ||
        pointerCancelQueued.interaction_queue.scheduled === true) &&
      pointerCancelAfter.history.undo_depth ===
        pointerCancelBefore.history.undo_depth &&
      pointerCancelAfter.history.transaction_active === false &&
      pointerCancelAfter.interaction_queue.in_flight === false &&
      pointerCancelAfter.interaction_queue.scheduled === false &&
      pointerCancelAfter.interaction_queue.latest === false &&
      JSON.stringify(pointerCancelAfter.primary_node) ===
        JSON.stringify(pointerCancelBefore.primary_node) &&
      pointerCancelSettled.engine_sequence ===
        pointerCancelAfter.engine_sequence &&
      Math.abs(
        pointerCancelBoundsAfter.center_x - pointerCancelBoundsBefore.center_x,
      ) < 2 &&
      Math.abs(
        pointerCancelBoundsAfter.center_y - pointerCancelBoundsBefore.center_y,
      ) < 2,
    undo_redo_dom_controls:
      undo.history.redo_depth > 0 && redo.history.redo_depth === 0,
    create_shape_dom_pointer:
      createAfter.history.undo_depth === createBefore.history.undo_depth + 1,
    group_atomic_single_history:
      grouped.history.undo_depth === groupBefore.history.undo_depth + 1 &&
      grouped.selection.length === 1,
    nested_group_edit_mode: Boolean(nested.nested_edit_root),
    nested_transform_preserves_world_interaction:
      nestedMoveAfter.history.undo_depth ===
        nestedMoveBefore.history.undo_depth + 1 &&
      Math.abs(nestedBoundsAfter.center_x - nestedBoundsBefore.center_x) > 10 &&
      Math.abs(nestedBoundsAfter.center_y - nestedBoundsBefore.center_y) > 5,
    ungroup_atomic_single_history:
      ungrouped.history.undo_depth === ungroupBefore.history.undo_depth + 1,
    group_ungroup_incremental_no_fallback: [grouped, ungrouped].every(
      (checkpoint) =>
        checkpoint.fallback_rebuild_count === 0 &&
        checkpoint.render_delta.scene_full_rebuilds === 0 &&
        checkpoint.render_delta.render_full_rebuilds === 0 &&
        checkpoint.metrics.render_items_cloned === 0,
    ),
    component_showcase_audited:
      showcaseInitial.labelled === true &&
      showcaseInitial.select === true &&
      showcaseInitial.tabs >= 3 &&
      showcaseInitial.radios >= 3 &&
      showcaseInitial.menu_trigger === true &&
      showcaseInitial.switch_control === true &&
      showcaseInitial.checkbox === true &&
      showcaseInitial.badge === true &&
      showcaseInitial.initial_focus === "Close showcase" &&
      showcaseFocusContained === true &&
      showcaseRoving === true,
    numeric_invalid_typed_no_worker_mutation:
      numericValidation.code === "numeric_non_finite" &&
      numericValidation.aria_invalid === "true" &&
      numericValidation.alert.includes("numeric_non_finite") &&
      JSON.stringify(invalidNumericAfter.revisions) ===
        JSON.stringify(invalidNumericBefore.revisions) &&
      invalidNumericAfter.history.undo_depth ===
        invalidNumericBefore.history.undo_depth,
    ten_k_virtualized:
      tenK.fixture === "BENCH-B" &&
      tenK.projection_nodes === 10001 &&
      tenK.ui.mountedRows <= 30,
    ten_k_group_ungroup_bounded:
      [tenKGrouped, tenKUngrouped].every(
        (checkpoint) =>
          checkpoint.fallback_rebuild_count === 0 &&
          checkpoint.metrics.document_full_clones === 0 &&
          checkpoint.render_delta.scene_full_rebuilds === 0 &&
          checkpoint.render_delta.render_full_rebuilds === 0 &&
          checkpoint.metrics.render_items_cloned === 0 &&
          checkpoint.metrics.ui_full_snapshots === 0 &&
          checkpoint.metrics.ui_structural_operations === 1 &&
          checkpoint.ui.hierarchyFullRebuilds ===
            tenKGroupBefore.ui.hierarchyFullRebuilds &&
          checkpoint.ui.mountedRows <= 30,
      ) &&
      tenKGrouped.metrics.scene_nodes_visited === 8 &&
      tenKUngrouped.metrics.scene_nodes_visited === 6,
    ten_k_single_leaf_projection_delta:
      Math.abs(tenKAfterEdit.primary_node.local_transform[4] - tenKExpectedX) <
        0.001 &&
      tenKAfterEdit.revisions.document ===
        tenKBeforeEdit.revisions.document + 1 &&
      tenKAfterEdit.revisions.scene === tenKBeforeEdit.revisions.scene + 1 &&
      tenKAfterEdit.revisions.render === tenKBeforeEdit.revisions.render + 1 &&
      tenKAfterEdit.history.undo_depth ===
        tenKBeforeEdit.history.undo_depth + 1 &&
      tenKAfterEdit.metrics.ui_delta_nodes === 1 &&
      tenKAfterEdit.metrics.ui_full_snapshots === 0 &&
      tenKAfterEdit.ui.projectionFullSnapshots ===
        tenKBeforeEdit.ui.projectionFullSnapshots &&
      tenKAfterEdit.ui.layersFullSerializes ===
        tenKBeforeEdit.ui.layersFullSerializes &&
      tenKAfterEdit.ui.hierarchyRebuilds ===
        tenKBeforeEdit.ui.hierarchyRebuilds &&
      tenKAfterEdit.ui.projectionDeltaNodes -
        tenKBeforeEdit.ui.projectionDeltaNodes ===
        1 &&
      tenKAfterEdit.render_delta.dirty_slots === 1 &&
      tenKAfterEdit.metrics.instance_upload_bytes === 48 &&
      tenKAfterEdit.render_delta.upload_bytes === 52 &&
      tenKAfterEdit.resources.dirty_record_stride_bytes === 52 &&
      tenKAfterEdit.binary.dirty_instances === true &&
      tenKAfterEdit.metrics.render_items_cloned === 0 &&
      tenKAfterEdit.metrics.full_render_model_scans === 0 &&
      Math.abs(tenKBoundsAfter.center_x - tenKBoundsBefore.center_x) > 1,
    worker_restart_generation_safe:
      restarted.fixture === "PREVIEW" &&
      restarted.heartbeat >= 1 &&
      restarted.response_gpu_overlay_sequence_match === true &&
      restartBefore.engine_sequence > restarted.engine_sequence,
    hundred_k_virtualized_and_loaded:
      hundredK.fixture === "BENCH-C" &&
      hundredK.projection_nodes === 100001 &&
      hundredK.ui.mountedRows <= 30,
    hundred_k_group_ungroup_bounded:
      [hundredKGrouped, hundredKUngrouped].every(
        (checkpoint) =>
          checkpoint.fallback_rebuild_count === 0 &&
          checkpoint.metrics.document_full_clones === 0 &&
          checkpoint.metrics.ui_full_snapshots === 0 &&
          checkpoint.metrics.ui_structural_operations === 1 &&
          checkpoint.render_delta.scene_full_rebuilds === 0 &&
          checkpoint.render_delta.render_full_rebuilds === 0 &&
          checkpoint.metrics.render_items_cloned === 0 &&
          checkpoint.ui.hierarchyFullRebuilds ===
            hundredKGroupBefore.ui.hierarchyFullRebuilds &&
          checkpoint.ui.mountedRows <= 30,
      ) &&
      hundredKGrouped.metrics.scene_nodes_visited ===
        tenKGrouped.metrics.scene_nodes_visited &&
      hundredKUngrouped.metrics.scene_nodes_visited ===
        tenKUngrouped.metrics.scene_nodes_visited &&
      hundredKGrouped.metrics.ui_nodes_serialized ===
        tenKGrouped.metrics.ui_nodes_serialized &&
      hundredKUngrouped.metrics.ui_nodes_serialized ===
        tenKUngrouped.metrics.ui_nodes_serialized,
    hundred_k_single_leaf_real_edit:
      Math.abs(
        hundredKAfterEdit.primary_node.local_transform[4] - hundredKExpectedX,
      ) < 0.001 &&
      hundredKAfterEdit.revisions.document ===
        hundredKBeforeEdit.revisions.document + 1 &&
      hundredKAfterEdit.revisions.scene ===
        hundredKBeforeEdit.revisions.scene + 1 &&
      hundredKAfterEdit.revisions.render ===
        hundredKBeforeEdit.revisions.render + 1 &&
      hundredKAfterEdit.history.undo_depth ===
        hundredKBeforeEdit.history.undo_depth + 1 &&
      hundredKAfterEdit.metrics.ui_delta_nodes === 1 &&
      hundredKAfterEdit.metrics.ui_full_snapshots === 0 &&
      hundredKAfterEdit.ui.projectionDeltaNodes -
        hundredKBeforeEdit.ui.projectionDeltaNodes ===
        1 &&
      hundredKAfterEdit.ui.layersFullSerializes ===
        hundredKBeforeEdit.ui.layersFullSerializes &&
      hundredKAfterEdit.ui.hierarchyRebuilds ===
        hundredKBeforeEdit.ui.hierarchyRebuilds &&
      hundredKAfterEdit.render_delta.dirty_slots === 1 &&
      hundredKAfterEdit.metrics.instance_upload_bytes === 48 &&
      hundredKAfterEdit.render_delta.upload_bytes === 52 &&
      hundredKAfterEdit.resources.dirty_record_stride_bytes === 52 &&
      hundredKAfterEdit.binary.dirty_instances === true &&
      hundredKAfterEdit.metrics.render_items_cloned === 0 &&
      hundredKAfterEdit.metrics.full_render_model_scans === 0 &&
      Math.abs(hundredKBoundsAfter.center_x - hundredKBoundsBefore.center_x) >
        1,
    revisions_converged:
      final.revisions.document === final.revisions.scene &&
      final.revisions.scene === final.revisions.render,
    fallback_rebuilds_zero:
      maxFallbackRebuildCountSeen === 0 &&
      fallbackCheckpoints.every(
        (checkpoint) => checkpoint.fallback_rebuild_count === 0,
      ),
    gpu_validation_errors_zero: final.gpu_validation_errors === 0,
    console_errors_zero:
      consoleErrors.length === 0 && final.console_errors === 0,
  };
  const screenshotInfo = {};
  for (const [name, pathname] of Object.entries(screenshots)) {
    const info = await stat(pathname);
    screenshotInfo[name] = {
      path: path.relative(workspace, pathname).replaceAll("\\", "/"),
      bytes: info.size,
    };
  }
  const proof = {
    phase: "0E-R1",
    captured_at_utc: new Date().toISOString(),
    proof_kind: "actual-hardware-browser",
    preview_url: preview.url,
    harness: {
      self_contained_server: true,
      server_port: preview.port,
      health: preview.health,
      assets: preview.assets,
      new_chrome_process: true,
      temporary_chrome_profile: profile,
      cleanup_required: true,
    },
    browser: {
      product: version.Browser,
      user_agent: version["User-Agent"],
      protocol_version: version["Protocol-Version"],
      mode: "headless=new with --enable-unsafe-webgpu; no mock and no Canvas2D fallback",
    },
    hardware: {
      actual_hardware: actualHardware,
      active_device: activeDevice,
      devices,
      aux_attributes: gpuInfo.gpu?.auxAttributes ?? {},
      feature_status: gpuInfo.gpu?.featureStatus ?? {},
    },
    runtime: final,
    scenarios: {
      initial,
      layers: {
        before: layerStateBefore,
        hidden,
        shown,
        locked,
        locked_after_attempt: lockedAfterAttempt,
        unlocked,
      },
      camera: {
        pan_before: panBefore,
        pan_after: panAfter,
        zoom_before: zoomBefore,
        zoom_after: zoomAfter,
        fit_selection: fitSelectionAfter,
        fit_document: fitDocumentAfter,
      },
      move: {
        before: moveBefore,
        after: moveAfter,
        dom_pointer_events_dispatched: 132,
      },
      resize: resizeAfter,
      rotate: rotateAfter,
      inspector: inspectorAfter,
      rollback: rollbackAfter,
      pointercancel: {
        before: pointerCancelBefore,
        queued: pointerCancelQueued,
        after: pointerCancelAfter,
        settled: pointerCancelSettled,
      },
      ui_audit: {
        showcase: showcaseInitial,
        focus_contained: showcaseFocusContained,
        roving_radio: showcaseRoving,
        numeric_validation: numericValidation,
        invalid_before: invalidNumericBefore,
        invalid_after: invalidNumericAfter,
      },
      undo,
      redo,
      create: createAfter,
      group: grouped,
      nested,
      nested_move: nestedMoveAfter,
      ungroup: ungrouped,
      ten_k: tenK,
      ten_k_group: tenKGrouped,
      ten_k_ungroup: tenKUngrouped,
      ten_k_single_leaf: tenKAfterEdit,
      worker_restart: restarted,
      hundred_k: hundredK,
      hundred_k_group: hundredKGrouped,
      hundred_k_ungroup: hundredKUngrouped,
      hundred_k_single_leaf: hundredKAfterEdit,
    },
    pixel_readback_artifact: path
      .relative(workspace, pixelPath)
      .replaceAll("\\", "/"),
    max_fallback_rebuild_count_seen: maxFallbackRebuildCountSeen,
    fallback_checkpoints: fallbackCheckpoints,
    screenshots: screenshotInfo,
    console_errors: consoleErrors,
    checks,
    all_passed: Object.values(checks).every(Boolean),
  };
  await writeFile(proofPath, `${JSON.stringify(proof, null, 2)}\n`, "utf8");
  await rm(failurePath, { force: true });
  console.log(
    `Phase 0E-R1 actual browser proof: ${proof.all_passed ? "PASS" : "FAIL"}`,
  );
  console.log(`browser=${version.Browser}`);
  console.log(
    `gpu=${activeDevice.deviceString ?? activeDevice.deviceId ?? "unknown"}`,
  );
  console.log(`proof=${proofPath}`);
  console.log(`screenshots=${Object.keys(screenshots).length}`);
  if (!proof.all_passed) {
    console.log(JSON.stringify(checks, null, 2));
    process.exitCode = 1;
  }
} catch (error) {
  const browserDiagnostics = pageClient
    ? await evaluate(`({
        body_ready: document.body?.dataset.ready ?? null,
        state: window.__phase0eState ?? null,
        proof: window.__PHASE0E_PROOF__ ?? null,
        navigator_gpu: Boolean(navigator.gpu),
      })`).catch((diagnosticError) => ({
        diagnostic_error: diagnosticError.message,
      }))
    : null;
  const failure = {
    phase: "0E-R1",
    captured_at_utc: new Date().toISOString(),
    code: error?.code ?? "browser_test_failed",
    message: error?.message ?? String(error),
    details: error?.details ?? null,
    preview_url: preview?.url ?? null,
    preview_health: preview?.health ?? null,
    preview_assets: preview?.assets ?? null,
    preview_server: preview?.processState ?? null,
    browser: browserDiagnostics,
    console_errors: consoleErrors,
    chrome_stderr_tail: chromeStderr,
  };
  await writeFile(
    failurePath,
    `${JSON.stringify(failure, null, 2)}\n`,
    "utf8",
  ).catch(() => {});
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

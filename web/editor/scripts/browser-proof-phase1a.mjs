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
const proofPath = path.join(verificationRoot, "PHASE_1A_BROWSER_PROOF.json");
const pixelPath = path.join(
  verificationRoot,
  "PHASE_1A_PIXEL_READBACK.json",
);
const failurePath = path.join(
  verificationRoot,
  "PHASE_1A_BROWSER_FAILURE.json",
);
const screenshots = {
  default: path.join(verificationRoot, "phase1a-editor-default.png"),
  selection: path.join(verificationRoot, "phase1a-frame-4k.png"),
  nested_group: path.join(
    verificationRoot,
    "phase1a-appearance-aa.png",
  ),
  component_showcase: path.join(
    verificationRoot,
    "phase1a-dpr-zoom-matrix.png",
  ),
  color_contract: path.join(verificationRoot, "phase1a-color-contract.png"),
  affine_parity: path.join(verificationRoot, "phase1a-affine-parity.png"),
  hundred_k_debug: path.join(verificationRoot, "phase1a-10k-comparison.png"),
};
const chrome =
  process.env.PHASE0E_CHROME ??
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";
const profile = path.join(
  os.tmpdir(),
  `phase1a-chrome-${process.pid}-${Date.now()}`,
);
const consoleErrors = [];
let chromeStderr = "";
let browserClient;
let pageClient;
let chromeProcess;
let preview;
let maxFallbackRebuildCountSeen = 0;
const fallbackCheckpoints = [];
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

async function clickPoint(
  point,
  { modifiers = 0, clickCount = 1, afterPressExpression = null } = {},
) {
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: point.center_x,
    y: point.center_y,
    button: "left",
    buttons: 1,
    clickCount,
    modifiers,
  });
  if (afterPressExpression) await waitFor(afterPressExpression);
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

async function dragFrom(point, dx, dy, moveCount = 12, readyExpression = null) {
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: point.center_x,
    y: point.center_y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  if (readyExpression) await waitFor(readyExpression);
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

function percentile(values, fraction) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.ceil((sorted.length - 1) * fraction)];
}

function rgbDistance(left, right) {
  return Math.sqrt(
    Math.pow(left[0] - right[0], 2) +
    Math.pow(left[1] - right[1], 2) +
    Math.pow(left[2] - right[2], 2)
  );
}

function transformPoint(matrix, point) {
  return [
    matrix[0] * point[0] + matrix[1] * point[1] + matrix[4],
    matrix[2] * point[0] + matrix[3] * point[1] + matrix[5],
  ];
}

function viewportPoint(proof, world) {
  return [
    (world[0] - proof.camera.center[0]) * proof.camera.zoom + proof.camera.viewport[0] / 2,
    (world[1] - proof.camera.center[1]) * proof.camera.zoom + proof.camera.viewport[1] / 2,
  ];
}

function srgbToLinear(value) {
  const encoded = Math.max(0, Math.min(1, value));
  return encoded <= 0.04045
    ? encoded / 12.92
    : Math.pow((encoded + 0.055) / 1.055, 2.4);
}

function pixelToLinear(pixel) {
  return pixel.rgba.slice(0, 3).map((channel) => srgbToLinear(channel / 255));
}

function maximumAbsolute(values) {
  return Math.max(...values.map((value) => Math.abs(value)));
}

function analyzeColorPair(label, blackPixel, whitePixel, sourceSrgb, tolerance = 0.055) {
  const blackLinear = pixelToLinear(blackPixel);
  const whiteLinear = pixelToLinear(whitePixel);
  const sourceLinear = sourceSrgb.slice(0, 3).map(srgbToLinear);
  const alphaChannels = whiteLinear.map((channel, index) => 1 - (channel - blackLinear[index]));
  const alpha = alphaChannels.reduce((sum, value) => sum + value, 0) / alphaChannels.length;
  const expectedBlack = sourceLinear.map((channel) => channel * alpha);
  const expectedWhite = sourceLinear.map((channel) => channel * alpha + 1 - alpha);
  const blackErrors = blackLinear.map((channel, index) => channel - expectedBlack[index]);
  const whiteErrors = whiteLinear.map((channel, index) => channel - expectedWhite[index]);
  const alphaSpread = Math.max(...alphaChannels) - Math.min(...alphaChannels);
  const maxChannelError = Math.max(maximumAbsolute(blackErrors), maximumAbsolute(whiteErrors));
  const rangeValid = [...blackLinear, ...whiteLinear, ...alphaChannels].every(
    (value) => value >= -tolerance && value <= 1 + tolerance,
  );
  return {
    label,
    source_srgb: sourceSrgb,
    source_linear: sourceLinear,
    black: { pixel: blackPixel, linear: blackLinear, expected_linear: expectedBlack, error: blackErrors },
    white: { pixel: whitePixel, linear: whiteLinear, expected_linear: expectedWhite, error: whiteErrors },
    reconstructed_alpha_channels: alphaChannels,
    reconstructed_alpha: alpha,
    alpha_channel_spread: alphaSpread,
    max_channel_error: maxChannelError,
    tolerance,
    black_halo_free: rangeValid && maximumAbsolute(blackErrors) <= tolerance,
    white_halo_free: rangeValid && maximumAbsolute(whiteErrors) <= tolerance,
    passed: rangeValid && alphaSpread <= tolerance && maxChannelError <= tolerance,
  };
}

async function setNodeAppearance(nodeId, { fill, stroke, strokeWidth, opacity }) {
  await send("command", {
    command: { kind: "set_fill", node_id: nodeId, color: fill },
  });
  await send("command", {
    command: { kind: "set_stroke", node_id: nodeId, color: stroke, width: strokeWidth },
  });
  await send("command", {
    command: { kind: "set_opacity", node_id: nodeId, opacity },
  });
}

async function readLocalSamples(proof, samples) {
  const node = proof.primary_node;
  const viewSamples = samples.map((sample) => ({
    label: sample.label,
    local: sample.local,
    viewport: viewportPoint(proof, transformPoint(node.world_transform, sample.local)),
  }));
  const pixels = await evaluate(
    "(async () => {" +
      "const samples = " + JSON.stringify(viewSamples) + ";" +
      "const result = [];" +
      "for (const sample of samples) {" +
        "result.push(await window.__phase0eReadPixel(sample.viewport[0], sample.viewport[1]));" +
      "}" +
      "return result;" +
    "})()",
  );
  return viewSamples.map((sample, index) => ({ ...sample, pixel: pixels[index] }));
}

async function captureColorContractCase(frameId, ellipseId, definition) {
  await send("selection", { target: ellipseId, mode: "replace" });
  await setNodeAppearance(ellipseId, definition.appearance);
  await send("command", {
    command: { kind: "set_fill", node_id: frameId, color: [0, 0, 0, 1] },
  });
  const blackProof = await getProof();
  const black = await readLocalSamples(blackProof, definition.samples);
  await send("command", {
    command: { kind: "set_fill", node_id: frameId, color: [1, 1, 1, 1] },
  });
  const whiteProof = await getProof();
  const white = await readLocalSamples(whiteProof, definition.samples);
  const analyses = definition.samples.map((sample, index) =>
    analyzeColorPair(
      sample.label,
      black[index].pixel,
      white[index].pixel,
      definition.sourceSrgb,
    ),
  );
  return {
    label: definition.label,
    appearance: definition.appearance,
    samples: definition.samples,
    black,
    white,
    analyses,
    passed: analyses.every((analysis) => analysis.passed),
  };
}


function centeredRotationMatrix(width, height, degrees, center) {
  const radians = degrees * (Math.PI / 180);
  const cosine = Math.cos(radians);
  const sine = Math.sin(radians);
  return [
    cosine,
    -sine,
    sine,
    cosine,
    center[0] - cosine * width / 2 + sine * height / 2,
    center[1] - sine * width / 2 - cosine * height / 2,
  ];
}

async function configureEllipseGeometry(nodeId, definition, center) {
  await send("command", {
    command: {
      kind: "set_geometry",
      node_id: nodeId,
      shape: "ellipse",
      width: definition.width,
      height: definition.height,
    },
  });
  await send("command", {
    command: {
      kind: "set_transform",
      node_id: nodeId,
      matrix: centeredRotationMatrix(
        definition.width,
        definition.height,
        definition.rotation,
        center,
      ),
    },
  });
}

async function measureEllipseAxis(proof, axis, dpr, zoom, requestedWidth) {
  const node = proof.primary_node;
  const matrix = node.world_transform;
  const width = node.geometry.width;
  const height = node.geometry.height;
  const localEdge = axis === "major" ? [width, height / 2] : [width / 2, 0];
  const worldDirection = axis === "major"
    ? [matrix[0], matrix[2]]
    : [-matrix[1], -matrix[3]];
  const axisScale = Math.hypot(worldDirection[0], worldDirection[1]);
  const normal = [worldDirection[0] / axisScale, worldDirection[1] / axisScale];
  const edge = viewportPoint(proof, transformPoint(matrix, localEdge));
  const physicalPixelsPerLocalUnit = dpr * zoom * axisScale;
  const requestedPhysical = requestedWidth * physicalPixelsPerLocalUnit;
  const outwardRadiusPhysical = Math.max(8, Math.ceil(requestedPhysical / 2 + 6));
  const diameterPhysical = (axis === "major" ? width : height) * physicalPixelsPerLocalUnit;
  const inwardRadiusPhysical = Math.max(
    3,
    Math.min(outwardRadiusPhysical, Math.floor(diameterPhysical / 2)),
  );
  const offsetsPhysical = Array.from(
    { length: inwardRadiusPhysical + outwardRadiusPhysical + 1 },
    (_, index) => index - inwardRadiusPhysical,
  );
  const pixels = await evaluate(
    "(async () => {" +
      "const edge = " + JSON.stringify(edge) + ";" +
      "const normal = " + JSON.stringify(normal) + ";" +
      "const dpr = " + JSON.stringify(dpr) + ";" +
      "const offsets = " + JSON.stringify(offsetsPhysical) + ";" +
      "const result = [];" +
      "for (const offset of offsets) {" +
        "result.push(await window.__phase0eReadPixel(" +
          "edge[0] + normal[0] * offset / dpr," +
          "edge[1] + normal[1] * offset / dpr" +
        "));" +
      "}" +
      "return result;" +
    "})()",
  );
  const profile = pixels.map((pixel, index) => ({
    offset_physical: offsetsPhysical[index],
    rgba: pixel.rgba,
    linear_luminance: Math.max(...pixelToLinear(pixel)),
  }));
  const active = profile
    .map((sample, index) => ({ sample, index }))
    .filter(({ sample }) => sample.linear_luminance > 0.08);
  const first = active.at(0)?.index ?? -1;
  const last = active.at(-1)?.index ?? -1;
  const centerIndex = offsetsPhysical.indexOf(0);
  let connectedStart = centerIndex;
  let connectedEnd = centerIndex;
  while (
    connectedStart > 0 &&
    profile[connectedStart - 1].linear_luminance > 0.001
  ) connectedStart -= 1;
  while (
    connectedEnd < profile.length - 1 &&
    profile[connectedEnd + 1].linear_luminance > 0.001
  ) connectedEnd += 1;
  const measuredPhysical = profile.slice(connectedStart, connectedEnd + 1).reduce(
    (sum, sample) => sum + sample.linear_luminance,
    0,
  );
  const measuredLocal = measuredPhysical / physicalPixelsPerLocalUnit;
  const tolerancePhysical = 1;
  const toleranceLocal = tolerancePhysical / physicalPixelsPerLocalUnit;
  const predictedOuter = [
    edge[0] + normal[0] * requestedWidth * zoom * axisScale / 2,
    edge[1] + normal[1] * requestedWidth * zoom * axisScale / 2,
  ];
  const clippingFree =
    first >= 0 &&
    last < profile.length - 1 &&
    profile.at(-1).linear_luminance < 0.03 &&
    predictedOuter[0] >= 1 &&
    predictedOuter[0] < proof.camera.viewport[0] - 1 &&
    predictedOuter[1] >= 1 &&
    predictedOuter[1] < proof.camera.viewport[1] - 1;
  return {
    axis,
    local_edge: localEdge,
    viewport_edge: edge,
    viewport_normal: normal,
    physical_pixels_per_local_unit: physicalPixelsPerLocalUnit,
    requested_local_width: requestedWidth,
    requested_physical_width: requestedPhysical,
    measured_local_width: measuredLocal,
    measured_physical_width: measuredPhysical,
    measurement_method: "integrated connected linear coverage over an isolated black backdrop",
    tolerance_physical: tolerancePhysical,
    tolerance_local: toleranceLocal,
    absolute_error_physical: Math.abs(measuredPhysical - requestedPhysical),
    absolute_error_local: Math.abs(measuredLocal - requestedWidth),
    clipping_free: clippingFree,
    uniform_within_tolerance:
      clippingFree && Math.abs(measuredLocal - requestedWidth) <= toleranceLocal,
    profile,
  };
}

async function measureEllipseStroke(definition, dpr, zoom) {
  await setSelectionZoom(dpr, zoom);
  const proof = await getProof();
  const requestedWidth = proof.primary_node.appearance.stroke.width;
  const major = await measureEllipseAxis(proof, "major", dpr, zoom, requestedWidth);
  const minor = await measureEllipseAxis(proof, "minor", dpr, zoom, requestedWidth);
  return {
    shape: definition.label,
    size: [definition.width, definition.height],
    rotation_degrees: definition.rotation,
    dpr,
    zoom,
    requested_local_width: requestedWidth,
    major,
    minor,
    major_minor_delta_local: Math.abs(major.measured_local_width - minor.measured_local_width),
    passed: major.uniform_within_tolerance && minor.uniform_within_tolerance,
  };
}

function inverseAffinePoint(matrix, point) {
  const determinant = matrix[0] * matrix[3] - matrix[1] * matrix[2];
  const x = point[0] - matrix[4];
  const y = point[1] - matrix[5];
  return [
    (matrix[3] * x - matrix[1] * y) / determinant,
    (-matrix[2] * x + matrix[0] * y) / determinant,
  ];
}

function outsideRectangleDistance(point, width, height) {
  return Math.max(-point[0], point[0] - width, -point[1], point[1] - height);
}

function discriminatingAffinePoints(matrix, width, height) {
  const transposed = [matrix[0], matrix[2], matrix[1], matrix[3], matrix[4], matrix[5]];
  let actualOnly = null;
  let transposedOnly = null;
  for (let xIndex = 1; xIndex < 20; xIndex += 1) {
    for (let yIndex = 1; yIndex < 20; yIndex += 1) {
      const local = [width * xIndex / 20, height * yIndex / 20];
      const actualWorld = transformPoint(matrix, local);
      const actualScore = outsideRectangleDistance(
        inverseAffinePoint(transposed, actualWorld),
        width,
        height,
      );
      if (actualScore > 2 && (!actualOnly || actualScore < actualOnly.score)) {
        actualOnly = { local, world: actualWorld, score: actualScore };
      }
      const transposedWorld = transformPoint(transposed, local);
      const transposedScore = outsideRectangleDistance(
        inverseAffinePoint(matrix, transposedWorld),
        width,
        height,
      );
      if (transposedScore > 2 && (!transposedOnly || transposedScore < transposedOnly.score)) {
        transposedOnly = { local, world: transposedWorld, score: transposedScore };
      }
    }
  }
  if (!actualOnly || !transposedOnly) {
    throw new HarnessFailure(
      "affine_discriminating_point_unavailable",
      "Rotation did not produce points that distinguish row-major and transposed semantics",
      { matrix, width, height, actualOnly, transposedOnly },
    );
  }
  return { transposed, actualOnly, transposedOnly };
}

async function proveAffineRenderHitOverlayParity(nodeId) {
  await send("selection", { target: nodeId, mode: "replace" });
  const proof = await getProof();
  const node = proof.primary_node;
  const matrix = node.world_transform;
  const width = node.geometry.width;
  const height = node.geometry.height;
  const points = discriminatingAffinePoints(matrix, width, height);
  const corners = [[0, 0], [width, 0], [width, height], [0, height]];
  const expectedOverlay = corners.map((corner) =>
    viewportPoint(proof, transformPoint(matrix, corner)),
  );
  const actualOverlay = await evaluate(`(() => {
    const polygon = document.querySelector(".selection-outline");
    if (!polygon) return null;
    return Array.from({ length: polygon.points.numberOfItems }, (_, index) => {
      const point = polygon.points.getItem(index);
      return [point.x, point.y];
    });
  })()`);
  if (!actualOverlay) {
    throw new HarnessFailure("selection_overlay_missing", "Selection overlay was not rendered");
  }
  const overlayMaxError = Math.max(...actualOverlay.flatMap((point, index) => [
    Math.abs(point[0] - expectedOverlay[index][0]),
    Math.abs(point[1] - expectedOverlay[index][1]),
  ]));
  const actualViewport = viewportPoint(proof, points.actualOnly.world);
  const transposedViewport = viewportPoint(proof, points.transposedOnly.world);
  const viewportSamples = [actualViewport, transposedViewport];
  const samplesInsideViewport = viewportSamples.every(
    ([x, y]) =>
      x >= 1 &&
      x < proof.camera.viewport[0] - 1 &&
      y >= 1 &&
      y < proof.camera.viewport[1] - 1,
  );
  if (!samplesInsideViewport) {
    throw new HarnessFailure(
      "affine_sample_outside_viewport",
      "Affine semantic parity samples must remain inside the render target",
      { viewportSamples, camera: proof.camera, points },
    );
  }
  const actualHit = await send("hit_test", { x: actualViewport[0], y: actualViewport[1] });
  const transposedHit = await send("hit_test", {
    x: transposedViewport[0],
    y: transposedViewport[1],
  });
  const actualPixel = await evaluate(
    `window.__phase0eReadPixel(${actualViewport[0]}, ${actualViewport[1]})`,
  );
  const transposedPixel = await evaluate(
    `window.__phase0eReadPixel(${transposedViewport[0]}, ${transposedViewport[1]})`,
  );
  const canvas = await boundsForSelector(".webgpu-canvas");
  await send("selection", { target: null, mode: "clear" });
  await clickPoint({
    center_x: canvas.x + actualViewport[0],
    center_y: canvas.y + actualViewport[1],
  }, {
    afterPressExpression: `window.__PHASE0E_PROOF__?.fsm === "Moving"`,
  });
  await waitFor(
    `window.__PHASE0E_PROOF__?.primary_node?.id === ${JSON.stringify(nodeId)} && window.__PHASE0E_PROOF__?.fsm === "Idle"`,
  );
  const actualClick = await getProof();
  await send("selection", { target: null, mode: "clear" });
  await clickPoint({
    center_x: canvas.x + transposedViewport[0],
    center_y: canvas.y + transposedViewport[1],
  }, {
    afterPressExpression: `window.__PHASE0E_PROOF__?.fsm === "Moving"`,
  });
  await waitFor(`window.__PHASE0E_PROOF__?.fsm === "Idle"`);
  const transposedClick = await getProof();
  return {
    matrix_order: "[m11,m12,m21,m22,tx,ty]",
    matrix,
    transposed_matrix: points.transposed,
    m12_not_equal_m21: matrix[1] !== matrix[2],
    actual_only: {
      ...points.actualOnly,
      viewport: actualViewport,
      hit_topmost: actualHit.result?.topmost ?? null,
      pixel: actualPixel,
      click_selected: actualClick.primary_node?.id ?? null,
    },
    transposed_only: {
      ...points.transposedOnly,
      viewport: transposedViewport,
      hit_topmost: transposedHit.result?.topmost ?? null,
      pixel: transposedPixel,
      click_selected: transposedClick.primary_node?.id ?? null,
    },
    expected_overlay_points: expectedOverlay,
    actual_overlay_points: actualOverlay,
    overlay_max_error_css_px: overlayMaxError,
    rust_semantic_hit_matches: actualHit.result?.topmost === nodeId,
    transposed_semantic_rejected: transposedHit.result?.topmost !== nodeId,
    render_pixels_distinguish_semantics:
      rgbDistance(actualPixel.rgba.slice(0, 3), transposedPixel.rgba.slice(0, 3)) > 40,
    overlay_matches_rust_semantics: overlayMaxError <= 0.75,
    actual_click_selects_shape: actualClick.primary_node?.id === nodeId,
    transposed_click_does_not_select_shape: transposedClick.primary_node?.id !== nodeId,
  };
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

async function setColor(selector, value) {
  const before = (await getProof()).engine_sequence;
  const expression = "(() => {" +
    "const input = document.querySelector(" + JSON.stringify(selector) + ");" +
    "if (!input) throw new Error('missing color input');" +
    "const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;" +
    "setter.call(input, " + JSON.stringify(value) + ");" +
    "input.dispatchEvent(new Event('input', { bubbles: true }));" +
    "input.dispatchEvent(new Event('change', { bubbles: true }));" +
    "})()";
  await evaluate(expression);
  await waitForSequenceAfter(before);
}

async function replaceNumeric(selector, value) {
  const before = (await getProof()).engine_sequence;
  await replaceInput(selector, value);
  await waitForSequenceAfter(before);
}
async function send(type, payload = {}) {
  return evaluate(
    "window.__phase0eSend(" + JSON.stringify(type) + "," + JSON.stringify(payload) + ")"
  );
}

async function selectedEdgeSample(label) {
  const proof = await getProof();
  const node = proof.primary_node;
  const matrix = node.world_transform;
  const width = node.geometry.width;
  const height = node.geometry.height;
  const camera = proof.camera;
  const worldPoint = (x, y) => [
    matrix[0] * x + matrix[1] * y + matrix[4],
    matrix[2] * x + matrix[3] * y + matrix[5],
  ];
  const toViewport = (point) => [
    (point[0] - camera.center[0]) * camera.zoom + camera.viewport[0] / 2,
    (point[1] - camera.center[1]) * camera.zoom + camera.viewport[1] / 2,
  ];
  const center = toViewport(worldPoint(width / 2, height / 2));
  const edge = toViewport(worldPoint(width, height / 2));
  const axisLength = Math.hypot(matrix[0], matrix[2]);
  const normal = axisLength > 0 ? [matrix[0] / axisLength, matrix[2] / axisLength] : [1, 0];
  const strokeHalf = (node.appearance.stroke.width * camera.zoom * axisLength) / 2;
  const radius = Math.max(5, strokeHalf + 3);
  const samples = await evaluate("(async () => {" +
    "const edge = " + JSON.stringify(edge) + ";" +
    "const center = " + JSON.stringify(center) + ";" +
    "const normal = " + JSON.stringify(normal) + ";" +
    "const radius = " + radius + ";" +
    "const values = [];" +
    "for (let index = 0; index <= 48; index += 1) {" +
      "const offset = -radius + (2 * radius * index) / 48;" +
      "values.push({ offset, pixel: await window.__phase0eReadPixel(edge[0] + normal[0] * offset, edge[1] + normal[1] * offset) });" +
    "}" +
    "const centerPixel = await window.__phase0eReadPixel(center[0], center[1]);" +
    "return { edge, center: centerPixel, values };" +
  "})()");
  const rgbs = samples.values.map((entry) => entry.pixel.rgba.slice(0, 3));
  const unique = new Set(rgbs.map((rgb) => rgb.join(",")));
  const first = rgbs[0];
  const last = rgbs[rgbs.length - 1];
  const partial = rgbs.some(
    (rgb) => rgbDistance(rgb, first) > 4 && rgbDistance(rgb, last) > 4
  );
  return {
    label,
    sampling: { local_edge: [width, height / 2], viewport_edge: edge, normal, radius },
    ...samples,
    unique_rgb_count: unique.size,
    endpoint_distance: rgbDistance(first, last),
    partial_coverage_present: partial && unique.size >= 3,
  };
}
async function setSelectionZoom(dpr, zoom) {
  const canvas = await boundsForSelector(".webgpu-canvas");
  await send("camera", {
    camera: {
      kind: "resize",
      width: canvas.width,
      height: canvas.height,
      dpr,
    },
  });
  await send("camera", {
    camera: {
      kind: "zoom",
      x: canvas.width / 2,
      y: canvas.height / 2,
      zoom,
    },
  });
  const proof = await getProof();
  const node = proof.primary_node;
  const matrix = node.world_transform;
  const worldEdge = [
    matrix[0] * node.geometry.width + matrix[1] * (node.geometry.height / 2) + matrix[4],
    matrix[2] * node.geometry.width + matrix[3] * (node.geometry.height / 2) + matrix[5],
  ];
  const axisScale = Math.hypot(matrix[0], matrix[2]);
  const viewportEdgeX = (worldEdge[0] - proof.camera.center[0]) * proof.camera.zoom + proof.camera.viewport[0] / 2;
  const outerPhysicalX = (viewportEdgeX + node.appearance.stroke.width * proof.camera.zoom * axisScale / 2) * dpr;
  const fraction = outerPhysicalX - Math.floor(outerPhysicalX);
  await send("camera", {
    camera: { kind: "pan", dx: (0.25 - fraction) / dpr, dy: 0 },
  });
}

async function fitSelection() {
  const before = (await getProof()).engine_sequence;
  await clickText("button", "Fit selection", undefined, true);
  await waitForSequenceAfter(before);
}


try {
  await stat(chrome).catch(() => {
    throw new HarnessFailure("chrome_not_found", "Chrome executable was not found: " + chrome);
  });
  preview = await preparePreview();
  chromeProcess = spawn(
    chrome,
    [
      "--headless=new",
      "--remote-debugging-port=0",
      "--user-data-dir=" + profile,
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
    chromeStderr = (chromeStderr + chunk).slice(-20000);
  });

  const activePortFile = path.join(profile, "DevToolsActivePort");
  const debugPort = (await waitForFile(activePortFile)).trim().split(/\r?\n/)[0];
  const debugBase = "http://127.0.0.1:" + debugPort;
  const version = await (await fetch(debugBase + "/json/version")).json();
  browserClient = new CdpClient(version.webSocketDebuggerUrl);
  await browserClient.connect();
  const gpuInfo = await browserClient.send("SystemInfo.getInfo");

  const target = await (
    await fetch(debugBase + "/json/new?" + encodeURIComponent(preview.url), {
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
        details.args.map((argument) => argument.value ?? argument.description).join(" "),
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
    width: 1440,
    height: 1000,
    deviceScaleFactor: 1,
    mobile: false,
  });

  await waitForReady();
  const initial = await getProof();
  await waitFor(
    "window.__PHASE0E_PROOF__?.fixture === 'EDITOR' && window.__PHASE0E_PROOF__?.projection_nodes === 2",
  );
  await capture(screenshots.default);

  await clickText(".tree-row", "Frame 1920×1080");
  await waitFor(
    "window.__PHASE0E_PROOF__?.primary_node?.name === 'Frame 1920×1080' && document.querySelector('.selection-outline')",
  );
  const defaultFrame = await getProof();
  const defaultFramePixel = await readPixelAtOverlayCenter();

  await pressKey("f", "KeyF", 70);
  await waitFor("Boolean(document.querySelector('[aria-label=\"Frame presets\"]'))");
  await capture(screenshots.selection);
  await clickText("button", "4K UHD");
  await waitFor(
    "window.__PHASE0E_PROOF__?.primary_node?.name === 'Frame 3840×2160'",
  );
  const frame4kCreated = await getProof();
  const frame4kId = frame4kCreated.primary_node.id;
  await clickSelector("[data-testid='undo']");
  await waitFor(
    "![...document.querySelectorAll('.tree-row')].some((row) => row.textContent?.includes('Frame 3840×2160'))",
  );
  const afterFrameUndo = await getProof();
  await clickSelector("[data-testid='redo']");
  await waitFor(
    "[...document.querySelectorAll('.tree-row')].some((row) => row.textContent?.includes('Frame 3840×2160'))",
  );
  await clickText(".tree-row", "Frame 3840×2160");
  await waitFor("window.__PHASE0E_PROOF__?.primary_node?.name === 'Frame 3840×2160'");
  await fitSelection();
  const afterFrameRedo = await getProof();

  await setColor("input[aria-label='Fill color']", "#f97316");
  await replaceNumeric("input[aria-label='Radius']", 32);
  await replaceNumeric("input[aria-label='Stroke width']", 12);
  await setColor("input[aria-label='Stroke color']", "#111827");
  await replaceNumeric("input[aria-label='Opacity']", 82);
  const styledFrame = await getProof();

  await clickSelector("[data-testid='tool-ellipse']");
  const shell = await boundsForSelector("[data-testid='canvas-shell']");
  await dragFrom(
    {
      center_x: shell.center_x - 80,
      center_y: shell.center_y - 45,
    },
    160,
    90,
  );
  await waitFor(
    "window.__PHASE0E_PROOF__?.primary_node?.kind === 'ellipse'",
  );
  await replaceNumeric("input[aria-label='W']", 80);
  await replaceNumeric("input[aria-label='H']", 48);
  await setColor("input[aria-label='Fill color']", "#2563eb");
  await replaceNumeric("input[aria-label='Stroke width']", 4);
  await setColor("input[aria-label='Stroke color']", "#f8fafc");
  await replaceNumeric("input[aria-label='Opacity']", 70);
  await fitSelection();
  const ellipseId = (await getProof()).primary_node.id;

  const aaMatrix = [];
  for (const dpr of [1, 1.25, 1.5, 2]) {
    for (const zoom of [0.25, 1, 4]) {
      await setSelectionZoom(dpr, zoom);
      aaMatrix.push(await selectedEdgeSample("ellipse-dpr-" + dpr + "-zoom-" + zoom));
    }
  }
  await capture(screenshots.component_showcase);

  await replaceNumeric("input[aria-label='Rotation']", 33);
  await setSelectionZoom(1, 1);
  const rotatedEllipse = await selectedEdgeSample("rotated-nonuniform-ellipse");

  await replaceNumeric("input[aria-label='W']", 64);
  await replaceNumeric("input[aria-label='H']", 64);
  await replaceNumeric("input[aria-label='Rotation']", 0);
  const circle = await selectedEdgeSample("circle");

  const ellipseCenterProof = await getProof();
  const ellipseCenter = transformPoint(
    ellipseCenterProof.primary_node.world_transform,
    [
      ellipseCenterProof.primary_node.geometry.width / 2,
      ellipseCenterProof.primary_node.geometry.height / 2,
    ],
  );
  const ellipseDefinitions = [
    { label: "circle-64x64", width: 64, height: 64, rotation: 0 },
    { label: "ellipse-80x48", width: 80, height: 48, rotation: 0 },
    { label: "ellipse-160x32", width: 160, height: 32, rotation: 0 },
    { label: "ellipse-160x32-rotated-33", width: 160, height: 32, rotation: 33 },
  ];
  await setNodeAppearance(ellipseId, {
    fill: [1, 1, 1, 0],
    stroke: [1, 1, 1, 1],
    strokeWidth: 8,
    opacity: 1,
  });
  await send("command", {
    command: { kind: "set_visible", node_id: defaultFrame.primary_node.id, visible: false },
  });
  await setNodeAppearance(frame4kId, {
    fill: [0, 0, 0, 1],
    stroke: [0, 0, 0, 0],
    strokeWidth: 0,
    opacity: 1,
  });
  const ellipseStrokeMeasurements = [];
  for (const definition of ellipseDefinitions) {
    await configureEllipseGeometry(ellipseId, definition, ellipseCenter);
    await send("selection", { target: ellipseId, mode: "replace" });
    for (const dpr of [1, 1.25, 1.5, 2]) {
      for (const zoom of [0.25, 1, 4]) {
        ellipseStrokeMeasurements.push(
          await measureEllipseStroke(definition, dpr, zoom),
        );
      }
    }
  }

  const colorEllipse = ellipseDefinitions[2];
  await configureEllipseGeometry(ellipseId, colorEllipse, ellipseCenter);
  await send("selection", { target: ellipseId, mode: "replace" });
  await setSelectionZoom(1, 4);
  const colorSource = [0.18, 0.62, 0.92, 1];
  const colorContractCases = [];
  colorContractCases.push(await captureColorContractCase(frame4kId, ellipseId, {
    label: "fill-only-opacity-70",
    sourceSrgb: colorSource,
    appearance: {
      fill: [colorSource[0], colorSource[1], colorSource[2], 0.55],
      stroke: [colorSource[0], colorSource[1], colorSource[2], 0],
      strokeWidth: 0,
      opacity: 0.7,
    },
    samples: [
      { label: "fill-internal", local: [80, 16] },
      { label: "fill-outer-aa-edge", local: [160, 16] },
    ],
  }));
  colorContractCases.push(await captureColorContractCase(frame4kId, ellipseId, {
    label: "stroke-only-opacity-70",
    sourceSrgb: colorSource,
    appearance: {
      fill: [colorSource[0], colorSource[1], colorSource[2], 0],
      stroke: [colorSource[0], colorSource[1], colorSource[2], 0.65],
      strokeWidth: 8,
      opacity: 0.7,
    },
    samples: [
      { label: "stroke-center", local: [160, 16] },
      { label: "stroke-outer-aa-edge", local: [164, 16] },
    ],
  }));
  colorContractCases.push(await captureColorContractCase(frame4kId, ellipseId, {
    label: "same-color-fill-stroke-opacity-70",
    sourceSrgb: colorSource,
    appearance: {
      fill: colorSource,
      stroke: colorSource,
      strokeWidth: 8,
      opacity: 0.7,
    },
    samples: [
      { label: "combined-fill-internal", local: [80, 16] },
      { label: "combined-fill-stroke-boundary", local: [156, 16] },
      { label: "combined-stroke-center", local: [160, 16] },
      { label: "combined-outer-aa-edge", local: [164, 16] },
    ],
  }));
  const combinedColorCase = colorContractCases.at(-1);
  const combinedAnalysisByLabel = new Map(
    combinedColorCase.analyses.map((analysis) => [analysis.label, analysis]),
  );
  const combinedFillAlpha = combinedAnalysisByLabel.get("combined-fill-internal").reconstructed_alpha;
  const combinedBoundaryAlpha = combinedAnalysisByLabel.get("combined-fill-stroke-boundary").reconstructed_alpha;
  const combinedStrokeAlpha = combinedAnalysisByLabel.get("combined-stroke-center").reconstructed_alpha;
  const colorContractMaxChannelError = Math.max(
    ...colorContractCases.flatMap((entry) =>
      entry.analyses.map((analysis) => analysis.max_channel_error),
    ),
  );
  await capture(screenshots.color_contract);

  await send("command", {
    command: {
      kind: "set_fill",
      node_id: frame4kId,
      color: [0.01, 0.015, 0.025, 1],
    },
  });
  await send("selection", { target: ellipseId, mode: "replace" });
  const blackBackground = await selectedEdgeSample("opacity-fill-stroke-black-background");

  await send("command", {
    command: {
      kind: "set_fill",
      node_id: frame4kId,
      color: [1, 1, 1, 1],
    },
  });
  await send("selection", { target: ellipseId, mode: "replace" });
  const whiteBackground = await selectedEdgeSample("opacity-fill-stroke-white-background");

  await clickSelector("[data-testid='tool-rectangle']");
  const rectangleShell = await boundsForSelector("[data-testid='canvas-shell']");
  await dragFrom(
    {
      center_x: rectangleShell.center_x - 70,
      center_y: rectangleShell.center_y - 45,
    },
    140,
    90,
  );
  await waitFor("window.__PHASE0E_PROOF__?.primary_node?.kind === 'rectangle'");
  await replaceNumeric("input[aria-label='Radius']", 24);
  await replaceNumeric("input[aria-label='Stroke width']", 6);
  await setColor("input[aria-label='Fill color']", "#ec4899");
  await setColor("input[aria-label='Stroke color']", "#fef3c7");
  await fitSelection();
  const roundedRectangle = await selectedEdgeSample("rounded-rectangle");

  const affineNodeId = (await getProof()).primary_node.id;
  const affineBefore = await getProof();
  const affineCenter = transformPoint(
    affineBefore.primary_node.world_transform,
    [affineBefore.primary_node.geometry.width / 2, affineBefore.primary_node.geometry.height / 2],
  );
  await send("command", {
    command: {
      kind: "set_geometry",
      node_id: affineNodeId,
      shape: "rectangle",
      width: 160,
      height: 60,
    },
  });
  await send("command", {
    command: { kind: "set_corner_radii", node_id: affineNodeId, radii: [0, 0, 0, 0] },
  });
  await setNodeAppearance(affineNodeId, {
    fill: [0.92, 0.12, 0.68, 1],
    stroke: [0.92, 0.12, 0.68, 0],
    strokeWidth: 0,
    opacity: 1,
  });
  await send("command", {
    command: { kind: "set_fill", node_id: frame4kId, color: [0, 0, 0, 1] },
  });
  await send("command", {
    command: {
      kind: "set_transform",
      node_id: affineNodeId,
      matrix: centeredRotationMatrix(160, 60, 33, affineCenter),
    },
  });
  await send("selection", { target: affineNodeId, mode: "replace" });
  await fitSelection();
  const affineParity = await proveAffineRenderHitOverlayParity(affineNodeId);
  await send("selection", { target: affineNodeId, mode: "replace" });
  await capture(screenshots.affine_parity);

  const beforeMove = await getProof();
  const selection = await boundsForSelector(".selection-outline");
  await dragFrom(selection, 24, 18, 12, "window.__PHASE0E_PROOF__?.fsm === 'Moving'");
  await waitForSequenceAfter(beforeMove.engine_sequence);
  await waitFor("window.__PHASE0E_PROOF__?.fsm === 'Idle' && window.__PHASE0E_PROOF__?.history?.transaction_active === false");
  const afterMove = await getProof();
  const resizeHandle = await boundsForSelector("[data-testid='resize-handle']");
  await dragFrom(resizeHandle, 22, 14, 12, "window.__PHASE0E_PROOF__?.fsm === 'Resizing'");
  await waitForSequenceAfter(afterMove.engine_sequence);
  await waitFor("window.__PHASE0E_PROOF__?.fsm === 'Idle'");
  const afterResizeInteraction = await getProof();
  await replaceNumeric("input[aria-label='W']", afterResizeInteraction.primary_node.geometry.width + 1);
  const afterResize = await getProof();
  const beforeUndoSequence = afterResize.engine_sequence;
  await clickSelector("[data-testid='undo']");
  await waitForSequenceAfter(beforeUndoSequence);
  const afterResizeUndo = await getProof();
  const beforeRedoSequence = afterResizeUndo.engine_sequence;
  await clickSelector("[data-testid='redo']");
  await waitForSequenceAfter(beforeRedoSequence);
  const afterResizeRedo = await getProof();
  await capture(screenshots.nested_group);

  delete screenshots.hundred_k_debug;
  const finalProof = await getProof();

  const allPixelSamples = [
    ...aaMatrix,
    rotatedEllipse,
    circle,
    blackBackground,
    whiteBackground,
    roundedRectangle,
  ];
  const srgbRenderViewActive =
    ["bgra8unorm", "rgba8unorm"].includes(initial.surface_base_format) &&
    initial.pipeline_view_format === `${initial.surface_base_format}-srgb` &&
    initial.readback_view_format === initial.pipeline_view_format;
  const affineSemanticParity =
    affineParity.m12_not_equal_m21 &&
    affineParity.rust_semantic_hit_matches &&
    affineParity.transposed_semantic_rejected;
  const renderHitTestOverlayParity =
    affineSemanticParity &&
    affineParity.render_pixels_distinguish_semantics &&
    affineParity.overlay_matches_rust_semantics &&
    affineParity.actual_click_selects_shape &&
    affineParity.transposed_click_does_not_select_shape;
  const expectedColorContractMatch = colorContractCases.every((entry) => entry.passed);
  const haloFreeBlackBackground = colorContractCases.every((entry) =>
    entry.analyses.every((analysis) => analysis.black_halo_free),
  );
  const haloFreeWhiteBackground = colorContractCases.every((entry) =>
    entry.analyses.every((analysis) => analysis.white_halo_free),
  );
  const boundaryAlphaTolerance = 0.045;
  const fillStrokeBoundaryHasNoAlphaDip =
    combinedBoundaryAlpha >= Math.min(combinedFillAlpha, combinedStrokeAlpha) - boundaryAlphaTolerance &&
    combinedBoundaryAlpha <= Math.max(combinedFillAlpha, combinedStrokeAlpha) + boundaryAlphaTolerance;
  const ellipseStrokeWidthUniform =
    ellipseStrokeMeasurements.length === 48 &&
    ellipseStrokeMeasurements.every((measurement) => measurement.passed);
  const ellipseStrokeMaxErrorLocal = Math.max(
    ...ellipseStrokeMeasurements.flatMap((measurement) => [
      measurement.major.absolute_error_local,
      measurement.minor.absolute_error_local,
    ]),
  );
  const ellipseStrokeMaxErrorPhysical = Math.max(
    ...ellipseStrokeMeasurements.flatMap((measurement) => [
      measurement.major.absolute_error_physical,
      measurement.minor.absolute_error_physical,
    ]),
  );
  const ellipseStrokeMaxMajorMinorDeltaLocal = Math.max(
    ...ellipseStrokeMeasurements.map((measurement) => measurement.major_minor_delta_local),
  );
  const pixelAssertions = {
    default_frame_is_visible:
      Array.isArray(defaultFramePixel?.rgba) &&
      rgbDistance(defaultFramePixel.rgba.slice(0, 3), [9, 14, 20]) > 30,
    affine_semantic_parity: affineSemanticParity,
    srgb_render_view_active: srgbRenderViewActive,
    expected_color_contract_match: expectedColorContractMatch,
    halo_free_black_background: haloFreeBlackBackground,
    halo_free_white_background: haloFreeWhiteBackground,
    fill_stroke_boundary_has_no_alpha_dip: fillStrokeBoundaryHasNoAlphaDip,
    ellipse_stroke_width_uniform: ellipseStrokeWidthUniform,
    render_hit_test_overlay_parity: renderHitTestOverlayParity,
    every_dpr_zoom_edge_has_partial_coverage:
      aaMatrix.every((sample) => sample.partial_coverage_present),
    circle_has_partial_coverage: circle.partial_coverage_present,
    nonuniform_rotated_ellipse_has_partial_coverage:
      rotatedEllipse.partial_coverage_present,
    rounded_rectangle_has_partial_coverage:
      roundedRectangle.partial_coverage_present,
    black_and_white_backgrounds_are_distinct:
      rgbDistance(
        blackBackground.values.at(-1).pixel.rgba.slice(0, 3),
        whiteBackground.values.at(-1).pixel.rgba.slice(0, 3),
      ) > 80,
    opacity_fill_and_stroke_render_together:
      blackBackground.unique_rgb_count >= 3 &&
      whiteBackground.unique_rgb_count >= 3,
    no_binary_edge_only_result:
      allPixelSamples.every((sample) => sample.unique_rgb_count >= 3),
  };
  const pixelReadback = {
    phase: "1A",
    proof_kind: "actual-hardware-webgpu-pixel-readback",
    captured_at_utc: new Date().toISOString(),
    color_contract: "sRGB UI to linear premultiplied WebGPU output",
    analytic_aa: "fwidth + smoothstep; no fragment discard",
    surface_base_format: initial.surface_base_format,
    pipeline_view_format: initial.pipeline_view_format,
    readback_view_format: initial.readback_view_format,
    raw_readback_channel_normalization: "BGRA/RGBA base bytes normalized explicitly to RGBA",
    color_channel_tolerance_linear: 0.055,
    fill_stroke_boundary_alpha_tolerance: boundaryAlphaTolerance,
    color_contract_cases: colorContractCases,
    color_contract_max_channel_error: colorContractMaxChannelError,
    ellipse_stroke_measurement_count: ellipseStrokeMeasurements.length,
    ellipse_stroke_measurement_method: "integrated connected linear coverage over an isolated black backdrop",
    ellipse_stroke_tolerance_physical: 1,
    ellipse_stroke_measurements: ellipseStrokeMeasurements,
    ellipse_stroke_max_error_local: ellipseStrokeMaxErrorLocal,
    ellipse_stroke_max_major_minor_delta_local: ellipseStrokeMaxMajorMinorDeltaLocal,
    ellipse_stroke_max_error_physical: ellipseStrokeMaxErrorPhysical,
    affine_parity: affineParity,
    assertion_count: Object.keys(pixelAssertions).length,
    passed_assertion_count: Object.values(pixelAssertions).filter(Boolean).length,
    expected_assertion_names: Object.keys(pixelAssertions),
    cases: allPixelSamples,
    assertions: pixelAssertions,
    all_passed: Object.values(pixelAssertions).every(Boolean),
  };
  await writeFile(pixelPath, JSON.stringify(pixelReadback, null, 2) + "\n", "utf8");

  const activeDevice =
    gpuInfo.gpu?.devices?.find((device) => device.active) ??
    gpuInfo.gpu?.devices?.[0] ??
    {};
  const checks = {
    server_health: preview.health.ok === true,
    asset_http_and_wasm_mime:
      Object.values(preview.assets).every((asset) => asset.status === 200),
    new_chrome_process: Boolean(chromeProcess.pid),
    temporary_chrome_profile: profile.includes("phase1a-chrome-"),
    navigator_gpu: initial.actual_webgpu === true,
    actual_adapter_and_device:
      Boolean(initial.adapter) && Boolean(activeDevice.deviceString || activeDevice.deviceId),
    affine_semantic_parity: pixelAssertions.affine_semantic_parity,
    srgb_render_view_active: pixelAssertions.srgb_render_view_active,
    expected_color_contract_match: pixelAssertions.expected_color_contract_match,
    halo_free_black_background: pixelAssertions.halo_free_black_background,
    halo_free_white_background: pixelAssertions.halo_free_white_background,
    fill_stroke_boundary_has_no_alpha_dip: pixelAssertions.fill_stroke_boundary_has_no_alpha_dip,
    ellipse_stroke_width_uniform: pixelAssertions.ellipse_stroke_width_uniform,
    render_hit_test_overlay_parity: pixelAssertions.render_hit_test_overlay_parity,
    dedicated_worker_wasm:
      initial.worker_runtime_owner === "dedicated-worker" &&
      initial.wasm_initialized === true &&
      initial.heartbeat >= 1,
    schema_v2:
      initial.render_binary_schema_version === 2 &&
      initial.resources.instance_stride_bytes === 112 &&
      initial.resources.dirty_record_stride_bytes === 116,
    default_1080_frame:
      defaultFrame.primary_node.name === "Frame 1920×1080" &&
      defaultFrame.primary_node.kind === "frame" &&
      defaultFrame.primary_node.geometry.width === 1920 &&
      defaultFrame.primary_node.geometry.height === 1080,
    frame_4k_created_selected_undo_redo:
      frame4kCreated.primary_node.name === "Frame 3840×2160" &&
      afterFrameUndo.history.redo_depth >= 1 &&
      afterFrameRedo.history.undo_depth >= 1,
    appearance_projection_synchronized:
      styledFrame.primary_node.appearance.corner_radii.every((radius) => radius === 32) &&
      styledFrame.primary_node.appearance.stroke.width === 12 &&
      Math.abs(styledFrame.primary_node.opacity - 0.82) < 0.001,
    direct_move_resize_undo_redo:
      afterMove.revisions.document > beforeMove.revisions.document &&
      afterMove.primary_node.local_transform.some((value, index) => value !== beforeMove.primary_node.local_transform[index]) &&
      afterResizeInteraction.revisions.document > afterMove.revisions.document &&
      afterResizeInteraction.primary_node.geometry.width !== afterMove.primary_node.geometry.width &&
      afterResize.revisions.document > afterResizeInteraction.revisions.document &&
      afterResizeUndo.history.redo_depth >= 1 &&
      afterResizeRedo.history.undo_depth >= 1,
    incremental_single_item_edits:
      [
        styledFrame.render_delta,
        afterResize.render_delta,
      ].every(
        (delta) =>
          delta.dirty_slots === 1 &&
          delta.render_full_rebuilds === 0 &&
          delta.scene_full_rebuilds === 0,
      ),
    actual_pixel_readback: pixelReadback.all_passed,
    console_errors_zero: consoleErrors.length === 0,
    gpu_validation_errors_zero:
      Number(finalProof.gpu_validation_errors ?? finalProof.gpu?.validation_errors ?? 0) === 0,
    fallback_rebuild_zero: maxFallbackRebuildCountSeen === 0,
    latest_sequences_match:
      finalProof.response_gpu_overlay_sequence_match === true,
  };

  const screenshotInfo = {};
  for (const [name, pathname] of Object.entries(screenshots)) {
    const info = await stat(pathname);
    screenshotInfo[name] = {
      path: path.relative(workspace, pathname).replaceAll("\\", "/"),
      bytes: info.size,
      modified_utc: info.mtime.toISOString(),
    };
  }

  const browserProofFinishedAtUtc = new Date().toISOString();
  const proof = {
    phase: "1A",
    proof_kind: "actual-hardware-browser",
    captured_at_utc: browserProofFinishedAtUtc,
    browser: version.Browser,
    browser_mode: "actual Chrome; no mock; no Canvas2D fallback",
    execution: {
      command: "npm run test:browser:phase1a",
      started_at_utc: browserProofStartedAtUtc,
      finished_at_utc: browserProofFinishedAtUtc,
    },
    user_agent: await evaluate("navigator.userAgent"),
    chrome_process_id: chromeProcess.pid,
    chrome_profile: profile,
    gpu: {
      adapter: initial.adapter,
      backend: initial.backend,
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
    interactions: {
      default_frame: defaultFrame,
      frame_4k_created: frame4kCreated,
      frame_undo: afterFrameUndo,
      frame_redo: afterFrameRedo,
      styled_frame: styledFrame,
      affine_render_hit_overlay_parity: affineParity,
      move: afterMove,
      resize_interaction: afterResizeInteraction,
      resize: afterResize,
      resize_undo: afterResizeUndo,
      resize_redo: afterResizeRedo,
    },
    pixel_readback_artifact: path.relative(workspace, pixelPath).replaceAll("\\", "/"),
    performance_artifact: "docs/PHASE_1A_METRICS.json",
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
  await writeFile(proofPath, JSON.stringify(proof, null, 2) + "\n", "utf8");

  console.log("Phase 1A actual browser proof: " + (proof.all_passed ? "PASS" : "FAIL"));
  console.log("browser=" + version.Browser);
  console.log("gpu=" + (activeDevice.deviceString ?? activeDevice.deviceId ?? "unknown"));
  console.log("proof=" + proofPath);
  console.log("screenshots=" + Object.keys(screenshots).length);
  if (!proof.all_passed) {
    console.log(JSON.stringify(checks, null, 2));
    process.exitCode = 1;
  }
} catch (error) {
  const browserDiagnostics = pageClient
    ? await evaluate("({" +
        "body_ready: document.body?.dataset.ready ?? null," +
        "state: window.__phase0eState ?? null," +
        "proof: window.__PHASE0E_PROOF__ ?? null," +
        "navigator_gpu: Boolean(navigator.gpu)" +
      "})").catch((diagnosticError) => ({
        diagnostic_error: diagnosticError.message,
      }))
    : null;
  const failure = {
    phase: "1A",
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
    JSON.stringify(failure, null, 2) + "\n",
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

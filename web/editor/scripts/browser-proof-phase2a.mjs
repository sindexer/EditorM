// Phase 2A actual-browser proof: persistent paths on the real WebGPU surface.
//
// Structure follows scripts/browser-proof-phase1b.mjs: this harness owns port allocation, the
// preview server, a new Chrome process with a throwaway profile, the CDP session, evidence
// capture and cleanup. Everything it asserts crosses React -> Dedicated Worker -> WASM
// EngineHost -> actual WebGPU. Paths are never drawn by React, SVG or Canvas2D: every path pixel
// asserted here is read back from the WebGPU surface through window.__phase0eReadPixel.
//
// What it proves:
//   * the PATH-A fixture reaches the GPU as tessellated triangles in ordered draw batches;
//   * a straight path, a Bezier path, a closed fill interior and a stroked closed curve all
//     produce the expected colours on the WebGPU surface, against background controls;
//   * hit testing picks paths by their real geometry, including a filled interior;
//   * creating and editing a path, then undo and redo, stay correct end to end;
//   * a save and load round trip preserves anchor identity, bounds and rendered pixels;
//   * a camera-only frame re-uploads no triangles and re-tessellates nothing.

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
  gate_run_id: process.env.PHASE2A_GATE_RUN_ID ?? null,
  tested_source_commit: process.env.PHASE2A_GATE_SOURCE_COMMIT ?? null,
  tested_branch: process.env.PHASE2A_GATE_SOURCE_BRANCH ?? null,
};
const allowSoftwareGpu = process.env.PHASE2A_ALLOW_SOFTWARE_GPU === "1";
// A software-GPU run can exercise the harness end to end, but it is never gate evidence, so it
// is written to separate diagnostic paths and never overwrites the hardware proof artifacts.
const proofPath = path.join(
  verificationRoot,
  allowSoftwareGpu ? "PHASE_2A_BROWSER_SOFTWARE_RUN.json" : "PHASE_2A_BROWSER_PROOF.json",
);
const pixelPath = path.join(
  verificationRoot,
  allowSoftwareGpu
    ? "PHASE_2A_PIXEL_READBACK_SOFTWARE_RUN.json"
    : "PHASE_2A_PIXEL_READBACK.json",
);
const failurePath = path.join(
  verificationRoot,
  allowSoftwareGpu
    ? "PHASE_2A_BROWSER_SOFTWARE_RUN_FAILURE.json"
    : "PHASE_2A_BROWSER_FAILURE.json",
);
const metricsPath = path.join(
  workspace,
  "docs",
  allowSoftwareGpu ? "PHASE_2A_BROWSER_METRICS_SOFTWARE_RUN.json" : "PHASE_2A_BROWSER_METRICS.json",
);
const screenshots = {
  paths: path.join(verificationRoot, "phase2a-paths.png"),
};
const chrome =
  process.env.PHASE0E_CHROME ??
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";
const profile = path.join(
  os.tmpdir(),
  `phase2a-chrome-${process.pid}-${Date.now()}`,
);



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
      const timeout = setTimeout(
        () => reject(new HarnessFailure("cdp_connect_timeout", `Timed out connecting to ${this.url}`)),
        30000,
      );
      this.socket.addEventListener("open", () => {
        clearTimeout(timeout);
        resolve();
      }, { once: true });
      this.socket.addEventListener("error", (error) => {
        clearTimeout(timeout);
        reject(error);
      }, { once: true });
    });
    this.socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      if (message.id) {
        const pending = this.pending.get(message.id);
        if (!pending) return;
        this.pending.delete(message.id);
        clearTimeout(pending.timeout);
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
    this.socket.addEventListener("close", () => {
      for (const [id, pending] of this.pending) {
        clearTimeout(pending.timeout);
        pending.reject(new HarnessFailure("cdp_socket_closed", `CDP socket closed with request ${id} pending`));
      }
      this.pending.clear();
    });
  }

  send(method, params = {}, timeoutMs = 30000) {
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        reject(new HarnessFailure("cdp_command_timeout", `Timed out waiting for CDP ${method}`, {
          method,
          timeout_ms: timeoutMs,
        }));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timeout });
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


/// Waits until the editor has no interaction in flight.
///
/// A pointer press on the canvas opens an interaction transaction, and the editor closes it
/// asynchronously after the pointer is released: `finishInteraction` drains the drag queue, awaits
/// `commit_transaction`, and only then returns the FSM to Idle. CDP resolving `mouseReleased` says
/// nothing about that tail, so any request sent immediately after a click can land while the
/// transaction is still open and be rejected — correctly — by the engine.
///
/// This is state-based, not time-based: it polls the live proof object and returns as soon as the
/// editor is quiescent, so an already-idle editor costs one poll.
async function waitForInteractionIdle(label, timeoutMs = 30000) {
  const deadline = Date.now() + timeoutMs;
  let state;
  while (Date.now() < deadline) {
    state = await evaluate(`(() => {
      const proof = window.__PHASE0E_PROOF__ ?? {};
      return {
        fsm: proof.fsm ?? null,
        transaction_active: proof.history?.transaction_active ?? null,
        interaction_active: proof.interaction_active ?? null,
        interaction_queue: proof.interaction_queue ?? null,
        engine_sequence: proof.engine_sequence ?? null,
        gpu_frame_sequence: proof.gpu_frame_sequence ?? null,
        selection: proof.selection ?? null,
      };
    })()`).catch((error) => ({ evaluation_error: error.message }));
    const idle =
      (state?.fsm === "Idle" || state?.fsm === "NestedEditing") &&
      state?.transaction_active === false &&
      state?.interaction_active === null &&
      state?.interaction_queue?.in_flight === false &&
      state?.interaction_queue?.scheduled === false;
    if (idle) return state;
    // Polling interval only. The condition above, never elapsed time, decides when to proceed.
    await sleep(25);
  }
  throw new HarnessFailure(
    "interaction_quiescence_timeout",
    `The editor did not return to an idle interaction state after ${label}`,
    { step: label, timeout_ms: timeoutMs, last_state: state },
  );
}

/// Presses and releases at a point, then waits for the interaction that press may have opened to
/// finish. Every call site can therefore treat a click as complete when it returns.
async function clickPoint(point, { modifiers = 0, clickCount = 1, label = "click" } = {}) {
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
  return waitForInteractionIdle(label);
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


function colorDistance(left, right) {
  return Math.sqrt(
    (left[0] - right[0]) ** 2 + (left[1] - right[1]) ** 2 + (left[2] - right[2]) ** 2,
  );
}



function recordPixelCase(entry) {
  pixelCases.push(entry);
  return entry;
}


/// Live projection of every node the engine currently reports, keyed by ID.
async function nodeTable() {
  const response = await send("get_ui_snapshot");
  const table = {};
  for (const node of response.projection.upserts) table[node.id] = node;
  return table;
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




async function fitDocument() {
  const before = (await getProof()).engine_sequence;
  await send("camera", { camera: { kind: "fit" } });
  await waitFor(
    `window.__PHASE0E_PROOF__?.engine_sequence > ${before}`,
    30000,
    "engine_response_timeout",
  );
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
      ...(process.env.PHASE2A_NO_SANDBOX === "1" ? ["--no-sandbox"] : []),
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
      `Adapter "${adapterLabel}" is a software renderer; Phase 2A hardware evidence requires actual GPU hardware`,
      {
        adapter: adapterLabel,
        active_device: activeDevice,
        hint: "Run on a machine with a real GPU, or set PHASE2A_ALLOW_SOFTWARE_GPU=1 to capture non-gate diagnostic evidence.",
      },
    );
  }


  // ------------------------------------------------------------------ PATH-A fixture on the GPU
  const fixtureResponse = await send("load_fixture", { fixture: "path-a" });
  await waitFor("window.__PHASE0E_PROOF__?.fixture === 'PATH-A'");
  await fitDocument();

  const pathNodes = Object.values(await nodeTable()).filter((node) => node.kind === "path");
  if (pathNodes.length !== 4) {
    throw new HarnessFailure(
      "path_fixture_incomplete",
      `PATH-A projected ${pathNodes.length} path nodes, expected 4`,
      { nodes: pathNodes.map((node) => node.name) },
    );
  }
  const byName = Object.fromEntries(pathNodes.map((node) => [node.name, node]));
  const fixtureResources = fixtureResponse.resources;
  const fixtureBinary = fixtureResponse.binary;
  const drawBatches = fixtureResources.draw_batches ?? [];

  // ------------------------------------------------------------------------- WebGPU pixel proof
  const backgroundWorld = [-300, 100];
  const background = await readCanvasPixelAtWorld(backgroundWorld);
  const STROKE_SRGB = [13, 18, 28];
  const TRIANGLE_FILL_SRGB = [41, 140, 242];
  const CURVE_FILL_SRGB = [250, 184, 46];
  const PIXEL_TOLERANCE = 70;

  async function samplePath(label, world, expected, { expectBackground = false } = {}) {
    const pixel = await readCanvasPixelAtWorld(world);
    const distanceToExpected = expected ? colorDistance(pixel.rgba, expected) : null;
    const distanceToBackground = colorDistance(pixel.rgba, background.rgba);
    const matched = expectBackground
      ? distanceToBackground <= 24
      : distanceToExpected <= PIXEL_TOLERANCE && distanceToBackground > 24;
    return recordPixelCase({
      case: label,
      world,
      expected_srgb: expected,
      expect_background: expectBackground,
      rgba: pixel.rgba,
      background_rgba: background.rgba,
      distance_to_expected: distanceToExpected,
      distance_to_background: distanceToBackground,
      surface_base_format: pixel.surface_base_format,
      readback_view_format: pixel.readback_view_format,
      matched,
    });
  }

  // 1. Straight open path: on the stroke, and beside it.
  const straightOnStroke = await samplePath("straight_path_stroke", [-240, -220], STROKE_SRGB);
  const straightBeside = await samplePath("straight_path_background", [-240, -180], null, {
    expectBackground: true,
  });
  // 2. Bezier path: a point on the curve that is nowhere near its chord, and the chord itself.
  const curveOnCurve = await samplePath("bezier_path_curve", [-300, -99.375], STROKE_SRGB);
  const curveOnChord = await samplePath("bezier_path_chord_is_empty", [-300, -60], null, {
    expectBackground: true,
  });
  // 3. Closed filled path: interior fill, and a bounding-box corner outside the shape.
  const triangleInterior = await samplePath(
    "closed_fill_interior",
    [180, -130],
    TRIANGLE_FILL_SRGB,
  );
  const triangleOutside = await samplePath("closed_fill_outside_bbox_corner", [95, -70], null, {
    expectBackground: true,
  });
  // 4. Closed stroked cubic: the stroke ring and the fill inside it.
  const ringStroke = await samplePath("closed_curve_stroke", [180, -20], STROKE_SRGB);
  const ringFill = await samplePath("closed_curve_fill", [180, 80], CURVE_FILL_SRGB);
  const backgroundControl = recordPixelCase({
    case: "background_control",
    world: backgroundWorld,
    rgba: background.rgba,
    matched: true,
  });

  await capture(screenshots.paths);

  // --------------------------------------------------------------------------- real hit testing
  // Each case is: clear the selection, click, let the interaction the click opened finish, then
  // read the result. `clickPoint` returns only once the editor is idle again, so no request can
  // race an open interaction transaction.
  const filledId = byName["Closed Filled"].id;
  const straightId = byName["Open Straight"].id;
  await send("selection", { mode: "clear" });
  await clickPoint(await clientPointForWorld([180, -130]), { label: "click on the fill interior" });
  await waitFor("window.__PHASE0E_PROOF__?.selection_count === 1", 30000, "path_pick_timeout");
  const interiorPick = (await selectionIds())[0];

  await send("selection", { mode: "clear" });
  await clickPoint(await clientPointForWorld([95, -70]), {
    label: "click outside the outline, inside the bounding box",
  });
  // A miss opens no interaction and changes no selection, so there is nothing to wait *for*:
  // quiescence has already been established by clickPoint before the selection is read.
  const outsidePick = await selectionIds();

  await send("selection", { mode: "clear" });
  await clickPoint(await clientPointForWorld([-240, -220]), { label: "click on the stroke" });
  await waitFor("window.__PHASE0E_PROOF__?.selection_count === 1", 30000, "path_pick_timeout");
  const strokePick = (await selectionIds())[0];

  // ------------------------------------------------ create, edit, undo and redo a path end to end
  const rootId = Object.values(await nodeTable()).find((node) => node.kind === "document")?.id;
  const createdId = "6f2c3b4a-1d5e-4a7b-8c9d-0e1f2a3b4c5d";
  const anchors = [
    { id: "6f2c3b4a-1d5e-4a7b-8c9d-0e1f2a3b4c01", position: [0, 0] },
    { id: "6f2c3b4a-1d5e-4a7b-8c9d-0e1f2a3b4c02", position: [160, 0] },
    { id: "6f2c3b4a-1d5e-4a7b-8c9d-0e1f2a3b4c03", position: [80, 120] },
  ];
  // The harness mixes real pointer interaction with direct command dispatch. Every crossing of
  // that boundary asserts quiescence first, so a command can never land inside an interaction.
  await waitForInteractionIdle("before dispatching create_path");
  const createdResponse = await send("command", {
    command: {
      kind: "create_path",
      node_id: createdId,
      parent_id: rootId,
      index: 0,
      name: "Proof Path",
      x: -300,
      y: 220,
      closed: true,
      anchors,
    },
  });
  const createdBounds = (await nodeTable())[createdId]?.world_bounds ?? null;
  const createdVertexCount = createdResponse.resources.path_vertex_count;

  const tallerAnchors = anchors.map((anchor, index) =>
    index === 2 ? { ...anchor, position: [80, 260] } : anchor,
  );
  await waitForInteractionIdle("before dispatching set_path_geometry");
  const editedResponse = await send("command", {
    command: {
      kind: "set_path_geometry",
      node_id: createdId,
      closed: true,
      anchors: tallerAnchors,
    },
  });
  const editedBounds = (await nodeTable())[createdId]?.world_bounds ?? null;

  await waitForInteractionIdle("before undo");
  const undoneResponse = await send("undo");
  const undoneBounds = (await nodeTable())[createdId]?.world_bounds ?? null;
  await waitForInteractionIdle("before redo");
  const redoneResponse = await send("redo");
  const redoneBounds = (await nodeTable())[createdId]?.world_bounds ?? null;
  await send("undo");
  const removedResponse = await send("undo");
  const removedFromProjection = (await nodeTable())[createdId] === undefined;

  // ------------------------------------------------------- save, load, and anchor identity
  // The document round trip runs through the same serialization the file format uses, so the
  // proof shows that a rendered path survives a save and load with its anchor identities intact.
  await waitForInteractionIdle("before save_document");
  const saved = await send("save_document");
  const savedDocument = JSON.parse(saved.result.document_json);
  const savedPaths = savedDocument.document.nodes.filter(
    (node) => node.kind?.type === "path",
  );
  const savedAnchorIds = savedPaths.map((node) =>
    node.kind.anchors.map((anchor) => anchor.id),
  );
  const beforeReloadBounds = Object.fromEntries(
    pathNodes.map((node) => [node.name, node.world_bounds]),
  );
  await waitForInteractionIdle("before load_document");
  const reloaded = await send("load_document", {
    document_json: saved.result.document_json,
  });
  const reloadedNodes = Object.values(await nodeTable()).filter((node) => node.kind === "path");
  const afterReloadBounds = Object.fromEntries(
    reloadedNodes.map((node) => [node.name, node.world_bounds]),
  );
  const resaved = await send("save_document");
  const resavedDocument = JSON.parse(resaved.result.document_json);
  const resavedAnchorIds = resavedDocument.document.nodes
    .filter((node) => node.kind?.type === "path")
    .map((node) => node.kind.anchors.map((anchor) => anchor.id));
  await fitDocument();
  const reloadedFillPixel = await samplePath(
    "closed_fill_interior_after_reload",
    [180, -130],
    TRIANGLE_FILL_SRGB,
  );

  // ------------------------------------------------------------------ camera-only frame contract
  const beforeCameraGpu = (await getProof()).gpu ?? {};
  const cameraResponse = await send("camera", { camera: { kind: "pan", dx: 24, dy: 18 } });
  const afterCamera = await getProof();

  const pixelAssertions = {
    straight_path_visible: straightOnStroke.matched,
    straight_path_background_control: straightBeside.matched,
    bezier_path_visible_off_its_chord: curveOnCurve.matched,
    bezier_chord_is_empty: curveOnChord.matched,
    closed_fill_interior_filled: triangleInterior.matched,
    closed_fill_respects_outline: triangleOutside.matched,
    closed_curve_stroke_visible: ringStroke.matched,
    closed_curve_fill_visible: ringFill.matched,
    background_control_captured: Array.isArray(backgroundControl.rgba),
    closed_fill_survives_save_and_load: reloadedFillPixel.matched,
  };
  const pixelReadback = {
    phase: "2A",
    ...gateEvidence,
    proof_kind: "actual-hardware-webgpu-surface-readback",
    captured_at_utc: new Date().toISOString(),
    surface_base_format: initial.surface_base_format,
    pipeline_view_format: initial.pipeline_view_format,
    readback_view_format: initial.readback_view_format,
    note:
      "Every sample is a WebGPU surface readback of document content. No SVG or Canvas2D pixel is asserted here, and paths are never drawn by React.",
    colour_tolerance: PIXEL_TOLERANCE,
    expected_stroke_srgb: STROKE_SRGB,
    expected_triangle_fill_srgb: TRIANGLE_FILL_SRGB,
    expected_curve_fill_srgb: CURVE_FILL_SRGB,
    cases: pixelCases,
    assertions: pixelAssertions,
    assertion_count: Object.keys(pixelAssertions).length,
    passed_assertion_count: Object.values(pixelAssertions).filter(Boolean).length,
    all_passed: Object.values(pixelAssertions).every(Boolean),
  };
  await writeFile(pixelPath, `${JSON.stringify(pixelReadback, null, 2)}\n`, "utf8");

  const metrics = {
    phase: "2A",
    ...gateEvidence,
    captured_at_utc: new Date().toISOString(),
    fixture: "PATH-A",
    path_instance_count: fixtureResources.path_instance_count,
    path_vertex_count: fixtureResources.path_vertex_count,
    draw_batches: drawBatches,
    fixture_frame_metrics: fixtureResponse.metrics,
    single_path_edit: {
      path_tessellations: editedResponse.metrics.path_tessellations,
      path_vertices_uploaded: editedResponse.metrics.path_vertices_uploaded,
      render_full_rebuilds: editedResponse.render_delta.render_full_rebuilds,
      fallback_rebuild_count: editedResponse.metrics.fallback_rebuild_count,
    },
    camera_only_frame: {
      path_tessellations: cameraResponse.metrics.path_tessellations,
      path_vertices_uploaded: cameraResponse.metrics.path_vertices_uploaded,
      path_vertex_upload_bytes: cameraResponse.metrics.path_vertex_upload_bytes,
      path_vertices_sent: Boolean(cameraResponse.binary.path_vertices),
      gpu_draw_calls: afterCamera.gpu?.draw_calls ?? null,
      gpu_batches: afterCamera.gpu?.batches ?? null,
      gpu_path_vertices_uploaded: afterCamera.gpu?.path_vertices_uploaded ?? null,
      gpu_path_vertex_upload_bytes: afterCamera.gpu?.path_vertex_upload_bytes ?? null,
      gpu_draw_calls_before_camera_frame: beforeCameraGpu.draw_calls ?? null,
    },
    document_round_trip: {
      saved_path_count: savedPaths.length,
      saved_anchor_ids: savedAnchorIds,
      resaved_anchor_ids: resavedAnchorIds,
      bounds_before_reload: beforeReloadBounds,
      bounds_after_reload: afterReloadBounds,
      reloaded_ok: reloaded.ok === true,
    },
    gpu: afterCamera.gpu,
    note:
      "Path CPU scaling at 10/100/1000 paths is measured by cargo run -p visual_authoring_runtime --bin phase2a_path_bench; this artifact records the actual-GPU frame behaviour.",
  };
  await writeFile(metricsPath, `${JSON.stringify(metrics, null, 2)}\n`, "utf8");

  const finalProof = await getProof();
  const checks = {
    server_health: preview.health.ok === true,
    asset_http_and_wasm_mime: Object.values(preview.assets).every(
      (asset) => asset.status === 200,
    ),
    new_chrome_process: Boolean(chromeProcess.pid),
    temporary_chrome_profile: profile.includes("phase2a-chrome-"),
    navigator_gpu: initial.actual_webgpu === true,
    hardware_gpu_adapter: softwareRenderer === false,
    dedicated_worker_wasm:
      initial.worker_runtime_owner === "dedicated-worker" &&
      initial.wasm_initialized === true &&
      initial.heartbeat >= 1,
    render_binary_schema_v3:
      initial.render_binary_schema_version === 3 &&
      fixtureResources.path_instance_stride_bytes === 80 &&
      fixtureResources.path_vertex_stride_bytes === 32,
    fixture_projects_four_paths: pathNodes.length === 4,
    paths_reach_the_gpu_as_triangles:
      fixtureResources.path_instance_count === 4 &&
      fixtureResources.path_vertex_count > 0 &&
      fixtureResources.path_vertex_count % 3 === 0 &&
      fixtureBinary.path_instances === true &&
      fixtureBinary.path_vertices === true,
    ordered_path_draw_batches:
      drawBatches.length > 0 &&
      drawBatches.every((batch) => batch.kind === "paths" || batch.kind === "primitives") &&
      drawBatches
        .filter((batch) => batch.kind === "paths")
        .reduce((total, batch) => total + batch.vertex_count, 0) ===
        fixtureResources.path_vertex_count,
    actual_pixel_readback: pixelReadback.all_passed,
    filled_interior_is_pickable: interiorPick === filledId,
    outline_only_region_is_not_pickable: outsidePick.length === 0,
    stroke_is_pickable: strokePick === straightId,
    create_path_command_round_trip:
      createdResponse.ok === true &&
      createdVertexCount > 0 &&
      createdBounds !== null,
    edit_path_tessellates_only_that_path:
      editedResponse.ok === true &&
      editedResponse.metrics.path_tessellations === 1 &&
      editedResponse.render_delta.render_full_rebuilds === 0 &&
      editedBounds !== null &&
      editedBounds.max[1] > createdBounds.max[1],
    undo_restores_path_geometry:
      undoneResponse.ok === true &&
      undoneBounds !== null &&
      Math.abs(undoneBounds.max[1] - createdBounds.max[1]) < 1e-6,
    redo_reapplies_path_geometry:
      redoneResponse.ok === true &&
      redoneBounds !== null &&
      Math.abs(redoneBounds.max[1] - editedBounds.max[1]) < 1e-6,
    undo_removes_the_created_path:
      removedResponse.ok === true &&
      removedFromProjection &&
      removedResponse.resources.path_instance_count === 4,
    camera_only_uploads_no_triangles:
      cameraResponse.ok === true &&
      cameraResponse.binary.path_vertices === false &&
      cameraResponse.metrics.path_vertices_uploaded === 0 &&
      cameraResponse.metrics.path_vertex_upload_bytes === 0 &&
      cameraResponse.metrics.path_tessellations === 0,
    save_load_preserves_path_identity:
      saved.ok === true &&
      reloaded.ok === true &&
      savedPaths.length === 4 &&
      reloadedNodes.length === 4 &&
      JSON.stringify(savedAnchorIds) === JSON.stringify(resavedAnchorIds) &&
      JSON.stringify(beforeReloadBounds) === JSON.stringify(afterReloadBounds),
    reloaded_path_still_renders: reloadedFillPixel.matched,
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
    phase: "2A",
    ...gateEvidence,
    proof_kind: allowSoftwareGpu
      ? "software-gpu-diagnostic-run-not-gate-evidence"
      : "actual-hardware-browser",
    captured_at_utc: browserProofFinishedAtUtc,
    browser: version.Browser,
    browser_mode: "actual Chrome; no mock; no Canvas2D fallback",
    execution: {
      command: "npm run test:browser:phase2a:path-render",
      started_at_utc: browserProofStartedAtUtc,
      finished_at_utc: browserProofFinishedAtUtc,
      software_gpu_allowed: allowSoftwareGpu,
      chrome_sandbox_disabled: process.env.PHASE2A_NO_SANDBOX === "1",
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
    fixture: {
      name: "PATH-A",
      paths: pathNodes.map((node) => ({
        id: node.id,
        name: node.name,
        world_bounds: node.world_bounds,
      })),
      resources: fixtureResources,
      binary: fixtureBinary,
    },
    hit_testing: {
      filled_interior_pick: interiorPick,
      filled_interior_expected: filledId,
      outside_outline_pick: outsidePick,
      stroke_pick: strokePick,
      stroke_expected: straightId,
    },
    editing: {
      created_bounds: createdBounds,
      edited_bounds: editedBounds,
      undone_bounds: undoneBounds,
      redone_bounds: redoneBounds,
      removed_from_projection: removedFromProjection,
    },
    camera_only_frame: metrics.camera_only_frame,
    document_round_trip: metrics.document_round_trip,
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

  console.log(`Phase 2A actual browser proof: ${proof.all_passed ? "PASS" : "FAIL"}`);
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
    phase: "2A",
    ...gateEvidence,
    captured_at_utc: new Date().toISOString(),
    execution: {
      command: "npm run test:browser:phase2a:path-render",
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
  // Process cleanup only: give Chrome a moment to release the profile directory before deleting
  // it. This is not synchronisation with the editor; every such wait is state-based.
  await sleep(300);
  await rm(profile, { recursive: true, force: true }).catch(() => {});
}

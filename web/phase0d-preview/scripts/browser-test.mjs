import { spawn } from "node:child_process";
import { readFile, rm, writeFile } from "node:fs/promises";
import { createServer as createNetServer } from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  EXPECTED_PREVIEW_ASSETS,
  HarnessFailure,
  classifyApplicationFailure,
  validateAssetResponse,
} from "./harness-support.mjs";

const scriptRoot = path.dirname(fileURLToPath(import.meta.url));
const workspace = path.resolve(scriptRoot, "../../..");
const webRoot = path.resolve(scriptRoot, "..");
const proofPath = path.join(workspace, "docs/verification/PHASE_0D_R1_BROWSER_PROOF.json");
const screenshotPath = path.join(workspace, "docs/verification/phase0d-r1-preview.png");
const pixelPath = path.join(workspace, "docs/verification/PHASE_0D_R1_PIXEL_READBACK.json");
const chrome = process.env.PHASE0D_CHROME ?? "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";
const profile = path.join(os.tmpdir(), `phase0d-chrome-${Date.now()}`);
const consoleErrors = [];
let stderr = "";
let browserClient;
let pageClient;

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

  close() {
    this.socket?.close();
  }
}

async function waitForFile(file, timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      return await readFile(file, "utf8");
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
  throw new Error(`Timed out waiting for ${file}`);
}

async function evaluate(expression) {
  const result = await pageClient.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
    userGesture: true,
  });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
  }
  return result.result.value;
}

async function waitFor(expression, timeoutMs = 30000) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      if (await evaluate(`Boolean(${expression})`)) return;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Timed out waiting for ${expression}${lastError ? `: ${lastError.message}` : ""}`);
}

async function waitForApplicationReady(timeoutMs = 30000) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  let lastStatus;
  while (Date.now() < deadline) {
    try {
      lastStatus = await evaluate(`({
        ready: document.body?.dataset.ready === "true" &&
          window.__PHASE0D_PROOF__?.worker_heartbeat >= 1,
        body_ready: document.body?.dataset.ready ?? null,
        last_error: window.__phase0dState?.lastError ?? null,
        worker_ready: window.__phase0dState?.workerReady ?? false,
        webgpu_available: window.__phase0dState?.webgpuAvailable ?? false,
        navigator_gpu: Boolean(navigator.gpu),
      })`);
      if (lastStatus?.ready) return lastStatus;
      if (lastStatus?.last_error) {
        throw classifyApplicationFailure(lastStatus.last_error);
      }
    } catch (error) {
      if (error instanceof HarnessFailure) throw error;
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new HarnessFailure(
    "application_readiness_timeout",
    "Application did not initialize before the readiness deadline",
    { status: lastStatus, last_error: lastError?.message },
  );
}
function request(type, payload = {}, timeout = 30000) {
  return evaluate(
    `window.__phase0dRequestAndWait(${JSON.stringify(type)}, ${JSON.stringify(payload)}, ${timeout})`,
  );
}

function worldToViewport(response, x, y) {
  return {
    x: (x - response.camera.center[0]) * response.camera.zoom + response.camera.viewport[0] * 0.5,
    y: (y - response.camera.center[1]) * response.camera.zoom + response.camera.viewport[1] * 0.5,
  };
}

async function readPixelAtWorld(response, x, y) {
  const point = worldToViewport(response, x, y);
  const pixel = await evaluate(
    "window.__phase0dReadPixel(" + JSON.stringify(point.x) + ", " + JSON.stringify(point.y) + ")",
  );
  return { world: [x, y], viewport: [point.x, point.y], ...pixel };
}

function colorMatches(pixel, expected, tolerance = 24) {
  return pixel.rgba.slice(0, 3).every((value, index) => Math.abs(value - expected[index]) <= tolerance);
}

async function elementCenter(selector) {
  return evaluate(
    "(() => { const r = document.querySelector(" + JSON.stringify(selector) +
      ").getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2, width: r.width, height: r.height }; })()",
  );
}

async function clickElement(selector) {
  const point = await elementCenter(selector);
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: point.x,
    y: point.y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: point.x,
    y: point.y,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
}

async function pointerMoveBurst(count) {
  const bounds = await elementCenter("#viewport");
  const startX = bounds.x - Math.min(100, bounds.width * 0.2);
  const startY = bounds.y;
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: startX,
    y: startY,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  const moves = [];
  for (let index = 1; index <= count; index += 1) {
    moves.push(
      pageClient.send("Input.dispatchMouseEvent", {
        type: "mouseMoved",
        x: startX + (index / count) * Math.min(160, bounds.width * 0.3),
        y: startY + Math.sin(index / 8) * 3,
        button: "left",
        buttons: 1,
      }),
    );
  }
  await Promise.all(moves);
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: startX + Math.min(160, bounds.width * 0.3),
    y: startY,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
  return { sent: count, bounds };
}

async function wheelBurst(count) {
  const point = await elementCenter("#viewport");
  const events = [];
  for (let index = 0; index < count; index += 1) {
    events.push(
      pageClient.send("Input.dispatchMouseEvent", {
        type: "mouseWheel",
        x: point.x,
        y: point.y,
        deltaX: 0,
        deltaY: index % 2 === 0 ? -3 : -2,
      }),
    );
  }
  await Promise.all(events);
  return { sent: count, point };
}

async function selectFixture(value) {
  await evaluate(
    "(() => { const select = document.querySelector('#fixture'); select.value = " +
      JSON.stringify(value) +
      "; select.dispatchEvent(new Event('change', { bubbles: true })); return true; })()",
  );
}
async function reservePort() {
  const listener = createNetServer();
  await new Promise((resolve, reject) => {
    listener.once("error", reject);
    listener.listen(0, "127.0.0.1", resolve);
  });
  const address = listener.address();
  const port = typeof address === "object" && address ? address.port : 0;
  await new Promise((resolve) => listener.close(resolve));
  if (!port) throw new HarnessFailure("port_allocation_failed", "No localhost port was allocated");
  return port;
}

async function waitForHealth(url, processState, timeoutMs = 10000) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    if (processState.exited) {
      throw new HarnessFailure(
        "preview_server_exited",
        "Preview server exited before health succeeded",
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
          "Preview health returned HTTP " + response.status,
        );
      }
      const health = await response.json();
      if (health.ok === true) return health;
      throw new HarnessFailure("preview_health_invalid", "Preview health payload was invalid", health);
    } catch (error) {
      lastError = error;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }
  throw new HarnessFailure(
    "preview_health_timeout",
    "Preview health did not become ready",
    { last_error: lastError?.message, ...processState },
  );
}

async function verifyAssets(url) {
  const assets = {};
  for (const [pathname, mime] of EXPECTED_PREVIEW_ASSETS) {
    let response;
    try {
      response = await fetch(new URL(pathname, url), {
        signal: AbortSignal.timeout(3000),
      });
    } catch (error) {
      throw new HarnessFailure(
        "preview_asset_connection_failed",
        pathname + " could not be fetched: " + error.message,
      );
    }
    const contentType = response.headers.get("content-type") ?? "";
    assets[pathname] = validateAssetResponse(pathname, mime, {
      ok: response.ok,
      status: response.status,
      contentType,
    });
  }
  return assets;
}

async function preparePreview() {
  if (process.env.PHASE0D_URL) {
    const url = new URL(process.env.PHASE0D_URL);
    const health = await waitForHealth(url, { external: true });
    const assets = await verifyAssets(url);
    return { url: url.href, server: null, health, assets, managed: false, processState: null };
  }

  const port = await reservePort();
  const url = new URL("http://127.0.0.1:" + port + "/");
  const processState = { exited: false, exit_code: null, stdout: "", stderr: "" };
  const server = spawn(process.execPath, ["server.mjs", "--port=" + port], {
    cwd: webRoot,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  server.stdout.setEncoding("utf8");
  server.stderr.setEncoding("utf8");
  server.stdout.on("data", (chunk) => {
    processState.stdout = (processState.stdout + chunk).slice(-20000);
  });
  server.stderr.on("data", (chunk) => {
    processState.stderr = (processState.stderr + chunk).slice(-20000);
  });
  server.once("exit", (code) => {
    processState.exited = true;
    processState.exit_code = code;
  });
  try {
    const health = await waitForHealth(url, processState);
    const assets = await verifyAssets(url);
    return { url: url.href, server, health, assets, managed: true, processState };
  } catch (error) {
    server.kill();
    throw error;
  }
}

const preview = await preparePreview();
const previewUrl = preview.url;
const chromeProcess = spawn(
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
  stderr = `${stderr}${chunk}`.slice(-20000);
});

try {
  const activePortFile = path.join(profile, "DevToolsActivePort");
  const [port] = (await waitForFile(activePortFile)).trim().split(/\r?\n/);
  const base = `http://127.0.0.1:${port}`;
  const version = await (await fetch(`${base}/json/version`)).json();
  browserClient = new CdpClient(version.webSocketDebuggerUrl);
  await browserClient.connect();
  const gpuInfo = await browserClient.send("SystemInfo.getInfo");

  const target = await (
    await fetch(`${base}/json/new?${encodeURIComponent(previewUrl)}`, { method: "PUT" })
  ).json();
  pageClient = new CdpClient(target.webSocketDebuggerUrl);
  await pageClient.connect();
  pageClient.on("Runtime.exceptionThrown", (details) => {
    consoleErrors.push(details.exceptionDetails.exception?.description ?? details.exceptionDetails.text);
  });
  pageClient.on("Runtime.consoleAPICalled", (details) => {
    if (details.type === "error" || details.type === "assert") {
      consoleErrors.push(details.args.map((argument) => argument.value ?? argument.description).join(" "));
    }
  });
  pageClient.on("Log.entryAdded", ({ entry }) => {
    if (entry.level === "error") consoleErrors.push(entry.text);
  });
  await pageClient.send("Runtime.enable");
  await pageClient.send("Page.enable");
  await pageClient.send("Log.enable");
  await waitForApplicationReady();

  const initial = await evaluate("window.__PHASE0D_PROOF__");
  const initialRevisions = { ...initial.revisions };
  const canvasBounds = await evaluate(
    "(() => { const r = document.querySelector('#viewport').getBoundingClientRect(); return { left: r.left, top: r.top, width: r.width, height: r.height }; })()",
  );
  const resize = await request("camera", {
    camera: {
      kind: "resize",
      width: canvasBounds.width,
      height: canvasBounds.height,
      dpr: 1,
    },
  });
  const fitted = await request("camera", { camera: { kind: "fit" } });

  const rectanglePixel = await readPixelAtWorld(fitted.response, -100, -20);
  const ellipseCenterPixel = await readPixelAtWorld(fitted.response, 160, 40);
  const ellipseCornerPixel = await readPixelAtWorld(fitted.response, 245, -45);
  const overlapInitialPixel = await readPixelAtWorld(fitted.response, 78, 40);

  const reorder = await request("command", {
    command: { kind: "reorder_node", node_index: 1, index: 1 },
  });
  const overlapReorderedPixel = await readPixelAtWorld(reorder.response, 78, 40);
  const undoReorder = await request("undo");
  const overlapUndoPixel = await readPixelAtWorld(undoReorder.response, 78, 40);
  const redoReorder = await request("redo");
  const overlapRedoPixel = await readPixelAtWorld(redoReorder.response, 78, 40);
  await request("undo");

  const pan = await request("camera", { camera: { kind: "pan", dx: 80, dy: 40 } });
  const zoom = await request("camera", {
    camera: { kind: "zoom", x: canvasBounds.width / 2, y: canvasBounds.height / 2, zoom: 1.25 },
  });
  const hit = await request("hit_test", {
    x: canvasBounds.width * 0.65,
    y: canvasBounds.height * 0.55,
  });
  const move = await request("command", {
    command: { kind: "move_node", node_index: 1, x: 120, y: 60 },
  });
  const undo = await request("undo");
  const redo = await request("redo");

  await request("load_fixture", { fixture: "bench-b" }, 45000);
  const tenThousand = await request("camera", { camera: { kind: "fit" } }, 45000);
  const offscreen = await request(
    "camera",
    { camera: { kind: "pan", dx: 100000000, dy: 100000000 } },
    45000,
  );
  const offscreenPixel = await evaluate(
    "window.__phase0dReadPixel(" +
      JSON.stringify(offscreen.response.camera.viewport[0] / 2) +
      ", " +
      JSON.stringify(offscreen.response.camera.viewport[1] / 2) +
      ")",
  );
  const sparse = await request("load_fixture", { fixture: "bench-c" }, 60000);
  await request("load_fixture", { fixture: "preview" }, 45000);
  await request("camera", {
    camera: {
      kind: "resize",
      width: canvasBounds.width,
      height: canvasBounds.height,
      dpr: 1,
    },
  });
  const finalPreview = await request("camera", { camera: { kind: "fit" } }, 45000);

  const domMoveBefore = await evaluate("window.__PHASE0D_PROOF__.revisions.document");
  await clickElement("#move-node");
  await waitFor("window.__PHASE0D_PROOF__.revisions.document > " + domMoveBefore);
  const domMove = await evaluate("window.__PHASE0D_PROOF__");
  await clickElement("#undo");
  await waitFor("window.__PHASE0D_PROOF__.revisions.document > " + domMove.revisions.document);
  const domUndo = await evaluate("window.__PHASE0D_PROOF__");
  await clickElement("#redo");
  await waitFor("window.__PHASE0D_PROOF__.revisions.document > " + domUndo.revisions.document);
  const domRedo = await evaluate("window.__PHASE0D_PROOF__");

  await selectFixture("bench-a");
  await waitFor("window.__PHASE0D_PROOF__.fixture === 'BENCH-A'", 45000);
  const domFixture = await evaluate("window.__PHASE0D_PROOF__");
  await selectFixture("preview");
  await waitFor("window.__PHASE0D_PROOF__.fixture === 'PREVIEW'", 45000);
  await clickElement("#fit-view");
  await waitFor("window.__PHASE0D_PROOF__.response_gpu_sequence_match === true");

  const pointerBurst = await pointerMoveBurst(500);
  const wheel = await wheelBurst(120);
  const heartbeatDuringCamera = request("heartbeat");
  await clickElement("#move-node");
  const heartbeatResult = await heartbeatDuringCamera;
  const burstFinal = await evaluate("window.__phase0dWaitForIdle()");

  await clickElement("#reset-view");
  await evaluate("window.__phase0dWaitForIdle()");
  await clickElement("#fit-view");
  const hitReady = await evaluate("window.__phase0dWaitForIdle()");
  const ellipseViewport = worldToViewport(
    await evaluate("window.__phase0dState.response"),
    160,
    40,
  );
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mousePressed",
    x: canvasBounds.left + ellipseViewport.x,
    y: canvasBounds.top + ellipseViewport.y,
    button: "left",
    buttons: 1,
    clickCount: 1,
  });
  await pageClient.send("Input.dispatchMouseEvent", {
    type: "mouseReleased",
    x: canvasBounds.left + ellipseViewport.x,
    y: canvasBounds.top + ellipseViewport.y,
    button: "left",
    buttons: 0,
    clickCount: 1,
  });
  await waitFor("window.__phase0dState.lastHit !== null");
  const domHit = await evaluate("window.__PHASE0D_PROOF__");

  const f32Omitted = await request("command", {
    command: { kind: "move_node", node_index: 1, x: 1e100, y: 0 },
  });
  const f32Recovered = await request("command", {
    command: { kind: "move_node", node_index: 1, x: -160, y: -80 },
  });

  await evaluate(
    "window.__restartWaiterOutcome = 'pending';" +
      "window.__phase0dRequestAndWait('load_fixture', { fixture: 'bench-c' }, 60000)" +
      ".then(() => { window.__restartWaiterOutcome = 'resolved'; })" +
      ".catch((error) => { window.__restartWaiterOutcome = error.code || error.message; }); true",
  );
  const beforeRestart = await evaluate("window.__phase0dState.workerRestarts");
  await clickElement("#restart-worker");
  await waitFor(
    "window.__restartWaiterOutcome !== 'pending' && " +
      "window.__phase0dState.workerRestarts > " + beforeRestart + " && " +
      "document.body.dataset.ready === 'true' && window.__PHASE0D_PROOF__.worker_heartbeat >= 1",
    60000,
  );
  const restartWaiterOutcome = await evaluate("window.__restartWaiterOutcome");
  const restarted = await evaluate("window.__PHASE0D_PROOF__");
  await request("camera", { camera: { kind: "fit" } }, 45000);

  await evaluate(
    "window.__phase0dSetFrameDelays([180, 0]);" +
      "window.__orderedFrameOutcome = 'pending';" +
      "Promise.all([" +
      "window.__phase0dRequestAndWait('command', { command: { kind: 'move_node', node_index: 1, x: 20, y: 20 } }, 30000)," +
      "window.__phase0dRequestAndWait('command', { command: { kind: 'move_node', node_index: 1, x: 40, y: 40 } }, 30000)" +
      "]).then((values) => { window.__orderedFrameSequences = values.map((value) => value.response.engine_sequence); window.__orderedFrameOutcome = 'resolved'; })" +
      ".catch((error) => { window.__orderedFrameOutcome = error.code || error.message; }); true",
  );
  await waitFor("window.__orderedFrameOutcome !== 'pending'", 45000);
  const orderedFrameOutcome = await evaluate(
    "({ outcome: window.__orderedFrameOutcome, sequences: window.__orderedFrameSequences ?? [] })",
  );
  const final = await evaluate("window.__PHASE0D_PROOF__");

  const expected = {
    rectangle: [51, 148, 245],
    ellipse: [245, 115, 64],
    background: [9, 14, 20],
  };
  const pixelAssertions = {
    rectangle_inside_is_rectangle_color: colorMatches(rectanglePixel, expected.rectangle),
    ellipse_center_is_ellipse_color: colorMatches(ellipseCenterPixel, expected.ellipse),
    ellipse_aabb_corner_is_background: colorMatches(ellipseCornerPixel, expected.background),
    overlap_initial_topmost_is_ellipse: colorMatches(overlapInitialPixel, expected.ellipse),
    overlap_reorder_topmost_is_rectangle: colorMatches(overlapReorderedPixel, expected.rectangle),
    overlap_undo_restores_ellipse: colorMatches(overlapUndoPixel, expected.ellipse),
    overlap_redo_restores_rectangle: colorMatches(overlapRedoPixel, expected.rectangle),
    offscreen_object_pixel_is_background: colorMatches(offscreenPixel, expected.background),
  };
  const pixelReadback = {
    captured_at_utc: new Date().toISOString(),
    expected,
    samples: {
      rectangle_inside: rectanglePixel,
      ellipse_center: ellipseCenterPixel,
      ellipse_aabb_corner: ellipseCornerPixel,
      overlap_initial: overlapInitialPixel,
      overlap_reordered: overlapReorderedPixel,
      overlap_undo: overlapUndoPixel,
      overlap_redo: overlapRedoPixel,
      offscreen_center: offscreenPixel,
    },
    assertions: pixelAssertions,
    all_passed: Object.values(pixelAssertions).every(Boolean),
  };
  await writeFile(pixelPath, JSON.stringify(pixelReadback, null, 2) + "\n", "utf8");
  const screenshot = await pageClient.send("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
    fromSurface: true,
  });
  await writeFile(screenshotPath, Buffer.from(screenshot.data, "base64"));

  const devices = gpuInfo.gpu?.devices ?? [];
  const activeDevice = devices.find((device) => device.active) ?? devices[0] ?? {};
  const actualDeviceText = JSON.stringify(activeDevice).toLowerCase();
  const actualHardware =
    devices.length > 0 &&
    !actualDeviceText.includes("swiftshader") &&
    !actualDeviceText.includes("software rasterizer");
  const cameraRevisionsStable =
    pan.proof.revisions.document === pan.proof.revisions.scene &&
    pan.proof.revisions.scene === pan.proof.revisions.render &&
    zoom.proof.revisions.document === zoom.proof.revisions.scene &&
    zoom.proof.revisions.scene === zoom.proof.revisions.render;
  const orderedSequences = orderedFrameOutcome.sequences;
  const checks = {
    worker_wasm_initialized:
      initial.wasm_initialized === true && initial.worker_runtime_owner === "dedicated-worker",
    main_thread_has_no_document_mutation: initial.main_thread_document_mutation_api === false,
    worker_heartbeat_and_restart:
      initial.worker_heartbeat >= 1 &&
      restarted.worker_heartbeat >= 1 &&
      restarted.worker_restarts > initial.worker_restarts,
    actual_webgpu_device: initial.actual_webgpu === true && actualHardware,
    render_schema_agreement:
      initial.render_binary_schema_version === 1 &&
      final.render_binary_schema_version === 1,
    actual_pixel_readback: pixelReadback.all_passed === true,
    resize_and_dpr:
      Math.abs(resize.response.camera.viewport[0] - canvasBounds.width) < 0.01 &&
      Math.abs(resize.response.camera.viewport[1] - canvasBounds.height) < 0.01 &&
      resize.response.camera.dpr === 1,
    pan_zoom_hit_test:
      cameraRevisionsStable &&
      hit.response.ok === true,
    command_scene_render_delta:
      move.response.ok === true &&
      move.response.render_delta.dirty_slots === 1 &&
      move.response.render_delta.render_full_rebuilds === 0,
    undo_redo:
      undo.response.result.changed === true && redo.response.result.changed === true,
    reorder_pixel_undo_redo:
      reorder.response.ok === true &&
      undoReorder.response.result.changed === true &&
      redoReorder.response.result.changed === true &&
      pixelAssertions.overlap_reorder_topmost_is_rectangle &&
      pixelAssertions.overlap_undo_restores_ellipse &&
      pixelAssertions.overlap_redo_restores_rectangle,
    offscreen_culling_and_pixel:
      offscreen.proof.culling.visible === 0 &&
      offscreen.proof.culling.submitted_instances === 0 &&
      offscreen.proof.gpu.submitted_instances === 0 &&
      pixelAssertions.offscreen_object_pixel_is_background,
    ten_thousand_instanced:
      tenThousand.proof.culling.total === 10000 &&
      tenThousand.proof.culling.visible === 10000 &&
      tenThousand.proof.gpu.draw_calls === 1 &&
      tenThousand.proof.gpu.batches === 1 &&
      tenThousand.proof.gpu.submitted_instances === 10000 &&
      tenThousand.proof.work_counters.sibling_search_steps === 0,
    sparse_hundred_thousand_pruned:
      sparse.proof.culling.total === 100000 &&
      sparse.proof.culling.spatial_candidates < sparse.proof.culling.total,
    f32_omission_and_recovery:
      f32Omitted.response.ok === true &&
      f32Omitted.response.render_encoding.gpu_omitted_items === 1 &&
      f32Omitted.response.render_encoding.diagnostics[0].reason ===
        "translation_outside_f32" &&
      f32Omitted.proof.culling.visible === 1 &&
      f32Omitted.response.metrics.render_items_cloned === 0 &&
      f32Omitted.response.metrics.full_render_model_scans === 0 &&
      f32Omitted.response.metrics.order_nodes_visited === 0 &&
      f32Omitted.response.metrics.document_nodes_scanned === 1 &&
      f32Omitted.response.metrics.render_items_planned === 1 &&
      f32Omitted.response.metrics.instance_upload_bytes === 48 &&
      f32Recovered.response.render_encoding.gpu_omitted_items === 0 &&
      f32Recovered.proof.culling.visible === 2,
    actual_dom_controls:
      domMove.revisions.document < domUndo.revisions.document &&
      domUndo.revisions.document < domRedo.revisions.document &&
      domFixture.fixture === "BENCH-A" &&
      domHit.hit_test_topmost !== null,
    camera_burst_coalesced:
      pointerBurst.sent === 500 &&
      wheel.sent === 120 &&
      burstFinal.camera_requests_coalesced > 0 &&
      burstFinal.camera_requests_sent < burstFinal.camera_raw_intents &&
      burstFinal.camera_request_in_flight === false,
    heartbeat_does_not_replace_frame:
      heartbeatResult.response.result.heartbeat === true &&
      heartbeatResult.response.engine_sequence >
        heartbeatResult.proof.engine_sequence &&
      heartbeatResult.proof.response_gpu_sequence_match === true,
    restart_rejects_waiter:
      restartWaiterOutcome === "worker_restarted" &&
      restarted.response_gpu_sequence_match === true,
    ordered_gpu_frames:
      orderedFrameOutcome.outcome === "resolved" &&
      orderedSequences.length === 2 &&
      orderedSequences[0] < orderedSequences[1] &&
      final.engine_sequence === orderedSequences[1] &&
      final.gpu_frame_sequence === final.engine_sequence &&
      final.gpu.frame_sequence === final.engine_sequence &&
      final.response_gpu_sequence_match === true,
    revisions_match:
      final.revisions.document === final.revisions.scene &&
      final.revisions.scene === final.revisions.render,
    fallback_rebuilds_zero: final.fallback_rebuild_count === 0,
    gpu_validation_errors_zero: final.gpu_validation_errors === 0,
    browser_console_errors_zero: consoleErrors.length === 0 && final.console_errors === 0,
  };
  const proof = {
    phase: "0D-R1",
    captured_at_utc: new Date().toISOString(),
    proof_kind: "actual-hardware-browser",
    preview_url: previewUrl,
    harness: {
      self_contained_server: preview.managed,
      health: preview.health,
      assets: preview.assets,
      temporary_chrome_profile: profile,
    },
    browser: {
      product: version.Browser,
      user_agent: version["User-Agent"],
      protocol_version: version["Protocol-Version"],
      command_line_mode: "headless=new with --enable-unsafe-webgpu; no WebGPU mock",
    },
    hardware: {
      actual_hardware: actualHardware,
      active_device: activeDevice,
      devices,
      aux_attributes: gpuInfo.gpu?.auxAttributes ?? {},
      feature_status: gpuInfo.gpu?.featureStatus ?? {},
    },
    runtime: final,
    webgpu: {
      actual_webgpu: final.actual_webgpu,
      adapter: final.adapter,
      vendor: final.vendor,
      architecture: final.architecture,
      device: final.device,
      backend: final.backend,
      gpu_validation_errors: final.gpu_validation_errors,
      renderer_location: "main-thread",
      core_location: "dedicated-worker",
    },
    scenarios: {
      initial_rectangle_ellipse: initial,
      resize: resize,
      fit_for_pixel_proof: fitted,
      pixel_readback: pixelReadback,
      reorder: reorder,
      reorder_undo: undoReorder,
      reorder_redo: redoReorder,
      pan,
      zoom,
      hit_test: hit,
      single_node_command: move,
      undo,
      redo,
      ten_thousand_fit: tenThousand,
      ten_thousand_offscreen: offscreen,
      sparse_hundred_thousand_narrow: sparse,
      final_preview: finalPreview,
      dom_move: domMove,
      dom_undo: domUndo,
      dom_redo: domRedo,
      dom_fixture: domFixture,
      dom_hit: domHit,
      hit_ready_frame: hitReady,
      pointer_burst: {
        dispatched_pointer_moves: pointerBurst.sent,
        dispatched_wheel_events: wheel.sent,
        final: burstFinal,
      },
      heartbeat_during_camera: heartbeatResult,
      f32_omitted: f32Omitted,
      f32_recovered: f32Recovered,
      worker_restart: {
        waiter_outcome: restartWaiterOutcome,
        state: restarted,
      },
      ordered_frames: orderedFrameOutcome,
    },
    request_sequences: {
      resize: resize.response.engine_sequence,
      fit: fitted.response.engine_sequence,
      reorder: reorder.response.engine_sequence,
      reorder_undo: undoReorder.response.engine_sequence,
      reorder_redo: redoReorder.response.engine_sequence,
      pan: pan.response.engine_sequence,
      zoom: zoom.response.engine_sequence,
      command: move.response.engine_sequence,
      undo: undo.response.engine_sequence,
      redo: redo.response.engine_sequence,
      heartbeat_during_camera: heartbeatResult.response.engine_sequence,
      f32_omitted: f32Omitted.response.engine_sequence,
      f32_recovered: f32Recovered.response.engine_sequence,
      ordered_frames: orderedSequences,
      final_gpu_frame: final.gpu_frame_sequence,
    },
    cpu_update_ms: {
      ten_thousand_fit: tenThousand.response.metrics.cpu_update_ms,
      sparse_hundred_thousand_load: sparse.response.metrics.cpu_update_ms,
      f32_omitted: f32Omitted.response.metrics.cpu_update_ms,
      f32_recovered: f32Recovered.response.metrics.cpu_update_ms,
    },
    screenshot: "docs/verification/phase0d-r1-preview.png",
    pixel_readback_artifact: "docs/verification/PHASE_0D_R1_PIXEL_READBACK.json",
    console_errors: consoleErrors,
    checks,
    all_passed: Object.values(checks).every(Boolean),
  };  await writeFile(proofPath, `${JSON.stringify(proof, null, 2)}\n`, "utf8");
  console.log(`Phase 0D actual browser proof: ${proof.all_passed ? "PASS" : "FAIL"}`);
  console.log(`browser=${version.Browser}`);
  console.log(`gpu=${activeDevice.deviceString ?? activeDevice.deviceId ?? "unknown"}`);
  console.log(`adapter=${final.adapter} backend=${final.backend}`);
  console.log(`screenshot=${screenshotPath}`);
  console.log(`proof=${proofPath}`);
  if (!proof.all_passed) {
    console.log(JSON.stringify(checks, null, 2));
    process.exitCode = 1;
  }
} catch (error) {
  let browserDiagnostics = {};
  if (pageClient) {
    browserDiagnostics = await evaluate(
      "({body_ready: document.body?.dataset.ready ?? null, phase0d_state: window.__phase0dState ?? null, phase0d_proof: window.__PHASE0D_PROOF__ ?? null, navigator_gpu: Boolean(navigator.gpu), last_error: window.__phase0dState?.lastError ?? null})",
    ).catch((diagnosticError) => ({ diagnostic_error: diagnosticError.message }));
  }
  console.error(JSON.stringify({
    code: error?.code ?? "browser_test_failed",
    message: error?.message ?? String(error),
    details: error?.details ?? null,
    preview_url: previewUrl,
    preview_health: preview.health,
    preview_assets: preview.assets,
    preview_server: preview.processState,
    browser: browserDiagnostics,
  }, null, 2));
  console.error(error);
  if (stderr) console.error(`Chrome stderr tail:\n${stderr}`);
  process.exitCode = 1;
} finally {
  pageClient?.close();
  browserClient?.close();
  chromeProcess.kill();
  preview.server?.kill();
  await new Promise((resolve) => setTimeout(resolve, 300));
  await rm(profile, { recursive: true, force: true }).catch(() => {});
}
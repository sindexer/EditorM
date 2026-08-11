import { assertEngineResponse, createRequest, PROTOCOL_VERSION } from "./protocol.js";
import { RendererFailure, WebGpuRenderer } from "./renderer.js";

const elements = Object.fromEntries(
  [...document.querySelectorAll("[data-bind]")].map((element) => [element.dataset.bind, element]),
);
const canvas = document.querySelector("#viewport");
const canvasShell = document.querySelector(".canvas-shell");
const fixture = document.querySelector("#fixture");

const state = {
  wasmInitialized: false,
  workerReady: false,
  workerHeartbeat: 0,
  runtimeOwner: "starting",
  protocolVersion: null,
  renderBinarySchemaVersion: null,
  webgpuAvailable: false,
  rendererCapabilities: null,
  rendererMetrics: null,
  response: null,
  lastEngineResponse: null,
  lastError: null,
  lastHit: null,
  consoleErrors: 0,
  workerRestarts: 0,
  workerGeneration: 0,
  lastEngineSequence: 0,
  gpuFrameSequence: 0,
  responseGpuSequenceMatch: true,
  staleFramesDiscarded: 0,
  cameraRawIntents: 0,
  cameraRequestsSent: 0,
  cameraRequestsCoalesced: 0,
  cameraRequestInFlight: false,
};

let worker;
let renderer;
let frameQueue = Promise.resolve();
const responseWaiters = new Map();
let dragging = false;
let movedDuringDrag = false;
let lastPointer = null;
let moveToggle = false;
let cameraAnimationFrame = null;
let pendingPanX = 0;
let pendingPanY = 0;
let pendingZoomFactor = 1;
let pendingZoomPoint = null;
let proofFrameDelays = [];

const rendererReady = WebGpuRenderer.create(canvas)
  .then((created) => {
    renderer = created;
    state.webgpuAvailable = true;
    state.rendererCapabilities = renderer.capabilities();
    updateHud();
    return renderer;
  })
  .catch((error) => {
    state.lastError = typedError(error, "renderer_initialization_failed");
    state.webgpuAvailable = false;
    updateHud();
    throw error;
  });

function typedError(error, fallbackCode) {
  return {
    code: error?.code || fallbackCode,
    message: error instanceof Error ? error.message : String(error),
  };
}

function typedException(code, message) {
  const error = new Error(message);
  error.code = code;
  return error;
}

function send(type, payload = {}) {
  if (!worker || !state.workerReady) return null;
  const request = createRequest(type, payload);
  worker.postMessage(request);
  return request.request_id;
}

function requestAndWait(type, payload = {}, timeoutMs = 15000) {
  const requestId = send(type, payload);
  if (!requestId) {
    return Promise.reject(typedException("worker_not_ready", "Worker is not ready"));
  }
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      responseWaiters.delete(requestId);
      reject(typedException("worker_response_timeout", "Timed out waiting for " + requestId));
    }, timeoutMs);
    responseWaiters.set(requestId, {
      resolve(value) {
        clearTimeout(timeout);
        resolve(value);
      },
      reject(error) {
        clearTimeout(timeout);
        reject(error);
      },
    });
  });
}

function rejectPendingWaiters(error) {
  for (const waiter of responseWaiters.values()) waiter.reject(error);
  responseWaiters.clear();
}

function restartWorker() {
  const restartError = typedException(
    "worker_restarted",
    "Worker restarted before the pending request completed",
  );
  rejectPendingWaiters(restartError);
  worker?.terminate();
  state.workerGeneration += 1;
  const generation = state.workerGeneration;
  state.workerReady = false;
  state.wasmInitialized = false;
  state.runtimeOwner = "starting";
  state.workerHeartbeat = 0;
  state.lastEngineSequence = 0;
  state.gpuFrameSequence = 0;
  state.responseGpuSequenceMatch = true;
  state.response = null;
  state.rendererMetrics = null;
  state.workerRestarts += 1;
  worker = new Worker("./src/worker.js", { type: "module", name: "phase0d-engine-host" });
  worker.addEventListener("message", (event) => enqueueWorkerMessage(event.data, generation));
  worker.addEventListener("error", (event) => {
    if (generation !== state.workerGeneration) return;
    state.lastError = { code: "worker_error", message: event.message };
    updateHud();
  });
  updateHud();
}

function enqueueWorkerMessage(message, generation) {
  frameQueue = frameQueue
    .then(() => processWorkerMessage(message, generation))
    .catch((error) => {
      if (generation !== state.workerGeneration) return;
      state.lastError = typedError(error, "frame_queue_failed");
      updateHud();
      const requestId = message?.response?.request_id ?? message?.request_id;
      const waiter = responseWaiters.get(requestId);
      if (waiter) {
        responseWaiters.delete(requestId);
        waiter.reject(error);
      }
    });
}

async function processWorkerMessage(message, generation) {
  if (generation !== state.workerGeneration) {
    state.staleFramesDiscarded += 1;
    return;
  }
  if (message.type === "worker_ready") {
    state.workerReady = true;
    state.wasmInitialized = message.wasm_initialized;
    state.runtimeOwner = message.runtime_owner;
    state.protocolVersion = message.protocol_version;
    state.renderBinarySchemaVersion = message.render_binary_schema_version;
    updateHud();
    try {
      await rendererReady;
      send("initialize");
      sendResize();
    } catch {
      // The typed renderer error is already visible. Gate 0D-R1 has no fallback.
    }
    return;
  }
  if (message.type === "worker_boot_error" || message.type === "worker_request_error") {
    state.lastError = message.error;
    if (message.type === "worker_request_error") {
      const waiter = responseWaiters.get(message.request_id);
      if (waiter) {
        responseWaiters.delete(message.request_id);
        const error = typedException(message.error.code, message.error.message);
        waiter.reject(error);
      }
    }
    updateHud();
    return;
  }
  if (message.type !== "engine") return;

  const response = assertEngineResponse(message.response);
  if (response.engine_sequence <= state.lastEngineSequence) {
    throw typedException(
      "engine_sequence_regression",
      "Engine sequence " + response.engine_sequence + " is not newer than " + state.lastEngineSequence,
    );
  }
  state.lastEngineSequence = response.engine_sequence;
  state.lastEngineResponse = response;
  state.runtimeOwner = response.runtime_owner;
  state.protocolVersion = response.protocol_version;
  state.renderBinarySchemaVersion = response.render_binary_schema_version;

  if (response.result?.heartbeat === true) {
    const parsed = Number(response.request_id.replace(/^heartbeat-/, ""));
    state.workerHeartbeat = Number.isFinite(parsed)
      ? Math.max(state.workerHeartbeat, parsed)
      : state.workerHeartbeat + 1;
    finishResponseWaiter(response);
    updateHud();
    return;
  }

  if (!response.ok) {
    state.lastError = response.error;
    finishResponseWaiter(response);
    updateHud();
    return;
  }
  if (response.result?.topmost !== undefined) {
    state.lastHit = response.result.topmost;
  }

  const hasFrame = Object.keys(message.payload ?? {}).length > 0;
  if (!hasFrame) {
    state.lastError = renderer?.lastError ?? null;
    finishResponseWaiter(response);
    updateHud();
    return;
  }
  if (
    response.revisions.document !== response.revisions.scene ||
    response.revisions.scene !== response.revisions.render
  ) {
    throw typedException("revision_frame_mismatch", "Document, Scene, and Render revisions differ");
  }
  if (response.engine_sequence <= state.gpuFrameSequence) {
    throw typedException("stale_gpu_frame", "A stale engine frame attempted to reach the GPU");
  }
  if (!renderer) {
    throw typedException("renderer_unavailable", "WebGPU renderer is unavailable");
  }

  const proofDelay = proofFrameDelays.shift() ?? 0;
  if (proofDelay > 0) {
    await new Promise((resolve) => setTimeout(resolve, proofDelay));
  }
  const metrics = await renderer.applyEngineFrame(response, message.payload);
  if (generation !== state.workerGeneration) {
    state.staleFramesDiscarded += 1;
    return;
  }
  state.response = response;
  state.rendererMetrics = metrics;
  state.gpuFrameSequence = response.engine_sequence;
  state.responseGpuSequenceMatch =
    state.response.engine_sequence === state.gpuFrameSequence &&
    metrics.frame_sequence === state.gpuFrameSequence;
  if (!state.responseGpuSequenceMatch) {
    throw typedException("response_gpu_sequence_mismatch", "GPU metrics do not match the engine frame");
  }
  state.lastError = renderer.lastError ?? null;
  finishResponseWaiter(response);
  updateHud();
}

function finishResponseWaiter(response) {
  const waiter = responseWaiters.get(response.request_id);
  if (!waiter) return;
  responseWaiters.delete(response.request_id);
  waiter.resolve({ response, proof: proofSnapshot() });
}

function sendResize() {
  if (!state.workerReady || !renderer) return;
  const bounds = canvasShell.getBoundingClientRect();
  const width = Math.max(1, bounds.width);
  const height = Math.max(1, bounds.height);
  const dpr = Math.max(1, window.devicePixelRatio || 1);
  renderer.resize(width, height, dpr);
  send("camera", { camera: { kind: "resize", width, height, dpr } });
}

function queuePan(dx, dy) {
  state.cameraRawIntents += 1;
  if (pendingPanX !== 0 || pendingPanY !== 0 || state.cameraRequestInFlight || cameraAnimationFrame) {
    state.cameraRequestsCoalesced += 1;
  }
  pendingPanX += dx;
  pendingPanY += dy;
  scheduleCameraFlush();
}

function queueZoom(deltaY, x, y) {
  state.cameraRawIntents += 1;
  if (pendingZoomPoint || state.cameraRequestInFlight || cameraAnimationFrame) {
    state.cameraRequestsCoalesced += 1;
  }
  pendingZoomFactor *= Math.exp(-deltaY * 0.0015);
  pendingZoomPoint = { x, y };
  scheduleCameraFlush();
}

function scheduleCameraFlush() {
  if (state.cameraRequestInFlight || cameraAnimationFrame !== null) return;
  cameraAnimationFrame = requestAnimationFrame(() => {
    cameraAnimationFrame = null;
    void flushCameraIntent();
  });
}

async function flushCameraIntent() {
  if (state.cameraRequestInFlight || !state.workerReady) return;
  let camera;
  if (pendingPanX !== 0 || pendingPanY !== 0) {
    camera = { kind: "pan", dx: pendingPanX, dy: pendingPanY };
    pendingPanX = 0;
    pendingPanY = 0;
  } else if (pendingZoomPoint) {
    const zoom = Math.min(
      1000,
      Math.max(0.000001, (state.response?.camera.zoom ?? 1) * pendingZoomFactor),
    );
    camera = { kind: "zoom", x: pendingZoomPoint.x, y: pendingZoomPoint.y, zoom };
    pendingZoomFactor = 1;
    pendingZoomPoint = null;
  } else {
    return;
  }

  state.cameraRequestInFlight = true;
  state.cameraRequestsSent += 1;
  try {
    await requestAndWait("camera", { camera }, 30000);
  } catch (error) {
    if (error?.code !== "worker_restarted") {
      state.lastError = typedError(error, "camera_request_failed");
    }
  } finally {
    state.cameraRequestInFlight = false;
    if (
      pendingPanX !== 0 ||
      pendingPanY !== 0 ||
      pendingZoomPoint
    ) {
      scheduleCameraFlush();
    }
    updateHud();
  }
}

async function waitForIdle() {
  while (
    state.cameraRequestInFlight ||
    cameraAnimationFrame !== null ||
    pendingPanX !== 0 ||
    pendingPanY !== 0 ||
    pendingZoomPoint
  ) {
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  await frameQueue;
  return proofSnapshot();
}

function updateHud() {
  const response = state.response;
  const gpu = state.rendererCapabilities;
  const metrics = state.rendererMetrics;
  set("wasm", state.wasmInitialized ? "initialized" : "not initialized");
  set(
    "owner",
    state.runtimeOwner === "dedicated-worker"
      ? "Worker core / main-thread WebGPU"
      : state.runtimeOwner,
  );
  set("heartbeat", state.workerHeartbeat ? "#" + state.workerHeartbeat : "waiting");
  set("protocol", (state.protocolVersion ?? PROTOCOL_VERSION) + " / render " + (state.renderBinarySchemaVersion ?? "?"));
  set("webgpu", state.webgpuAvailable ? "available" : "unavailable");
  set("adapter", gpu ? gpu.adapter + " / " + gpu.vendor : "pending");
  set("device", gpu?.device ?? "pending");
  set("backend", gpu?.backend ?? "pending");
  set(
    "revisions",
    response
      ? response.revisions.document + " / " + response.revisions.scene + " / " + response.revisions.render
      : "- / - / -",
  );
  set(
    "fixtureName",
    response?.fixture ?? fixture.value.toUpperCase(),
  );
  set(
    "nodes",
    response
      ? response.culling.total + " / " + response.culling.visible + " / " + response.culling.culled
      : "- / - / -",
  );
  set("candidates", response?.culling.spatial_candidates ?? "-");
  set("drawCalls", metrics?.draw_calls ?? "-");
  set("batches", metrics?.batches ?? "-");
  set("submitted", metrics?.submitted_instances ?? response?.culling.submitted_instances ?? "-");
  set("buffers", metrics ? metrics.buffer_count + " / " + formatBytes(metrics.buffer_bytes) : "-");
  set("upload", metrics ? metrics.upload_calls + " / " + formatBytes(metrics.frame_upload_bytes) : "-");
  set(
    "timings",
    response && metrics
      ? response.metrics.cpu_update_ms.toFixed(2) + " / " +
        metrics.render_submit_ms.toFixed(2) + " / " + metrics.frame_ms.toFixed(2) + " ms"
      : "-",
  );
  set("fallbacks", response?.metrics.fallback_rebuild_count ?? "-");
  set("validation", metrics?.validation_errors ?? "-");
  set("hit", state.lastHit ?? "none");
  set("error", state.lastError ? state.lastError.code + ": " + state.lastError.message : "none");
  document.body.dataset.ready = String(
    state.wasmInitialized &&
      state.workerReady &&
      state.webgpuAvailable &&
      Boolean(response?.ok) &&
      state.responseGpuSequenceMatch,
  );
  window.__PHASE0D_PROOF__ = proofSnapshot();
}

function proofSnapshot() {
  const response = state.response;
  const metrics = state.rendererMetrics;
  return {
    protocol_version: state.protocolVersion,
    render_binary_schema_version: state.renderBinarySchemaVersion,
    wasm_initialized: state.wasmInitialized,
    worker_runtime_owner: state.runtimeOwner,
    worker_heartbeat: state.workerHeartbeat,
    main_thread_document_mutation_api: false,
    actual_webgpu: state.webgpuAvailable,
    adapter: state.rendererCapabilities?.adapter ?? null,
    vendor: state.rendererCapabilities?.vendor ?? null,
    architecture: state.rendererCapabilities?.architecture ?? null,
    device: state.rendererCapabilities?.device ?? null,
    backend: state.rendererCapabilities?.backend ?? null,
    fixture: response?.fixture ?? null,
    revisions: response?.revisions ?? null,
    culling: response?.culling ?? null,
    render_delta: response?.render_delta ?? null,
    render_encoding: response?.render_encoding ?? null,
    work_counters: response?.metrics ?? null,
    gpu: metrics,
    engine_sequence: response?.engine_sequence ?? null,
    latest_engine_sequence: state.lastEngineSequence,
    gpu_frame_sequence: state.gpuFrameSequence,
    response_gpu_sequence_match: state.responseGpuSequenceMatch,
    hit_test_topmost: state.lastHit,
    fallback_rebuild_count: response?.metrics.fallback_rebuild_count ?? null,
    gpu_validation_errors: metrics?.validation_errors ?? null,
    last_typed_error: state.lastError,
    console_errors: state.consoleErrors,
    worker_restarts: state.workerRestarts,
    worker_generation: state.workerGeneration,
    stale_frames_discarded: state.staleFramesDiscarded,
    camera_raw_intents: state.cameraRawIntents,
    camera_requests_sent: state.cameraRequestsSent,
    camera_requests_coalesced: state.cameraRequestsCoalesced,
    camera_requests_dropped: Math.max(0, state.cameraRawIntents - state.cameraRequestsSent),
    camera_request_in_flight: state.cameraRequestInFlight,
  };
}

function set(name, value) {
  if (elements[name]) elements[name].textContent = String(value);
}

function formatBytes(value) {
  if (value < 1024) return value + " B";
  if (value < 1024 * 1024) return (value / 1024).toFixed(1) + " KiB";
  return (value / (1024 * 1024)).toFixed(1) + " MiB";
}

fixture.addEventListener("change", () => send("load_fixture", { fixture: fixture.value }));
document.querySelector("#move-node").addEventListener("click", () => {
  moveToggle = !moveToggle;
  send("command", {
    command: {
      kind: "move_node",
      node_index: 1,
      x: moveToggle ? 120 : -160,
      y: moveToggle ? 60 : -80,
    },
  });
});
document.querySelector("#undo").addEventListener("click", () => send("undo"));
document.querySelector("#redo").addEventListener("click", () => send("redo"));
document
  .querySelector("#reset-view")
  .addEventListener("click", () => send("camera", { camera: { kind: "reset" } }));
document
  .querySelector("#fit-view")
  .addEventListener("click", () => send("camera", { camera: { kind: "fit" } }));
document.querySelector("#restart-worker").addEventListener("click", restartWorker);

canvas.addEventListener("pointerdown", (event) => {
  dragging = true;
  movedDuringDrag = false;
  lastPointer = { x: event.clientX, y: event.clientY };
  canvas.setPointerCapture(event.pointerId);
});
canvas.addEventListener("pointermove", (event) => {
  if (!dragging || !lastPointer) return;
  const dx = event.clientX - lastPointer.x;
  const dy = event.clientY - lastPointer.y;
  if (Math.abs(dx) + Math.abs(dy) > 0) {
    movedDuringDrag = true;
    queuePan(dx, dy);
  }
  lastPointer = { x: event.clientX, y: event.clientY };
});
canvas.addEventListener("pointerup", (event) => {
  if (!movedDuringDrag) {
    const bounds = canvas.getBoundingClientRect();
    send("hit_test", {
      x: event.clientX - bounds.left,
      y: event.clientY - bounds.top,
    });
  }
  dragging = false;
  lastPointer = null;
  canvas.releasePointerCapture(event.pointerId);
});
canvas.addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    const bounds = canvas.getBoundingClientRect();
    queueZoom(event.deltaY, event.clientX - bounds.left, event.clientY - bounds.top);
  },
  { passive: false },
);

window.addEventListener("phase0d-renderer-error", (event) => {
  state.lastError = event.detail;
  updateHud();
});
window.addEventListener("error", () => {
  state.consoleErrors += 1;
  updateHud();
});
window.addEventListener("unhandledrejection", () => {
  state.consoleErrors += 1;
  updateHud();
});
new ResizeObserver(() => sendResize()).observe(canvasShell);
window.__phase0dRequest = send;
window.__phase0dRequestAndWait = requestAndWait;
window.__phase0dWaitForIdle = waitForIdle;
window.__phase0dReadPixel = (x, y) => renderer.readPixel(x, y);
window.__phase0dSetFrameDelays = (delays) => {
  proofFrameDelays = Array.isArray(delays) ? delays.map(Number) : [];
};
window.__phase0dState = state;
restartWorker();
updateHud();
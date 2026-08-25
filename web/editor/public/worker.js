import init, { EngineHost } from "/pkg/engine_host.js";

const PROTOCOL_VERSION = 1;
let host;
let heartbeat = 0;

function transferResponse(response) {
  const payload = {};
  const transfer = [];
  if (response.binary?.full_instances) {
    const data = host.takeFullInstances();
    payload.fullInstances = data.buffer;
    transfer.push(data.buffer);
  }
  if (response.binary?.dirty_instances) {
    const data = host.takeDirtyInstances();
    payload.dirtyInstances = data.buffer;
    transfer.push(data.buffer);
  }
  if (response.binary?.removed_slots) {
    const data = host.takeRemovedSlots();
    payload.removedSlots = data.buffer;
    transfer.push(data.buffer);
  }
  if (response.binary?.visible_slots) {
    const data = host.takeVisibleSlots();
    payload.visibleSlots = data.buffer;
    transfer.push(data.buffer);
  }
  if (response.binary?.path_instances) {
    const data = host.takePathInstances();
    payload.pathInstances = data.buffer;
    transfer.push(data.buffer);
  }
  if (response.binary?.path_vertices) {
    const data = host.takePathVertices();
    payload.pathVertices = data.buffer;
    transfer.push(data.buffer);
  }
  self.postMessage({ type: "engine", response, payload }, transfer);
}

function handleRequest(request) {
  transferResponse(JSON.parse(host.handleJson(JSON.stringify(request))));
}

const ready = (async () => {
  await init();
  host = new EngineHost();
  self.postMessage({
    type: "worker_ready",
    wasm_initialized: true,
    runtime_owner: "dedicated-worker",
    protocol_version: EngineHost.protocolVersion(),
    render_binary_schema_version: EngineHost.renderBinarySchemaVersion(),
  });
  setInterval(() => {
    heartbeat += 1;
    handleRequest({
      protocol_version: PROTOCOL_VERSION,
      request_id: `heartbeat-${heartbeat}`,
      type: "heartbeat",
    });
  }, 1000);
})().catch((error) => {
  self.postMessage({
    type: "worker_boot_error",
    error: {
      code: "wasm_initialization_failed",
      message: error instanceof Error ? error.message : String(error),
    },
  });
  throw error;
});

self.addEventListener("message", (event) => {
  ready.then(() => handleRequest(event.data)).catch((error) => {
    self.postMessage({
      type: "worker_request_error",
      request_id: event.data?.request_id ?? "unknown",
      error: {
        code: "worker_request_failed",
        message: error instanceof Error ? error.message : String(error),
      },
    });
  });
});

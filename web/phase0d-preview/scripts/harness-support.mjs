export class HarnessFailure extends Error {
  constructor(code, message, details = {}) {
    super(message);
    this.name = "HarnessFailure";
    this.code = code;
    this.details = details;
  }
}

export const EXPECTED_PREVIEW_ASSETS = new Map([
  ["/index.html", "text/html"],
  ["/src/worker.js", "text/javascript"],
  ["/pkg/engine_host.js", "text/javascript"],
  ["/pkg/engine_host_bg.wasm", "application/wasm"],
]);

export function validateAssetResponse(pathname, expectedMime, response) {
  const details = {
    status: response.status,
    content_type: response.contentType,
  };
  if (!response.ok) {
    throw new HarnessFailure(
      "preview_asset_http_error",
      pathname + " returned HTTP " + response.status,
      details,
    );
  }
  if (!response.contentType.startsWith(expectedMime)) {
    throw new HarnessFailure(
      "preview_asset_mime_error",
      pathname + " returned " + response.contentType + " instead of " + expectedMime,
      details,
    );
  }
  return details;
}

export function classifyApplicationFailure(error) {
  const code = error?.code ?? "application_failed";
  if (code === "wasm_initialization_failed") {
    return new HarnessFailure("wasm_boot_failed", error.message, { cause_code: code });
  }
  if (code === "worker_error" || code === "worker_request_failed") {
    return new HarnessFailure("worker_failed", error.message, { cause_code: code });
  }
  if (
    code === "webgpu_unavailable" ||
    code === "adapter_unavailable" ||
    code === "surface_unavailable" ||
    code === "renderer_initialization_failed"
  ) {
    return new HarnessFailure("webgpu_initialization_failed", error.message, {
      cause_code: code,
    });
  }
  return new HarnessFailure("application_failed", error?.message ?? String(error), {
    cause_code: code,
  });
}
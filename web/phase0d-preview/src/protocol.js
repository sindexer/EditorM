import { RENDER_BINARY_SCHEMA_VERSION } from "./render_contract.js";

export const PROTOCOL_VERSION = 1;

export class ProtocolFailure extends TypeError {
  constructor(code, message) {
    super(message);
    this.name = "ProtocolFailure";
    this.code = code;
  }
}

let sequence = 0;

export function createRequest(type, payload = {}) {
  sequence += 1;
  return {
    protocol_version: PROTOCOL_VERSION,
    request_id: type + "-" + Date.now() + "-" + sequence,
    type,
    ...payload,
  };
}

export function assertEngineResponse(response) {
  if (!response || response.type !== "engine_response") {
    throw new ProtocolFailure(
      "invalid_engine_response",
      "Worker returned a non-EngineHost response",
    );
  }
  if (response.protocol_version !== PROTOCOL_VERSION) {
    throw new ProtocolFailure(
      "protocol_version_mismatch",
      "Protocol mismatch: received " + response.protocol_version +
        ", expected " + PROTOCOL_VERSION,
    );
  }
  if (response.render_binary_schema_version !== RENDER_BINARY_SCHEMA_VERSION) {
    throw new ProtocolFailure(
      "render_schema_version_mismatch",
      "Render schema mismatch: received " + response.render_binary_schema_version +
        ", expected " + RENDER_BINARY_SCHEMA_VERSION,
    );
  }
  if (!Number.isSafeInteger(response.engine_sequence) || response.engine_sequence <= 0) {
    throw new ProtocolFailure(
      "invalid_engine_sequence",
      "EngineHost response has no valid engine sequence",
    );
  }
  if (typeof response.request_id !== "string") {
    throw new ProtocolFailure(
      "missing_request_id",
      "EngineHost response is missing its correlation ID",
    );
  }
  return response;
}

export function splitF64(value) {
  if (!Number.isFinite(value)) {
    throw new TypeError("Camera coordinate must be finite");
  }
  const high = Math.fround(value);
  const low = Math.fround(value - high);
  if (!Number.isFinite(high) || !Number.isFinite(low)) {
    throw new TypeError("Camera coordinate cannot be represented by high/low f32");
  }
  return [high, low];
}
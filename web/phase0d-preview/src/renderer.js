import { splitF64 } from "./protocol.js";
import {
  DIRTY_STRIDE,
  INSTANCE_STRIDE,
  PATH_INSTANCE_STRIDE,
  PATH_VERTEX_STRIDE,
  RENDER_BINARY_SCHEMA_VERSION,
  SHADER_SOURCE,
} from "./render_contract.js";



const GPU = globalThis.GPUBufferUsage ?? {};
const TEXTURE_USAGE = globalThis.GPUTextureUsage ?? {};
const MAP_MODE = globalThis.GPUMapMode ?? {};

export class RendererFailure extends Error {
  constructor(code, message) {
    super(message);
    this.name = "RendererFailure";
    this.code = code;
  }
}

export function srgbViewFormat(baseFormat) {
  if (baseFormat === "bgra8unorm") return "bgra8unorm-srgb";
  if (baseFormat === "rgba8unorm") return "rgba8unorm-srgb";
  throw new RendererFailure(
    "srgb_view_format_unavailable",
    `Preferred canvas format ${baseFormat} has no supported sRGB view format`,
  );
}

export class WebGpuRenderer {
  static async create(canvas) {
    if (!globalThis.navigator?.gpu) {
      throw new RendererFailure(
        "webgpu_unavailable",
        "WebGPU is unavailable. Gate 0D cannot pass and no Canvas2D fallback is used.",
      );
    }
    const adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
    if (!adapter) {
      throw new RendererFailure("adapter_unavailable", "No WebGPU adapter was returned");
    }
    const device = await adapter.requestDevice({ label: "Phase 0D WebGPU device" });
    const renderer = new WebGpuRenderer(canvas, adapter, device);
    await renderer.pipelineReady;
    return renderer;
  }

  constructor(canvas, adapter, device) {
    this.canvas = canvas;
    this.adapter = adapter;
    this.device = device;
    this.queue = device.queue;
    this.context = canvas.getContext("webgpu");
    if (!this.context) {
      throw new RendererFailure("surface_unavailable", "Canvas WebGPU context creation failed");
    }
    this.surfaceBaseFormat = navigator.gpu.getPreferredCanvasFormat();
    this.pipelineViewFormat = srgbViewFormat(this.surfaceBaseFormat);
    this.readbackViewFormat = this.pipelineViewFormat;
    this.instanceBuffer = null;
    this.visibleBuffer = null;
    this.pathInstanceBuffer = null;
    this.pathVertexBuffer = null;
    this.instanceBufferBytes = 0;
    this.visibleBufferBytes = 0;
    this.pathInstanceBufferBytes = 0;
    this.pathVertexBufferBytes = 0;
    this.visibleCount = 0;
    // Ordered pipeline switches for the current frame, bottom to top.
    this.drawBatches = [];
    this.lastDpr = 1;
    this.validationErrors = 0;
    this.lastError = null;
    this.metrics = {
      draw_calls: 0,
      batches: 0,
      submitted_instances: 0,
      buffer_count: 5,
      buffer_bytes: 0,
      upload_calls: 0,
      frame_upload_bytes: 0,
      render_submit_ms: 0,
      frame_ms: 0,
      validation_errors: 0,
      frame_sequence: 0,
      gpu_encode_attempted: 0,
      gpu_encode_omitted: 0,
      instance_upload_bytes: 0,
      visible_slot_upload_bytes: 0,
      allocation_growth_count: 0,
      path_instance_upload_bytes: 0,
      path_vertex_upload_bytes: 0,
      path_vertices_uploaded: 0,
    };
    this.adapterInfo = adapter.info ?? {};
    this.deviceLabel = device.label || "Phase 0D WebGPU device";
    this.backend = "browser-webgpu (implementation backend not exposed by Web API)";
    this.device.lost.then((info) => {
      this.lastError = {
        code: "device_lost",
        message: `${info.reason}: ${info.message}`,
      };
      globalThis.dispatchEvent?.(new CustomEvent("phase0d-renderer-error", { detail: this.lastError }));
    });
    this.device.addEventListener("uncapturederror", (event) => {
      this.validationErrors += 1;
      this.metrics.validation_errors = this.validationErrors;
      this.lastError = { code: "gpu_validation_error", message: event.error.message };
    });
    this.pipelineReady = this.initializePipeline();
  }

  async initializePipeline() {
    this.device.pushErrorScope("validation");
    const shader = this.device.createShaderModule({
      label: "Phase 0D geometry-aware shader",
      code: SHADER_SOURCE,
    });
    const compilation = await shader.getCompilationInfo();
    const compilationErrors = compilation.messages.filter((message) => message.type === "error");
    if (compilationErrors.length > 0) {
      await this.device.popErrorScope();
      throw new RendererFailure(
        "shader_compilation_failed",
        compilationErrors.map((message) => message.lineNum + ":" + message.linePos + " " + message.message).join("\\n"),
      );
    }
    this.bindGroupLayout = this.device.createBindGroupLayout({
      label: "Phase 0D bind group layout",
      entries: [
        {
          binding: 0,
          visibility: GPUShaderStage.VERTEX,
          buffer: { type: "read-only-storage" },
        },
        {
          binding: 1,
          visibility: GPUShaderStage.VERTEX,
          buffer: { type: "read-only-storage" },
        },
        {
          binding: 2,
          visibility: GPUShaderStage.VERTEX,
          buffer: { type: "uniform" },
        },
        {
          binding: 3,
          visibility: GPUShaderStage.VERTEX,
          buffer: { type: "read-only-storage" },
        },
        {
          binding: 4,
          visibility: GPUShaderStage.VERTEX,
          buffer: { type: "read-only-storage" },
        },
      ],
    });
    const layout = this.device.createPipelineLayout({ bindGroupLayouts: [this.bindGroupLayout] });
    this.pipeline = this.device.createRenderPipeline({
      label: "Phase 0D instanced rectangle and ellipse pipeline",
      layout,
      vertex: { module: shader, entryPoint: "vs_main" },
      fragment: {
        module: shader,
        entryPoint: "fs_main",
        targets: [
          {
            format: this.pipelineViewFormat,
            blend: {
              color: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
              alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
            },
          },
        ],
      },
      primitive: { topology: "triangle-list" },
    });
    // Paths arrive as pre-tessellated triangles from Rust; this pipeline only transforms them
    // and applies the one-pixel screen-space feather.
    this.pathPipeline = this.device.createRenderPipeline({
      label: "Phase 2A tessellated path pipeline",
      layout,
      vertex: { module: shader, entryPoint: "vs_path" },
      fragment: {
        module: shader,
        entryPoint: "fs_path",
        targets: [
          {
            format: this.pipelineViewFormat,
            blend: {
              color: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
              alpha: { srcFactor: "one", dstFactor: "one-minus-src-alpha", operation: "add" },
            },
          },
        ],
      },
      primitive: { topology: "triangle-list" },
    });
    this.viewBuffer = this.device.createBuffer({
      label: "Phase 0D view uniform",
      size: 32,
      usage: GPU.UNIFORM | GPU.COPY_DST,
    });
    this.ensureInstanceBuffer(INSTANCE_STRIDE);
    this.ensureVisibleBuffer(4);
    this.ensurePathInstanceBuffer(PATH_INSTANCE_STRIDE);
    this.ensurePathVertexBuffer(PATH_VERTEX_STRIDE);
    this.configureSurface();
    const pipelineError = await this.device.popErrorScope();
    if (pipelineError) {
      this.validationErrors += 1;
      this.metrics.validation_errors = this.validationErrors;
      this.lastError = { code: "pipeline_validation_error", message: pipelineError.message };
      throw new RendererFailure("pipeline_validation_error", pipelineError.message);
    }
  }

  capabilities() {
    return {
      available: true,
      adapter: this.adapterInfo.description || this.adapterInfo.architecture || "WebGPU adapter (privacy-redacted)",
      vendor: this.adapterInfo.vendor || "privacy-redacted",
      architecture: this.adapterInfo.architecture || "privacy-redacted",
      device: this.deviceLabel,
      backend: this.backend,
      format: this.surfaceBaseFormat,
      surface_base_format: this.surfaceBaseFormat,
      pipeline_view_format: this.pipelineViewFormat,
      readback_view_format: this.readbackViewFormat,
    };
  }

  configureSurface() {
    try {
      this.context.configure({
        device: this.device,
        format: this.surfaceBaseFormat,
        viewFormats: [this.pipelineViewFormat],
        alphaMode: "opaque",
      });
    } catch (error) {
      throw new RendererFailure(
        "surface_configuration_failed",
        error instanceof Error ? error.message : String(error),
      );
    }
  }

  resize(width, height, dpr) {
    this.lastDpr = dpr;
    const physicalWidth = Math.max(1, Math.round(width * dpr));
    const physicalHeight = Math.max(1, Math.round(height * dpr));
    if (this.canvas.width !== physicalWidth || this.canvas.height !== physicalHeight) {
      this.canvas.width = physicalWidth;
      this.canvas.height = physicalHeight;
      this.configureSurface();
    }
  }

  ensureInstanceBuffer(byteLength) {
    const required = Math.max(INSTANCE_STRIDE, Math.ceil(byteLength / 4) * 4);
    if (this.instanceBuffer && this.instanceBufferBytes >= required) return;
    const previous = this.instanceBuffer;
    const previousBytes = this.instanceBufferBytes;
    const next = this.device.createBuffer({
      label: "Phase 0D instance storage",
      size: required,
      usage: GPU.STORAGE | GPU.COPY_DST | GPU.COPY_SRC,
    });
    if (previous && previousBytes > 0) {
      const encoder = this.device.createCommandEncoder({ label: "Phase 0D buffer growth copy" });
      encoder.copyBufferToBuffer(previous, 0, next, 0, Math.min(previousBytes, required));
      this.queue.submit([encoder.finish()]);
      previous.destroy();
    }
    this.instanceBuffer = next;
    this.instanceBufferBytes = required;
    this.metrics.allocation_growth_count += 1;
    this.rebuildBindGroup();
  }

  ensureVisibleBuffer(byteLength) {
    const required = Math.max(4, Math.ceil(byteLength / 4) * 4);
    if (this.visibleBuffer && this.visibleBufferBytes >= required) return;
    this.visibleBuffer?.destroy();
    this.metrics.allocation_growth_count += 1;
    this.visibleBuffer = this.device.createBuffer({
      label: "Phase 0D visible slot storage",
      size: required,
      usage: GPU.STORAGE | GPU.COPY_DST,
    });
    this.visibleBufferBytes = required;
    this.rebuildBindGroup();
  }

  ensurePathInstanceBuffer(byteLength) {
    const required = Math.max(PATH_INSTANCE_STRIDE, Math.ceil(byteLength / 4) * 4);
    if (this.pathInstanceBuffer && this.pathInstanceBufferBytes >= required) return;
    this.pathInstanceBuffer?.destroy();
    this.metrics.allocation_growth_count += 1;
    this.pathInstanceBuffer = this.device.createBuffer({
      label: "Phase 2A path instance storage",
      size: required,
      usage: GPU.STORAGE | GPU.COPY_DST,
    });
    this.pathInstanceBufferBytes = required;
    this.rebuildBindGroup();
  }

  ensurePathVertexBuffer(byteLength) {
    const required = Math.max(PATH_VERTEX_STRIDE, Math.ceil(byteLength / 4) * 4);
    if (this.pathVertexBuffer && this.pathVertexBufferBytes >= required) return;
    this.pathVertexBuffer?.destroy();
    this.metrics.allocation_growth_count += 1;
    this.pathVertexBuffer = this.device.createBuffer({
      label: "Phase 2A path vertex storage",
      size: required,
      usage: GPU.STORAGE | GPU.COPY_DST,
    });
    this.pathVertexBufferBytes = required;
    this.rebuildBindGroup();
  }

  rebuildBindGroup() {
    if (
      !this.instanceBuffer ||
      !this.visibleBuffer ||
      !this.viewBuffer ||
      !this.pathInstanceBuffer ||
      !this.pathVertexBuffer ||
      !this.bindGroupLayout
    ) {
      return;
    }
    this.bindGroup = this.device.createBindGroup({
      label: "Phase 0D render bind group",
      layout: this.bindGroupLayout,
      entries: [
        { binding: 0, resource: { buffer: this.instanceBuffer } },
        { binding: 1, resource: { buffer: this.visibleBuffer } },
        { binding: 2, resource: { buffer: this.viewBuffer } },
        { binding: 3, resource: { buffer: this.pathInstanceBuffer } },
        { binding: 4, resource: { buffer: this.pathVertexBuffer } },
      ],
    });
  }

  // Records the frame's draw calls in scene order, switching pipelines only where the batch
  // list says the kind changes.
  encodeDrawBatches(pass) {
    let drawCalls = 0;
    pass.setBindGroup(0, this.bindGroup);
    for (const batch of this.drawBatches) {
      if (batch.kind === "primitives") {
        if (batch.count === 0) continue;
        pass.setPipeline(this.pipeline);
        pass.draw(6, batch.count, 0, batch.first_visible);
        drawCalls += 1;
      } else if (batch.kind === "paths") {
        if (batch.vertex_count === 0) continue;
        pass.setPipeline(this.pathPipeline);
        pass.draw(batch.vertex_count, 1, batch.first_vertex, 0);
        drawCalls += 1;
      }
    }
    return drawCalls;
  }

  upload(response, payload) {
    if (response.render_binary_schema_version !== RENDER_BINARY_SCHEMA_VERSION) {
      throw new RendererFailure(
        "render_schema_version_mismatch",
        "Engine response render schema does not match the browser renderer",
      );
    }
    if (
      response.resources.instance_stride_bytes !== INSTANCE_STRIDE ||
      response.resources.dirty_record_stride_bytes !== DIRTY_STRIDE
    ) {
      throw new RendererFailure(
        "render_schema_layout_mismatch",
        "Engine response binary strides do not match the browser renderer",
      );
    }
    let calls = 0;
    let bytes = 0;
    const capacityBytes = Math.max(INSTANCE_STRIDE, response.resources.instance_capacity * INSTANCE_STRIDE);
    this.ensureInstanceBuffer(capacityBytes);

    if (payload.fullInstances) {
      const data = new Uint8Array(payload.fullInstances);
      this.queue.writeBuffer(this.instanceBuffer, 0, data);
      calls += 1;
      bytes += data.byteLength;
    }
    if (payload.dirtyInstances) {
      const data = new Uint8Array(payload.dirtyInstances);
      if (data.byteLength % DIRTY_STRIDE !== 0) {
        throw new RendererFailure("invalid_render_delta", "Dirty instance payload has an invalid stride");
      }
      const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
      for (let offset = 0; offset < data.byteLength; offset += DIRTY_STRIDE) {
        const slot = view.getUint32(offset, true);
        this.queue.writeBuffer(
          this.instanceBuffer,
          slot * INSTANCE_STRIDE,
          new Uint8Array(data.buffer, data.byteOffset + offset + 4, INSTANCE_STRIDE),
        );
        calls += 1;
        bytes += INSTANCE_STRIDE;
      }
    }
    if (payload.removedSlots) {
      const zero = new Uint8Array(INSTANCE_STRIDE);
      for (const slot of new Uint32Array(payload.removedSlots)) {
        this.queue.writeBuffer(this.instanceBuffer, slot * INSTANCE_STRIDE, zero);
        calls += 1;
        bytes += INSTANCE_STRIDE;
      }
    }
    if (payload.visibleSlots) {
      const visible = new Uint32Array(payload.visibleSlots);
      this.ensureVisibleBuffer(Math.max(4, visible.byteLength));
      if (visible.byteLength > 0) {
        this.queue.writeBuffer(this.visibleBuffer, 0, visible);
        calls += 1;
        bytes += visible.byteLength;
      }
      this.visibleCount = visible.length;
    }
    if (payload.pathInstances) {
      const data = new Uint8Array(payload.pathInstances);
      if (data.byteLength % PATH_INSTANCE_STRIDE !== 0) {
        throw new RendererFailure("invalid_path_payload", "Path instance payload has an invalid stride");
      }
      if (data.byteLength > 0) {
        this.ensurePathInstanceBuffer(data.byteLength);
        this.queue.writeBuffer(this.pathInstanceBuffer, 0, data);
        calls += 1;
        bytes += data.byteLength;
        this.metrics.path_instance_upload_bytes = data.byteLength;
      }
    } else {
      this.metrics.path_instance_upload_bytes = 0;
    }
    if (payload.pathVertices) {
      const data = new Uint8Array(payload.pathVertices);
      if (data.byteLength % PATH_VERTEX_STRIDE !== 0) {
        throw new RendererFailure("invalid_path_payload", "Path vertex payload has an invalid stride");
      }
      if (data.byteLength > 0) {
        this.ensurePathVertexBuffer(data.byteLength);
        this.queue.writeBuffer(this.pathVertexBuffer, 0, data);
        calls += 1;
        bytes += data.byteLength;
      }
      this.metrics.path_vertex_upload_bytes = data.byteLength;
      this.metrics.path_vertices_uploaded = data.byteLength / PATH_VERTEX_STRIDE;
    } else {
      // A frame that changed only the camera or a colour re-sends no triangles at all.
      this.metrics.path_vertex_upload_bytes = 0;
      this.metrics.path_vertices_uploaded = 0;
    }
    this.drawBatches = Array.isArray(response.resources?.draw_batches)
      ? response.resources.draw_batches
      : [{ kind: "primitives", first_visible: 0, count: this.visibleCount }];

    const [centerXHigh, centerXLow] = splitF64(response.camera.center[0]);
    const [centerYHigh, centerYLow] = splitF64(response.camera.center[1]);
    const view = new Float32Array([
      centerXHigh,
      centerYHigh,
      centerXLow,
      centerYLow,
      response.camera.viewport[0],
      response.camera.viewport[1],
      response.camera.zoom,
      0,
    ]);
    this.queue.writeBuffer(this.viewBuffer, 0, view);
    calls += 1;
    bytes += view.byteLength;
    this.metrics.upload_calls = calls;
    this.metrics.frame_upload_bytes = bytes;
    this.metrics.gpu_encode_attempted = response.metrics.gpu_encode_attempted;
    this.metrics.gpu_encode_omitted = response.metrics.gpu_encode_omitted;
    this.metrics.instance_upload_bytes = response.metrics.instance_upload_bytes;
    this.metrics.visible_slot_upload_bytes = response.metrics.visible_slot_upload_bytes;
  }

  async render() {
    const started = performance.now();
    this.device.pushErrorScope("validation");
    let textureView;
    try {
      textureView = this.context
        .getCurrentTexture()
        .createView({ format: this.pipelineViewFormat });
    } catch (error) {
      this.configureSurface();
      try {
        textureView = this.context
          .getCurrentTexture()
          .createView({ format: this.pipelineViewFormat });
      } catch (secondError) {
        throw new RendererFailure(
          "surface_lost_or_outdated",
          secondError instanceof Error ? secondError.message : String(secondError),
        );
      }
    }
    const encoder = this.device.createCommandEncoder({ label: "Phase 0D frame encoder" });
    const pass = encoder.beginRenderPass({
      label: "Phase 0D frame pass",
      colorAttachments: [
        {
          view: textureView,
          clearValue: { r: 0.035, g: 0.055, b: 0.08, a: 1 },
          loadOp: "clear",
          storeOp: "store",
        },
      ],
    });
    const drawCalls = this.encodeDrawBatches(pass);
    pass.end();
    this.queue.submit([encoder.finish()]);
    const validation = await this.device.popErrorScope();
    if (validation) {
      this.validationErrors += 1;
      this.metrics.validation_errors = this.validationErrors;
      throw new RendererFailure("gpu_validation_error", validation.message);
    }
    this.metrics.draw_calls = drawCalls;
    this.metrics.batches = drawCalls;
    this.metrics.submitted_instances = this.visibleCount;
    this.metrics.buffer_bytes =
      this.instanceBufferBytes +
      this.visibleBufferBytes +
      this.pathInstanceBufferBytes +
      this.pathVertexBufferBytes +
      32;
    this.metrics.render_submit_ms = performance.now() - started;
    return this.metrics;
  }


  async readPixel(x, y) {
    const physicalX = Math.floor(x * this.lastDpr);
    const physicalY = Math.floor(y * this.lastDpr);
    if (
      !Number.isInteger(physicalX) ||
      !Number.isInteger(physicalY) ||
      physicalX < 0 ||
      physicalY < 0 ||
      physicalX >= this.canvas.width ||
      physicalY >= this.canvas.height
    ) {
      throw new RendererFailure("pixel_out_of_bounds", "Readback pixel is outside the render target");
    }
    const texture = this.device.createTexture({
      label: "Phase 0D-R1 pixel proof target",
      size: {
        width: this.canvas.width,
        height: this.canvas.height,
        depthOrArrayLayers: 1,
      },
      format: this.surfaceBaseFormat,
      viewFormats: [this.readbackViewFormat],
      usage: TEXTURE_USAGE.RENDER_ATTACHMENT | TEXTURE_USAGE.COPY_SRC,
    });
    const readback = this.device.createBuffer({
      label: "Phase 0D-R1 pixel proof readback",
      size: 256,
      usage: GPU.COPY_DST | GPU.MAP_READ,
    });
    this.device.pushErrorScope("validation");
    const encoder = this.device.createCommandEncoder({
      label: "Phase 0D-R1 pixel proof encoder",
    });
    const pass = encoder.beginRenderPass({
      label: "Phase 0D-R1 pixel proof pass",
      colorAttachments: [
        {
          view: texture.createView({ format: this.readbackViewFormat }),
          clearValue: { r: 0.035, g: 0.055, b: 0.08, a: 1 },
          loadOp: "clear",
          storeOp: "store",
        },
      ],
    });
    this.encodeDrawBatches(pass);
    pass.end();
    encoder.copyTextureToBuffer(
      {
        texture,
        origin: { x: physicalX, y: physicalY, z: 0 },
      },
      {
        buffer: readback,
        bytesPerRow: 256,
        rowsPerImage: 1,
      },
      {
        width: 1,
        height: 1,
        depthOrArrayLayers: 1,
      },
    );
    this.queue.submit([encoder.finish()]);
    await readback.mapAsync(MAP_MODE.READ);
    const raw = new Uint8Array(readback.getMappedRange()).slice(0, 4);
    readback.unmap();
    readback.destroy();
    texture.destroy();
    const validation = await this.device.popErrorScope();
    if (validation) {
      this.validationErrors += 1;
      this.metrics.validation_errors = this.validationErrors;
      throw new RendererFailure("gpu_validation_error", validation.message);
    }
    const rgba = this.surfaceBaseFormat.startsWith("bgra")
      ? [raw[2], raw[1], raw[0], raw[3]]
      : [...raw];
    return {
      x,
      y,
      physical_x: physicalX,
      physical_y: physicalY,
      surface_base_format: this.surfaceBaseFormat,
      readback_view_format: this.readbackViewFormat,
      raw_channel_order: this.surfaceBaseFormat.startsWith("bgra") ? "BGRA" : "RGBA",
      normalized_channel_order: "RGBA",
      rgba,
    };
  }

  async applyEngineFrame(response, payload) {
    const started = performance.now();
    this.resize(response.camera.viewport[0], response.camera.viewport[1], response.camera.dpr);
    this.upload(response, payload);
    await this.render();
    this.metrics.frame_ms = performance.now() - started;
    this.metrics.frame_sequence = response.engine_sequence;
    return { ...this.metrics };
  }
}
// Generated from shared/render_contract.wgsl and render_binary_schema.json.
export const RENDER_BINARY_SCHEMA = Object.freeze({
  "version": 1,
  "endianness": "little",
  "instance_stride_bytes": 48,
  "dirty_record_stride_bytes": 52,
  "uniform_stride_bytes": 32,
  "primitive": {
    "rectangle": 0,
    "ellipse": 1
  },
  "instance_fields": {
    "linear": 0,
    "translation_hi": 16,
    "translation_lo": 24,
    "size": 32,
    "opacity": 40,
    "primitive": 44
  }
});
export const RENDER_BINARY_SCHEMA_VERSION = RENDER_BINARY_SCHEMA.version;
export const INSTANCE_STRIDE = RENDER_BINARY_SCHEMA.instance_stride_bytes;
export const DIRTY_STRIDE = RENDER_BINARY_SCHEMA.dirty_record_stride_bytes;
export const SHADER_SOURCE = "struct Instance {\n    linear: vec4<f32>,\n    translation_hi: vec2<f32>,\n    translation_lo: vec2<f32>,\n    size: vec2<f32>,\n    opacity: f32,\n    primitive: u32,\n};\n\nstruct View {\n    center_hi: vec2<f32>,\n    center_lo: vec2<f32>,\n    viewport: vec2<f32>,\n    zoom: f32,\n    _padding: f32,\n};\n\n@group(0) @binding(0) var<storage, read> instances: array<Instance>;\n@group(0) @binding(1) var<storage, read> visible_slots: array<u32>;\n@group(0) @binding(2) var<uniform> view: View;\n\nstruct VertexOutput {\n    @builtin(position) position: vec4<f32>,\n    @location(0) local_uv: vec2<f32>,\n    @location(1) opacity: f32,\n    @location(2) @interpolate(flat) primitive: u32,\n};\n\n@vertex\nfn vs_main(\n    @builtin(vertex_index) vertex_index: u32,\n    @builtin(instance_index) visible_index: u32,\n) -> VertexOutput {\n    var uv = vec2<f32>(0.0, 0.0);\n    switch vertex_index {\n        case 0u: { uv = vec2<f32>(0.0, 0.0); }\n        case 1u: { uv = vec2<f32>(1.0, 0.0); }\n        case 2u: { uv = vec2<f32>(0.0, 1.0); }\n        case 3u: { uv = vec2<f32>(0.0, 1.0); }\n        case 4u: { uv = vec2<f32>(1.0, 0.0); }\n        default: { uv = vec2<f32>(1.0, 1.0); }\n    }\n    let slot = visible_slots[visible_index];\n    let item = instances[slot];\n    let local = uv * item.size;\n    let transformed = vec2<f32>(\n        item.linear.x * local.x + item.linear.z * local.y,\n        item.linear.y * local.x + item.linear.w * local.y,\n    );\n    let relative_translation =\n        (item.translation_hi - view.center_hi) +\n        (item.translation_lo - view.center_lo);\n    let world = transformed + relative_translation;\n    let viewport = world * view.zoom + view.viewport * 0.5;\n    let clip = vec2<f32>(\n        viewport.x / view.viewport.x * 2.0 - 1.0,\n        1.0 - viewport.y / view.viewport.y * 2.0,\n    );\n\n    var output: VertexOutput;\n    output.position = vec4<f32>(clip, 0.0, 1.0);\n    output.local_uv = uv;\n    output.opacity = item.opacity;\n    output.primitive = item.primitive;\n    return output;\n}\n\n@fragment\nfn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {\n    if input.primitive == 1u {\n        let centered = input.local_uv - vec2<f32>(0.5, 0.5);\n        if dot(centered, centered) > 0.25 {\n            discard;\n        }\n        return vec4<f32>(0.96, 0.45, 0.25, input.opacity);\n    }\n    return vec4<f32>(0.20, 0.58, 0.96, input.opacity);\n}\n";

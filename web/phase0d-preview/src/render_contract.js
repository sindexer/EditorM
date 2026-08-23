// Generated from shared/render_contract.wgsl and render_binary_schema.json.
export const RENDER_BINARY_SCHEMA = Object.freeze({
  "version": 3,
  "endianness": "little",
  "instance_stride_bytes": 112,
  "dirty_record_stride_bytes": 116,
  "uniform_stride_bytes": 32,
  "path_instance_stride_bytes": 80,
  "path_vertex_stride_bytes": 32,
  "alpha_contract": "premultiplied-linear",
  "color_input": "srgb",
  "stroke_alignment": "center",
  "path_fill_rule": "even-odd",
  "primitive": {
    "rectangle": 0,
    "ellipse": 1,
    "path": 2
  },
  "path_vertex_kind": {
    "fill": 0,
    "stroke": 1
  },
  "instance_fields": {
    "linear": 0,
    "translation_hi": 16,
    "translation_lo": 24,
    "size": 32,
    "opacity": 40,
    "primitive": 44,
    "fill_linear": 48,
    "corner_radii": 64,
    "stroke_linear": 80,
    "stroke_width": 96,
    "padding": 100
  },
  "path_instance_fields": {
    "linear": 0,
    "translation_hi": 16,
    "translation_lo": 24,
    "fill_linear": 32,
    "stroke_linear": 48,
    "opacity": 64,
    "padding": 68
  },
  "path_vertex_fields": {
    "position": 0,
    "normal": 8,
    "coverage": 16,
    "kind": 20,
    "path_index": 24,
    "padding": 28
  }
});
export const RENDER_BINARY_SCHEMA_VERSION = RENDER_BINARY_SCHEMA.version;
export const INSTANCE_STRIDE = RENDER_BINARY_SCHEMA.instance_stride_bytes;
export const DIRTY_STRIDE = RENDER_BINARY_SCHEMA.dirty_record_stride_bytes;
export const PATH_INSTANCE_STRIDE = RENDER_BINARY_SCHEMA.path_instance_stride_bytes;
export const PATH_VERTEX_STRIDE = RENDER_BINARY_SCHEMA.path_vertex_stride_bytes;
export const SHADER_SOURCE = "struct Instance {\n    linear: vec4<f32>,\n    translation_hi: vec2<f32>,\n    translation_lo: vec2<f32>,\n    size: vec2<f32>,\n    opacity: f32,\n    primitive: u32,\n    fill_linear: vec4<f32>,\n    corner_radii: vec4<f32>,\n    stroke_linear: vec4<f32>,\n    stroke_width: f32,\n    _padding_0: f32,\n    _padding_1: f32,\n    _padding_2: f32,\n};\n\nstruct View {\n    center_hi: vec2<f32>,\n    center_lo: vec2<f32>,\n    viewport: vec2<f32>,\n    zoom: f32,\n    _padding: f32,\n};\n\n// One path node: the transform and colours shared by every triangle of that path.\nstruct PathInstance {\n    linear: vec4<f32>,\n    translation_hi: vec2<f32>,\n    translation_lo: vec2<f32>,\n    fill_linear: vec4<f32>,\n    stroke_linear: vec4<f32>,\n    opacity: f32,\n    _padding_0: f32,\n    _padding_1: f32,\n    _padding_2: f32,\n};\n\n// One tessellated vertex in its path's local space. `normal` is the outward feather direction\n// (zero for interior vertices) and `coverage` is 1.0 inside the shape and 0.0 at the feather rim.\nstruct PathVertexRecord {\n    position: vec2<f32>,\n    normal: vec2<f32>,\n    coverage: f32,\n    kind: u32,\n    path_index: u32,\n    _padding: u32,\n};\n\n@group(0) @binding(0) var<storage, read> instances: array<Instance>;\n@group(0) @binding(1) var<storage, read> visible_slots: array<u32>;\n@group(0) @binding(2) var<uniform> view: View;\n@group(0) @binding(3) var<storage, read> path_instances: array<PathInstance>;\n@group(0) @binding(4) var<storage, read> path_vertices: array<PathVertexRecord>;\n\nstruct VertexOutput {\n    @builtin(position) position: vec4<f32>,\n    @location(0) local_position: vec2<f32>,\n    @location(1) @interpolate(flat) size: vec2<f32>,\n    @location(2) @interpolate(flat) opacity: f32,\n    @location(3) @interpolate(flat) primitive: u32,\n    @location(4) @interpolate(flat) fill_linear: vec4<f32>,\n    @location(5) @interpolate(flat) corner_radii: vec4<f32>,\n    @location(6) @interpolate(flat) stroke_linear: vec4<f32>,\n    @location(7) @interpolate(flat) stroke_width: f32,\n};\n\n@vertex\nfn vs_main(\n    @builtin(vertex_index) vertex_index: u32,\n    @builtin(instance_index) visible_index: u32,\n) -> VertexOutput {\n    var uv = vec2<f32>(0.0, 0.0);\n    switch vertex_index {\n        case 0u: { uv = vec2<f32>(0.0, 0.0); }\n        case 1u: { uv = vec2<f32>(1.0, 0.0); }\n        case 2u: { uv = vec2<f32>(0.0, 1.0); }\n        case 3u: { uv = vec2<f32>(0.0, 1.0); }\n        case 4u: { uv = vec2<f32>(1.0, 0.0); }\n        default: { uv = vec2<f32>(1.0, 1.0); }\n    }\n    let slot = visible_slots[visible_index];\n    let item = instances[slot];\n    let half_stroke = item.stroke_width * 0.5;\n    let expansion = vec2<f32>(half_stroke);\n    let local = uv * (item.size + expansion * 2.0) - expansion;\n    let transformed = vec2<f32>(\n        item.linear.x * local.x + item.linear.y * local.y,\n        item.linear.z * local.x + item.linear.w * local.y,\n    );\n    let relative_translation =\n        (item.translation_hi - view.center_hi) +\n        (item.translation_lo - view.center_lo);\n    let world = transformed + relative_translation;\n    let viewport = world * view.zoom + view.viewport * 0.5;\n    let clip = vec2<f32>(\n        viewport.x / view.viewport.x * 2.0 - 1.0,\n        1.0 - viewport.y / view.viewport.y * 2.0,\n    );\n\n    var output: VertexOutput;\n    output.position = vec4<f32>(clip, 0.0, 1.0);\n    output.local_position = local;\n    output.size = item.size;\n    output.opacity = item.opacity;\n    output.primitive = item.primitive;\n    output.fill_linear = item.fill_linear;\n    output.corner_radii = item.corner_radii;\n    output.stroke_linear = item.stroke_linear;\n    output.stroke_width = item.stroke_width;\n    return output;\n}\n\nfn normalized_corner_radii(size: vec2<f32>, source: vec4<f32>) -> vec4<f32> {\n    let positive = max(source, vec4<f32>(0.0));\n    let horizontal = max(positive.x + positive.y, positive.w + positive.z);\n    let vertical = max(positive.x + positive.w, positive.y + positive.z);\n    let scale_x = size.x / max(horizontal, 0.000001);\n    let scale_y = size.y / max(vertical, 0.000001);\n    let scale = min(1.0, min(scale_x, scale_y));\n    let maximum = min(size.x, size.y) * 0.5;\n    return min(positive * scale, vec4<f32>(maximum));\n}\n\nfn rounded_rectangle_distance(\n    local: vec2<f32>,\n    size: vec2<f32>,\n    source_radii: vec4<f32>,\n) -> f32 {\n    let radii = normalized_corner_radii(size, source_radii);\n    let centered = local - size * 0.5;\n    var radius = radii.x;\n    if centered.x >= 0.0 && centered.y < 0.0 {\n        radius = radii.y;\n    } else if centered.x >= 0.0 && centered.y >= 0.0 {\n        radius = radii.z;\n    } else if centered.x < 0.0 && centered.y >= 0.0 {\n        radius = radii.w;\n    }\n    let q = abs(centered) - size * 0.5 + vec2<f32>(radius);\n    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;\n}\n\nfn ellipse_distance(local: vec2<f32>, size: vec2<f32>) -> f32 {\n    let radii = max(size * 0.5, vec2<f32>(0.000001));\n    let normalized = (local - size * 0.5) / radii;\n    let normalized_length = length(normalized);\n    if normalized_length < 0.000001 {\n        return -min(radii.x, radii.y);\n    }\n    let gradient_length = length(normalized / radii) / normalized_length;\n    return (normalized_length - 1.0) / max(gradient_length, 0.000001);\n}\n\nfn coverage(distance: f32) -> f32 {\n    let width = max(fwidth(distance), 0.0001);\n    return 1.0 - smoothstep(-width, width, distance);\n}\n\n@fragment\nfn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {\n    var signed_distance = rounded_rectangle_distance(\n        input.local_position,\n        input.size,\n        input.corner_radii,\n    );\n    if input.primitive == 1u {\n        signed_distance = ellipse_distance(input.local_position, input.size);\n    }\n\n    let half_stroke = input.stroke_width * 0.5;\n    let outer_coverage = coverage(signed_distance - half_stroke);\n    let inner_coverage = coverage(signed_distance + half_stroke);\n    let stroke_enabled = select(0.0, 1.0, input.stroke_width > 0.0);\n    let fill_coverage = mix(outer_coverage, inner_coverage, stroke_enabled);\n    let stroke_coverage = max(outer_coverage - fill_coverage, 0.0);\n\n    let fill_alpha = input.fill_linear.a * fill_coverage;\n    let stroke_alpha = input.stroke_linear.a * stroke_coverage;\n    let alpha = fill_alpha + stroke_alpha;\n    let premultiplied =\n        input.fill_linear.rgb * fill_alpha +\n        input.stroke_linear.rgb * stroke_alpha;\n    return vec4<f32>(premultiplied, alpha) * input.opacity;\n}\n\nstruct PathVertexOutput {\n    @builtin(position) position: vec4<f32>,\n    @location(0) @interpolate(flat) fill_linear: vec4<f32>,\n    @location(1) @interpolate(flat) stroke_linear: vec4<f32>,\n    @location(2) @interpolate(flat) kind: u32,\n    @location(3) @interpolate(flat) opacity: f32,\n    @location(4) coverage: f32,\n};\n\n// Paths are pre-tessellated on the CPU in local space and cached, so this stage only transforms\n// vertices and pushes feather vertices one pixel outward in screen space. That keeps a single\n// cached tessellation antialiased at every zoom level without MSAA.\n@vertex\nfn vs_path(@builtin(vertex_index) vertex_index: u32) -> PathVertexOutput {\n    let vertex = path_vertices[vertex_index];\n    let item = path_instances[vertex.path_index];\n    let local = vertex.position;\n    let transformed = vec2<f32>(\n        item.linear.x * local.x + item.linear.y * local.y,\n        item.linear.z * local.x + item.linear.w * local.y,\n    );\n    let relative_translation =\n        (item.translation_hi - view.center_hi) +\n        (item.translation_lo - view.center_lo);\n    let world = transformed + relative_translation;\n    var viewport_position = world * view.zoom + view.viewport * 0.5;\n\n    let transformed_normal = vec2<f32>(\n        item.linear.x * vertex.normal.x + item.linear.y * vertex.normal.y,\n        item.linear.z * vertex.normal.x + item.linear.w * vertex.normal.y,\n    );\n    let screen_normal = transformed_normal * view.zoom;\n    let normal_length = length(screen_normal);\n    if normal_length > 0.000001 {\n        viewport_position = viewport_position + screen_normal / normal_length;\n    }\n\n    let clip = vec2<f32>(\n        viewport_position.x / view.viewport.x * 2.0 - 1.0,\n        1.0 - viewport_position.y / view.viewport.y * 2.0,\n    );\n\n    var output: PathVertexOutput;\n    output.position = vec4<f32>(clip, 0.0, 1.0);\n    output.fill_linear = item.fill_linear;\n    output.stroke_linear = item.stroke_linear;\n    output.kind = vertex.kind;\n    output.opacity = item.opacity;\n    output.coverage = vertex.coverage;\n    return output;\n}\n\n@fragment\nfn fs_path(input: PathVertexOutput) -> @location(0) vec4<f32> {\n    var color = input.fill_linear;\n    if input.kind == 1u {\n        color = input.stroke_linear;\n    }\n    let alpha = color.a * clamp(input.coverage, 0.0, 1.0) * input.opacity;\n    return vec4<f32>(color.rgb * alpha, alpha);\n}\n";

struct Instance {
    linear: vec4<f32>,
    translation_hi: vec2<f32>,
    translation_lo: vec2<f32>,
    size: vec2<f32>,
    opacity: f32,
    primitive: u32,
    fill_linear: vec4<f32>,
    corner_radii: vec4<f32>,
    stroke_linear: vec4<f32>,
    stroke_width: f32,
    _padding_0: f32,
    _padding_1: f32,
    _padding_2: f32,
};

struct View {
    center_hi: vec2<f32>,
    center_lo: vec2<f32>,
    viewport: vec2<f32>,
    zoom: f32,
    _padding: f32,
};

// One path node: the transform and colours shared by every triangle of that path.
struct PathInstance {
    linear: vec4<f32>,
    translation_hi: vec2<f32>,
    translation_lo: vec2<f32>,
    fill_linear: vec4<f32>,
    stroke_linear: vec4<f32>,
    opacity: f32,
    _padding_0: f32,
    _padding_1: f32,
    _padding_2: f32,
};

// One tessellated vertex in its path's local space. `normal` is the outward feather direction
// (zero for interior vertices) and `coverage` is 1.0 inside the shape and 0.0 at the feather rim.
struct PathVertexRecord {
    position: vec2<f32>,
    normal: vec2<f32>,
    coverage: f32,
    kind: u32,
    path_index: u32,
    _padding: u32,
};

@group(0) @binding(0) var<storage, read> instances: array<Instance>;
@group(0) @binding(1) var<storage, read> visible_slots: array<u32>;
@group(0) @binding(2) var<uniform> view: View;
@group(0) @binding(3) var<storage, read> path_instances: array<PathInstance>;
@group(0) @binding(4) var<storage, read> path_vertices: array<PathVertexRecord>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local_position: vec2<f32>,
    @location(1) @interpolate(flat) size: vec2<f32>,
    @location(2) @interpolate(flat) opacity: f32,
    @location(3) @interpolate(flat) primitive: u32,
    @location(4) @interpolate(flat) fill_linear: vec4<f32>,
    @location(5) @interpolate(flat) corner_radii: vec4<f32>,
    @location(6) @interpolate(flat) stroke_linear: vec4<f32>,
    @location(7) @interpolate(flat) stroke_width: f32,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) visible_index: u32,
) -> VertexOutput {
    var uv = vec2<f32>(0.0, 0.0);
    switch vertex_index {
        case 0u: { uv = vec2<f32>(0.0, 0.0); }
        case 1u: { uv = vec2<f32>(1.0, 0.0); }
        case 2u: { uv = vec2<f32>(0.0, 1.0); }
        case 3u: { uv = vec2<f32>(0.0, 1.0); }
        case 4u: { uv = vec2<f32>(1.0, 0.0); }
        default: { uv = vec2<f32>(1.0, 1.0); }
    }
    let slot = visible_slots[visible_index];
    let item = instances[slot];
    let half_stroke = item.stroke_width * 0.5;
    let expansion = vec2<f32>(half_stroke);
    let local = uv * (item.size + expansion * 2.0) - expansion;
    let transformed = vec2<f32>(
        item.linear.x * local.x + item.linear.y * local.y,
        item.linear.z * local.x + item.linear.w * local.y,
    );
    let relative_translation =
        (item.translation_hi - view.center_hi) +
        (item.translation_lo - view.center_lo);
    let world = transformed + relative_translation;
    let viewport = world * view.zoom + view.viewport * 0.5;
    let clip = vec2<f32>(
        viewport.x / view.viewport.x * 2.0 - 1.0,
        1.0 - viewport.y / view.viewport.y * 2.0,
    );

    var output: VertexOutput;
    output.position = vec4<f32>(clip, 0.0, 1.0);
    output.local_position = local;
    output.size = item.size;
    output.opacity = item.opacity;
    output.primitive = item.primitive;
    output.fill_linear = item.fill_linear;
    output.corner_radii = item.corner_radii;
    output.stroke_linear = item.stroke_linear;
    output.stroke_width = item.stroke_width;
    return output;
}

fn normalized_corner_radii(size: vec2<f32>, source: vec4<f32>) -> vec4<f32> {
    let positive = max(source, vec4<f32>(0.0));
    let horizontal = max(positive.x + positive.y, positive.w + positive.z);
    let vertical = max(positive.x + positive.w, positive.y + positive.z);
    let scale_x = size.x / max(horizontal, 0.000001);
    let scale_y = size.y / max(vertical, 0.000001);
    let scale = min(1.0, min(scale_x, scale_y));
    let maximum = min(size.x, size.y) * 0.5;
    return min(positive * scale, vec4<f32>(maximum));
}

fn rounded_rectangle_distance(
    local: vec2<f32>,
    size: vec2<f32>,
    source_radii: vec4<f32>,
) -> f32 {
    let radii = normalized_corner_radii(size, source_radii);
    let centered = local - size * 0.5;
    var radius = radii.x;
    if centered.x >= 0.0 && centered.y < 0.0 {
        radius = radii.y;
    } else if centered.x >= 0.0 && centered.y >= 0.0 {
        radius = radii.z;
    } else if centered.x < 0.0 && centered.y >= 0.0 {
        radius = radii.w;
    }
    let q = abs(centered) - size * 0.5 + vec2<f32>(radius);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

fn ellipse_distance(local: vec2<f32>, size: vec2<f32>) -> f32 {
    let radii = max(size * 0.5, vec2<f32>(0.000001));
    let normalized = (local - size * 0.5) / radii;
    let normalized_length = length(normalized);
    if normalized_length < 0.000001 {
        return -min(radii.x, radii.y);
    }
    let gradient_length = length(normalized / radii) / normalized_length;
    return (normalized_length - 1.0) / max(gradient_length, 0.000001);
}

fn coverage(distance: f32) -> f32 {
    let width = max(fwidth(distance), 0.0001);
    return 1.0 - smoothstep(-width, width, distance);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var signed_distance = rounded_rectangle_distance(
        input.local_position,
        input.size,
        input.corner_radii,
    );
    if input.primitive == 1u {
        signed_distance = ellipse_distance(input.local_position, input.size);
    }

    let half_stroke = input.stroke_width * 0.5;
    let outer_coverage = coverage(signed_distance - half_stroke);
    let inner_coverage = coverage(signed_distance + half_stroke);
    let stroke_enabled = select(0.0, 1.0, input.stroke_width > 0.0);
    let fill_coverage = mix(outer_coverage, inner_coverage, stroke_enabled);
    let stroke_coverage = max(outer_coverage - fill_coverage, 0.0);

    let fill_alpha = input.fill_linear.a * fill_coverage;
    let stroke_alpha = input.stroke_linear.a * stroke_coverage;
    let alpha = fill_alpha + stroke_alpha;
    let premultiplied =
        input.fill_linear.rgb * fill_alpha +
        input.stroke_linear.rgb * stroke_alpha;
    return vec4<f32>(premultiplied, alpha) * input.opacity;
}

struct PathVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) fill_linear: vec4<f32>,
    @location(1) @interpolate(flat) stroke_linear: vec4<f32>,
    @location(2) @interpolate(flat) kind: u32,
    @location(3) @interpolate(flat) opacity: f32,
    @location(4) coverage: f32,
};

// Paths are pre-tessellated on the CPU in local space and cached, so this stage only transforms
// vertices and pushes feather vertices one pixel outward in screen space. That keeps a single
// cached tessellation antialiased at every zoom level without MSAA.
@vertex
fn vs_path(@builtin(vertex_index) vertex_index: u32) -> PathVertexOutput {
    let vertex = path_vertices[vertex_index];
    let item = path_instances[vertex.path_index];
    let local = vertex.position;
    let transformed = vec2<f32>(
        item.linear.x * local.x + item.linear.y * local.y,
        item.linear.z * local.x + item.linear.w * local.y,
    );
    let relative_translation =
        (item.translation_hi - view.center_hi) +
        (item.translation_lo - view.center_lo);
    let world = transformed + relative_translation;
    var viewport_position = world * view.zoom + view.viewport * 0.5;

    let transformed_normal = vec2<f32>(
        item.linear.x * vertex.normal.x + item.linear.y * vertex.normal.y,
        item.linear.z * vertex.normal.x + item.linear.w * vertex.normal.y,
    );
    let screen_normal = transformed_normal * view.zoom;
    let normal_length = length(screen_normal);
    if normal_length > 0.000001 {
        viewport_position = viewport_position + screen_normal / normal_length;
    }

    let clip = vec2<f32>(
        viewport_position.x / view.viewport.x * 2.0 - 1.0,
        1.0 - viewport_position.y / view.viewport.y * 2.0,
    );

    var output: PathVertexOutput;
    output.position = vec4<f32>(clip, 0.0, 1.0);
    output.fill_linear = item.fill_linear;
    output.stroke_linear = item.stroke_linear;
    output.kind = vertex.kind;
    output.opacity = item.opacity;
    output.coverage = vertex.coverage;
    return output;
}

@fragment
fn fs_path(input: PathVertexOutput) -> @location(0) vec4<f32> {
    var color = input.fill_linear;
    if input.kind == 1u {
        color = input.stroke_linear;
    }
    let alpha = color.a * clamp(input.coverage, 0.0, 1.0) * input.opacity;
    return vec4<f32>(color.rgb * alpha, alpha);
}

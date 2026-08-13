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

@group(0) @binding(0) var<storage, read> instances: array<Instance>;
@group(0) @binding(1) var<storage, read> visible_slots: array<u32>;
@group(0) @binding(2) var<uniform> view: View;

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
    let ellipse_axis_ratio = max(item.size.x, item.size.y) /
        max(min(item.size.x, item.size.y), 0.000001);
    let expansion = select(
        half_stroke,
        half_stroke * ellipse_axis_ratio,
        item.primitive == 1u,
    );
    let local = uv * (item.size + vec2<f32>(expansion * 2.0)) - vec2<f32>(expansion);
    let transformed = vec2<f32>(
        item.linear.x * local.x + item.linear.z * local.y,
        item.linear.y * local.x + item.linear.w * local.y,
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
    return (length(normalized) - 1.0) * min(radii.x, radii.y);
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

    let fill_alpha = input.fill_linear.a * input.opacity * fill_coverage;
    let stroke_alpha = input.stroke_linear.a * input.opacity * stroke_coverage;
    let alpha = stroke_alpha + fill_alpha * (1.0 - stroke_alpha);
    let premultiplied =
        input.stroke_linear.rgb * stroke_alpha +
        input.fill_linear.rgb * fill_alpha * (1.0 - stroke_alpha);
    return vec4<f32>(premultiplied, alpha);
}

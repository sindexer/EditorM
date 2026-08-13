struct Instance {
    linear: vec4<f32>,
    translation_hi: vec2<f32>,
    translation_lo: vec2<f32>,
    size: vec2<f32>,
    opacity: f32,
    primitive: u32,
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
    @location(0) local_uv: vec2<f32>,
    @location(1) opacity: f32,
    @location(2) @interpolate(flat) primitive: u32,
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
    let local = uv * item.size;
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
    output.local_uv = uv;
    output.opacity = item.opacity;
    output.primitive = item.primitive;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.primitive == 1u {
        let centered = input.local_uv - vec2<f32>(0.5, 0.5);
        if dot(centered, centered) > 0.25 {
            discard;
        }
        return vec4<f32>(0.96, 0.45, 0.25, input.opacity);
    }
    return vec4<f32>(0.20, 0.58, 0.96, input.opacity);
}

//! Byte encoding for the path GPU buffers.
//!
//! Both GPU consumers — the native wgpu renderer and the WASM bridge that feeds the browser
//! renderer — encode path records here, so there is exactly one definition of the path binary
//! layout on the Rust side and it always matches `shared/render_binary_schema.json`.

use crate::{PathVertex, RenderItem, RenderModel};

/// Bytes per path instance record (`PathInstance` in `shared/render_contract.wgsl`).
pub const PATH_INSTANCE_STRIDE_BYTES: usize = 80;
/// Bytes per path vertex record (`PathVertexRecord` in `shared/render_contract.wgsl`).
pub const PATH_VERTEX_STRIDE_BYTES: usize = 32;

/// Encodes one path item's transform and colours.
///
/// Returns `None` when the item cannot be represented at the f32 GPU boundary, matching the
/// omission contract the analytic primitives already use.
#[must_use]
pub fn encode_path_instance(item: &RenderItem) -> Option<[u8; PATH_INSTANCE_STRIDE_BYTES]> {
    let world = item.world_transform?;
    let checked = [
        world.m11,
        world.m12,
        world.m21,
        world.m22,
        item.opacity,
        item.fill_linear[0],
        item.fill_linear[1],
        item.fill_linear[2],
        item.fill_linear[3],
        item.stroke_linear[0],
        item.stroke_linear[1],
        item.stroke_linear[2],
        item.stroke_linear[3],
    ];
    if checked.iter().any(|value| !finite_f32(*value)) {
        return None;
    }
    let (tx_hi, tx_lo) = split_f64(world.tx)?;
    let (ty_hi, ty_lo) = split_f64(world.ty)?;
    let mut bytes = [0_u8; PATH_INSTANCE_STRIDE_BYTES];
    let head = [
        world.m11 as f32,
        world.m12 as f32,
        world.m21 as f32,
        world.m22 as f32,
        tx_hi,
        ty_hi,
        tx_lo,
        ty_lo,
    ];
    for (index, value) in head.into_iter().enumerate() {
        write_f32(&mut bytes, index * 4, value);
    }
    for (index, value) in item.fill_linear.into_iter().enumerate() {
        write_f32(&mut bytes, 32 + index * 4, value as f32);
    }
    for (index, value) in item.stroke_linear.into_iter().enumerate() {
        write_f32(&mut bytes, 48 + index * 4, value as f32);
    }
    write_f32(&mut bytes, 64, item.opacity as f32);
    Some(bytes)
}

/// Encodes one tessellated vertex together with the instance it belongs to.
#[must_use]
pub fn encode_path_vertex(
    vertex: &PathVertex,
    path_index: u32,
) -> Option<[u8; PATH_VERTEX_STRIDE_BYTES]> {
    let checked = [
        vertex.position.x,
        vertex.position.y,
        vertex.normal.x,
        vertex.normal.y,
        vertex.coverage,
    ];
    if checked.iter().any(|value| !finite_f32(*value)) {
        return None;
    }
    let mut bytes = [0_u8; PATH_VERTEX_STRIDE_BYTES];
    write_f32(&mut bytes, 0, vertex.position.x as f32);
    write_f32(&mut bytes, 4, vertex.position.y as f32);
    write_f32(&mut bytes, 8, vertex.normal.x as f32);
    write_f32(&mut bytes, 12, vertex.normal.y as f32);
    write_f32(&mut bytes, 16, vertex.coverage as f32);
    bytes[20..24].copy_from_slice(&vertex.kind.to_le_bytes());
    bytes[24..28].copy_from_slice(&path_index.to_le_bytes());
    Some(bytes)
}

/// Encodes the whole path instance array in compact path-index order.
#[must_use]
pub fn encode_path_instances(model: &RenderModel) -> Vec<u8> {
    let mut bytes = vec![0_u8; model.path_item_count() * PATH_INSTANCE_STRIDE_BYTES];
    for item in model.path_items() {
        let Some(path_index) = item.path_index else {
            continue;
        };
        let Some(encoded) = encode_path_instance(item) else {
            continue;
        };
        let offset = path_index as usize * PATH_INSTANCE_STRIDE_BYTES;
        bytes[offset..offset + PATH_INSTANCE_STRIDE_BYTES].copy_from_slice(&encoded);
    }
    bytes
}

/// Encodes the whole path vertex buffer, laid out exactly as `path_vertex_range` reports.
#[must_use]
pub fn encode_path_vertices(model: &RenderModel) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(model.path_vertex_count() * PATH_VERTEX_STRIDE_BYTES);
    for item in model.path_items() {
        let (Some(path_index), Some(tessellation)) = (item.path_index, item.path.as_ref()) else {
            continue;
        };
        for vertex in &tessellation.vertices {
            match encode_path_vertex(vertex, path_index) {
                Some(encoded) => bytes.extend_from_slice(&encoded),
                // A vertex that cannot cross the f32 boundary degenerates instead of shifting
                // every later vertex, which would corrupt the ranges the draw batches use.
                None => bytes.extend_from_slice(&[0_u8; PATH_VERTEX_STRIDE_BYTES]),
            }
        }
    }
    bytes
}

fn write_f32(bytes: &mut [u8], offset: usize, value: f32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn finite_f32(value: f64) -> bool {
    value.is_finite() && (value as f32).is_finite()
}

fn split_f64(value: f64) -> Option<(f32, f32)> {
    if !value.is_finite() {
        return None;
    }
    let high = value as f32;
    if !high.is_finite() {
        return None;
    }
    let low = (value - f64::from(high)) as f32;
    low.is_finite().then_some((high, low))
}

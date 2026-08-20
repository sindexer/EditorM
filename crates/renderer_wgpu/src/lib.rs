//! Actual wgpu backend for the backend-neutral RenderModel.

use std::collections::BTreeSet;
use std::mem::size_of;
use std::time::Instant;

use bytemuck::{Pod, Zeroable};
use thiserror::Error;
use visual_authoring_core_math::Vec2;
use visual_authoring_render_model::{
    CullingResult, PrimitiveKind, RenderDelta, RenderItem, RenderModel,
};
use wgpu::util::DeviceExt;

pub const RENDER_BINARY_SCHEMA_VERSION: u32 = 2;
pub const INSTANCE_STRIDE_BYTES: usize = 112;
pub const DIRTY_RECORD_STRIDE_BYTES: usize = 116;
pub const VIEW_UNIFORM_BYTES: usize = 32;
pub const SHADER_SOURCE: &str = include_str!("../../../shared/render_contract.wgsl");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RendererCapabilities {
    pub adapter_name: String,
    pub backend: String,
    pub device_type: String,
    pub driver: String,
    pub driver_info: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuResourceMetrics {
    pub buffer_count: u64,
    pub buffer_bytes: u64,
    pub upload_calls: u64,
    pub upload_bytes: u64,
    pub dirty_instance_ranges: u64,
    pub full_instance_buffer_uploads: u64,
    pub draw_calls: u64,
    pub batches: u64,
    pub submitted_instances: u64,
    pub validation_errors: u64,
    pub render_submit_micros: u64,
    pub gpu_encode_attempted: u64,
    pub gpu_encode_omitted: u64,
    pub instance_upload_bytes: u64,
    pub visible_slot_upload_bytes: u64,
    pub allocation_growth_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderView {
    pub center: Vec2,
    pub zoom: f64,
    pub viewport_size: Vec2,
    pub device_pixel_ratio: f64,
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum RendererError {
    #[error("no WebGPU/wgpu adapter is available")]
    AdapterUnavailable,
    #[error("requesting a WebGPU/wgpu device failed: {0}")]
    RequestDevice(String),
    #[error("render target dimensions must be non-zero")]
    InvalidTargetSize,
    #[error("render view contains invalid numeric values")]
    InvalidView,
    #[error("node {node_id} cannot be represented safely at the f32 GPU boundary")]
    F32Conversion {
        node_id: visual_authoring_document::NodeId,
    },
    #[error("GPU validation failed: {0}")]
    Validation(String),
    #[error("surface was lost")]
    SurfaceLost,
    #[error("surface is outdated")]
    SurfaceOutdated,
    #[error("device was lost: {0}")]
    DeviceLost(String),
    #[error("shader or pipeline creation failed: {0}")]
    Pipeline(String),
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct InstanceRaw {
    linear: [f32; 4],
    translation_hi: [f32; 2],
    translation_lo: [f32; 2],
    size: [f32; 2],
    opacity: f32,
    primitive: u32,
    fill_linear: [f32; 4],
    corner_radii: [f32; 4],
    stroke_linear: [f32; 4],
    stroke_width: f32,
    padding: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
struct ViewRaw {
    center_hi: [f32; 2],
    center_lo: [f32; 2],
    viewport: [f32; 2],
    zoom: f32,
    padding: f32,
}

pub struct WgpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    visible_buffer: wgpu::Buffer,
    view_buffer: wgpu::Buffer,
    instance_capacity: usize,
    visible_capacity: usize,
    capabilities: RendererCapabilities,
    metrics: GpuResourceMetrics,
    last_frame: GpuResourceMetrics,
}

impl WgpuRenderer {
    pub async fn new_headless() -> Result<Self, RendererError> {
        let instance = wgpu::Instance::default();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or(RendererError::AdapterUnavailable)?;
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("visual-authoring-engine-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
                },
                None,
            )
            .await
            .map_err(|error| RendererError::RequestDevice(error.to_string()))?;
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("visual-authoring-engine-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SOURCE.into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("visual-authoring-engine-bind-group-layout"),
            entries: &[
                storage_layout_entry(0),
                storage_layout_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("visual-authoring-engine-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("visual-authoring-engine-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview: None,
        });
        let instance_buffer = create_buffer(
            &device,
            "visual-authoring-instances",
            size_of::<InstanceRaw>() as u64,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let visible_buffer = create_buffer(
            &device,
            "visual-authoring-visible-slots",
            size_of::<u32>() as u64,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let view_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("visual-authoring-view"),
            contents: bytemuck::bytes_of(&ViewRaw::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = create_bind_group(
            &device,
            &bind_group_layout,
            &instance_buffer,
            &visible_buffer,
            &view_buffer,
        );
        device.poll(wgpu::Maintain::Wait);
        if let Some(error) = device.pop_error_scope().await {
            return Err(RendererError::Pipeline(error.to_string()));
        }

        let buffer_bytes =
            size_of::<InstanceRaw>() as u64 + size_of::<u32>() as u64 + size_of::<ViewRaw>() as u64;
        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group_layout,
            bind_group,
            instance_buffer,
            visible_buffer,
            view_buffer,
            instance_capacity: 1,
            visible_capacity: 1,
            capabilities: RendererCapabilities {
                adapter_name: info.name,
                backend: format!("{:?}", info.backend),
                device_type: format!("{:?}", info.device_type),
                driver: info.driver,
                driver_info: info.driver_info,
            },
            metrics: GpuResourceMetrics {
                buffer_count: 3,
                buffer_bytes,
                ..GpuResourceMetrics::default()
            },
            last_frame: GpuResourceMetrics::default(),
        })
    }

    #[must_use]
    pub const fn capabilities(&self) -> &RendererCapabilities {
        &self.capabilities
    }

    #[must_use]
    pub const fn metrics(&self) -> GpuResourceMetrics {
        self.metrics
    }

    #[must_use]
    pub const fn last_frame_metrics(&self) -> GpuResourceMetrics {
        self.last_frame
    }

    pub fn sync_model(
        &mut self,
        model: &RenderModel,
        delta: &RenderDelta,
    ) -> Result<GpuResourceMetrics, RendererError> {
        self.last_frame = GpuResourceMetrics::default();
        let required = model.allocated_slot_count().max(1);
        if required > self.instance_capacity {
            self.instance_capacity = required.next_power_of_two();
            let mut raws = vec![InstanceRaw::default(); self.instance_capacity];
            for item in model.items() {
                if item.renderable {
                    self.last_frame.gpu_encode_attempted += 1;
                }
                if item.gpu_encodable {
                    raws[item.slot as usize] = encode_instance(item)?;
                } else if item.renderable {
                    self.last_frame.gpu_encode_omitted += 1;
                }
            }
            self.instance_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("visual-authoring-instances"),
                        contents: bytemuck::cast_slice(&raws),
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    });
            self.recreate_bind_group();
            let uploaded = bytemuck::cast_slice::<InstanceRaw, u8>(&raws).len() as u64;
            self.record_upload(uploaded);
            self.last_frame.instance_upload_bytes += uploaded;
            self.last_frame.full_instance_buffer_uploads = 1;
            self.last_frame.allocation_growth_count += 1;
        } else {
            let mut slots = BTreeSet::new();
            slots.extend(delta.dirty_slots.iter().copied());
            slots.extend(delta.removed_slots.iter().copied());
            for (first, count) in coalesce_slots(&slots) {
                let mut raws = Vec::with_capacity(count as usize);
                for slot in first..first + count {
                    let raw = if let Some(item) = model.item_by_slot(slot) {
                        if item.renderable {
                            self.last_frame.gpu_encode_attempted += 1;
                        }
                        if item.gpu_encodable {
                            encode_instance(item)?
                        } else {
                            if item.renderable {
                                self.last_frame.gpu_encode_omitted += 1;
                            }
                            InstanceRaw::default()
                        }
                    } else {
                        InstanceRaw::default()
                    };
                    raws.push(raw);
                }
                let offset = first as u64 * size_of::<InstanceRaw>() as u64;
                self.queue
                    .write_buffer(&self.instance_buffer, offset, bytemuck::cast_slice(&raws));
                let uploaded = bytemuck::cast_slice::<InstanceRaw, u8>(&raws).len() as u64;
                self.record_upload(uploaded);
                self.last_frame.instance_upload_bytes += uploaded;
                self.last_frame.dirty_instance_ranges += 1;
            }
        }
        self.finish_last_frame();
        Ok(self.last_frame)
    }

    pub async fn render_offscreen(
        &mut self,
        culling: &CullingResult,
        view: RenderView,
        width: u32,
        height: u32,
    ) -> Result<GpuResourceMetrics, RendererError> {
        self.last_frame = GpuResourceMetrics::default();
        if width == 0 || height == 0 {
            return Err(RendererError::InvalidTargetSize);
        }
        let view_raw = encode_view(view)?;
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        self.ensure_visible_capacity(culling.slots_bottom_to_top.len().max(1));
        if !culling.slots_bottom_to_top.is_empty() {
            self.queue.write_buffer(
                &self.visible_buffer,
                0,
                bytemuck::cast_slice(&culling.slots_bottom_to_top),
            );
            let uploaded = (culling.slots_bottom_to_top.len() * size_of::<u32>()) as u64;
            self.record_upload(uploaded);
            self.last_frame.visible_slot_upload_bytes += uploaded;
        }
        self.queue
            .write_buffer(&self.view_buffer, 0, bytemuck::bytes_of(&view_raw));
        self.record_upload(size_of::<ViewRaw>() as u64);

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("visual-authoring-offscreen-target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let start = Instant::now();
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("visual-authoring-render-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("visual-authoring-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.025,
                            g: 0.035,
                            b: 0.055,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if !culling.slots_bottom_to_top.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.draw(0..6, 0..culling.slots_bottom_to_top.len() as u32);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        self.device.poll(wgpu::Maintain::Wait);
        self.last_frame.draw_calls = u64::from(!culling.slots_bottom_to_top.is_empty());
        self.last_frame.batches = self.last_frame.draw_calls;
        self.last_frame.submitted_instances = culling.slots_bottom_to_top.len() as u64;
        self.last_frame.render_submit_micros = start.elapsed().as_micros() as u64;
        if let Some(error) = self.device.pop_error_scope().await {
            self.last_frame.validation_errors += 1;
            self.finish_last_frame();
            return Err(RendererError::Validation(error.to_string()));
        }
        self.finish_last_frame();
        Ok(self.last_frame)
    }

    fn ensure_visible_capacity(&mut self, required: usize) {
        if required <= self.visible_capacity {
            return;
        }
        self.visible_capacity = required.next_power_of_two();
        self.last_frame.allocation_growth_count += 1;
        self.visible_buffer = create_buffer(
            &self.device,
            "visual-authoring-visible-slots",
            (self.visible_capacity * size_of::<u32>()) as u64,
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        self.recreate_bind_group();
        self.metrics.buffer_bytes = (self.instance_capacity * size_of::<InstanceRaw>()
            + self.visible_capacity * size_of::<u32>()
            + size_of::<ViewRaw>()) as u64;
    }

    fn recreate_bind_group(&mut self) {
        self.bind_group = create_bind_group(
            &self.device,
            &self.bind_group_layout,
            &self.instance_buffer,
            &self.visible_buffer,
            &self.view_buffer,
        );
        self.metrics.buffer_bytes = (self.instance_capacity * size_of::<InstanceRaw>()
            + self.visible_capacity * size_of::<u32>()
            + size_of::<ViewRaw>()) as u64;
    }

    fn record_upload(&mut self, bytes: u64) {
        self.last_frame.upload_calls += 1;
        self.last_frame.upload_bytes += bytes;
    }

    fn finish_last_frame(&mut self) {
        self.last_frame.buffer_count = self.metrics.buffer_count;
        self.last_frame.buffer_bytes = self.metrics.buffer_bytes;
        self.metrics.upload_calls += self.last_frame.upload_calls;
        self.metrics.upload_bytes += self.last_frame.upload_bytes;
        self.metrics.dirty_instance_ranges += self.last_frame.dirty_instance_ranges;
        self.metrics.full_instance_buffer_uploads += self.last_frame.full_instance_buffer_uploads;
        self.metrics.draw_calls += self.last_frame.draw_calls;
        self.metrics.batches += self.last_frame.batches;
        self.metrics.submitted_instances += self.last_frame.submitted_instances;
        self.metrics.validation_errors += self.last_frame.validation_errors;
        self.metrics.render_submit_micros += self.last_frame.render_submit_micros;
        self.metrics.gpu_encode_attempted += self.last_frame.gpu_encode_attempted;
        self.metrics.gpu_encode_omitted += self.last_frame.gpu_encode_omitted;
        self.metrics.instance_upload_bytes += self.last_frame.instance_upload_bytes;
        self.metrics.visible_slot_upload_bytes += self.last_frame.visible_slot_upload_bytes;
        self.metrics.allocation_growth_count += self.last_frame.allocation_growth_count;
    }
}

fn storage_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn create_buffer(
    device: &wgpu::Device,
    label: &str,
    size: u64,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size.max(4),
        usage,
        mapped_at_creation: false,
    })
}

fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    instances: &wgpu::Buffer,
    visible: &wgpu::Buffer,
    view: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("visual-authoring-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: instances.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: visible.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: view.as_entire_binding(),
            },
        ],
    })
}

fn encode_instance(item: &RenderItem) -> Result<InstanceRaw, RendererError> {
    let world = item.world_transform.ok_or(RendererError::F32Conversion {
        node_id: item.node_id,
    })?;
    let (tx_hi, tx_lo) = split_f64(world.tx).ok_or(RendererError::F32Conversion {
        node_id: item.node_id,
    })?;
    let (ty_hi, ty_lo) = split_f64(world.ty).ok_or(RendererError::F32Conversion {
        node_id: item.node_id,
    })?;
    let values = [
        world.m11,
        world.m12,
        world.m21,
        world.m22,
        item.size.x,
        item.size.y,
        item.fill_linear[0],
        item.fill_linear[1],
        item.fill_linear[2],
        item.fill_linear[3],
        item.opacity,
        item.corner_radii[0],
        item.corner_radii[1],
        item.corner_radii[2],
        item.corner_radii[3],
        item.stroke_linear[0],
        item.stroke_linear[1],
        item.stroke_linear[2],
        item.stroke_linear[3],
        item.stroke_width,
    ];
    if values.iter().any(|value| !finite_f32(*value)) {
        return Err(RendererError::F32Conversion {
            node_id: item.node_id,
        });
    }
    Ok(InstanceRaw {
        linear: [
            world.m11 as f32,
            world.m12 as f32,
            world.m21 as f32,
            world.m22 as f32,
        ],
        translation_hi: [tx_hi, ty_hi],
        translation_lo: [tx_lo, ty_lo],
        size: [item.size.x as f32, item.size.y as f32],
        opacity: item.opacity as f32,
        primitive: u32::from(item.primitive == PrimitiveKind::Ellipse),
        fill_linear: item.fill_linear.map(|value| value as f32),
        corner_radii: item.corner_radii.map(|value| value as f32),
        stroke_linear: item.stroke_linear.map(|value| value as f32),
        stroke_width: item.stroke_width as f32,
        padding: [0.0; 3],
    })
}

fn encode_view(view: RenderView) -> Result<ViewRaw, RendererError> {
    if !view.center.is_finite()
        || !view.zoom.is_finite()
        || view.zoom <= 0.0
        || !view.viewport_size.is_finite()
        || view.viewport_size.x <= 0.0
        || view.viewport_size.y <= 0.0
        || !view.device_pixel_ratio.is_finite()
        || view.device_pixel_ratio <= 0.0
        || !finite_f32(view.zoom)
        || !finite_f32(view.viewport_size.x)
        || !finite_f32(view.viewport_size.y)
    {
        return Err(RendererError::InvalidView);
    }
    let (center_x_hi, center_x_lo) = split_f64(view.center.x).ok_or(RendererError::InvalidView)?;
    let (center_y_hi, center_y_lo) = split_f64(view.center.y).ok_or(RendererError::InvalidView)?;
    Ok(ViewRaw {
        center_hi: [center_x_hi, center_y_hi],
        center_lo: [center_x_lo, center_y_lo],
        viewport: [view.viewport_size.x as f32, view.viewport_size.y as f32],
        zoom: view.zoom as f32,
        padding: 0.0,
    })
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

fn coalesce_slots(slots: &BTreeSet<u32>) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    let mut iter = slots.iter().copied();
    let Some(mut start) = iter.next() else {
        return ranges;
    };
    let mut previous = start;
    for slot in iter {
        if slot == previous.saturating_add(1) {
            previous = slot;
        } else {
            ranges.push((start, previous - start + 1));
            start = slot;
            previous = slot;
        }
    }
    ranges.push((start, previous - start + 1));
    ranges
}

#[cfg(test)]
mod tests {
    use visual_authoring_core_math::{Affine2, Rect};
    use visual_authoring_document::NodeId;

    use super::*;

    fn item(transform: Affine2, primitive: PrimitiveKind) -> RenderItem {
        RenderItem {
            node_id: NodeId::new(),
            slot: 0,
            primitive,
            size: Vec2::new(100.0, 50.0),
            world_transform: Some(transform),
            world_bounds: Some(Rect::from_min_max(Vec2::ZERO, Vec2::new(100.0, 50.0))),
            fill_linear: [0.2, 0.4, 0.8, 1.0],
            opacity: 1.0,
            corner_radii: [8.0; 4],
            stroke_linear: [0.1, 0.1, 0.1, 1.0],
            stroke_width: 2.0,
            renderable: true,
            gpu_encodable: true,
            encoding_diagnostic: None,
        }
    }

    #[test]
    fn high_low_translation_split_preserves_large_world_offset() {
        let value = 1.0e15 + 0.125;
        let (high, low) = split_f64(value).unwrap();
        let reconstructed = f64::from(high) + f64::from(low);
        let split_error = (reconstructed - value).abs();
        let single_f32_error = (f64::from(value as f32) - value).abs();
        assert!(split_error <= 1.0);
        assert!(split_error < single_f32_error / 1_000_000.0);
    }

    #[test]
    fn ellipse_and_rectangle_have_distinct_shader_primitive_flags() {
        let rectangle =
            encode_instance(&item(Affine2::IDENTITY, PrimitiveKind::Rectangle)).unwrap();
        let ellipse = encode_instance(&item(Affine2::IDENTITY, PrimitiveKind::Ellipse)).unwrap();
        assert_eq!(rectangle.primitive, 0);
        assert_eq!(ellipse.primitive, 1);
        assert!(SHADER_SOURCE.contains("fwidth"));
        assert!(SHADER_SOURCE.contains("smoothstep"));
        assert!(!SHADER_SOURCE.contains("discard"));
        assert_eq!(size_of::<InstanceRaw>(), INSTANCE_STRIDE_BYTES);
    }

    #[test]
    fn non_finite_or_unrepresentable_values_fail_at_gpu_boundary() {
        let mut too_large = item(Affine2::IDENTITY, PrimitiveKind::Rectangle);
        too_large.size = Vec2::new(f64::MAX, 1.0);
        assert!(matches!(
            encode_instance(&too_large),
            Err(RendererError::F32Conversion { .. })
        ));
    }

    #[test]
    fn dirty_slots_are_coalesced_without_full_buffer_semantics() {
        let slots = [1, 2, 3, 7, 9, 10].into_iter().collect();
        assert_eq!(coalesce_slots(&slots), vec![(1, 3), (7, 1), (9, 2)]);
    }

    #[test]
    fn shared_shader_and_binary_schema_match_native_layout() {
        let schema = include_str!("../../../shared/render_binary_schema.json");
        assert_eq!(RENDER_BINARY_SCHEMA_VERSION, 2);
        assert_eq!(size_of::<InstanceRaw>(), INSTANCE_STRIDE_BYTES);
        assert_eq!(size_of::<ViewRaw>(), VIEW_UNIFORM_BYTES);
        assert_eq!(DIRTY_RECORD_STRIDE_BYTES, 4 + INSTANCE_STRIDE_BYTES);
        assert!(schema.contains("\"version\": 2"));
        assert!(schema.contains("\"instance_stride_bytes\": 112"));
        assert!(schema.contains("\"dirty_record_stride_bytes\": 116"));
        assert!(schema.contains("\"rectangle\": 0"));
        assert!(schema.contains("\"ellipse\": 1"));
        assert!(SHADER_SOURCE.contains("visible_slots[visible_index]"));
        assert!(SHADER_SOURCE.contains("input.primitive == 1u"));
    }
}

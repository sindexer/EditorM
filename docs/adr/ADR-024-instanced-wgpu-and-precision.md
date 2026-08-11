# ADR-024: Instanced wgpu Rendering and Precision Boundary

- Status: Accepted
- Date: 2026-08-09

## Context

Renderer replacement must preserve Document semantics, avoid one draw per node, distinguish
ellipse from rectangle geometry, and make the f64-to-f32 boundary explicit.

## Decision

The native renderer uses `wgpu 0.20.1`; the browser uses the WebGPU API with the same 48-byte
instance layout and WGSL behavior. One instanced triangle-list draw reads stable slots through
a visible-slot storage buffer. The fragment shader discards pixels outside ellipse geometry.
Rectangle and ellipse therefore do not share a rectangular visual result.

Document/Scene/RenderModel stay f64. At the renderer boundary all linear, size, opacity, and
view values are checked for finite f32 representation. World and camera translations use a
high/low f32 split and camera-relative subtraction. Unrepresentable data returns a typed
renderer/host error.

## Consequences

The 10,000-rectangle fixture renders in one batch and one draw call. Native proof used an
NVIDIA GeForce GTX 970 through Vulkan. Actual Chrome proof used the same physical GPU through
browser WebGPU; the Web API intentionally does not expose its implementation backend name.
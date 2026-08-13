# ADR-043: Analytic Primitive Coverage and Premultiplied Alpha

- Status: Implemented for Phase 1A review
- Date: 2026-08-14
- Follows: ADR-042

## Context

Binary fragment discard produced jagged ellipse edges and could not provide stable partial coverage across zoom and device-pixel-ratio changes. Fill, opacity, and stroke also require one explicit color and blend contract across native and browser WebGPU.

## Decision

The shared WGSL computes signed boundary functions for ellipse and normalized four-corner rounded rectangles. fwidth and smoothstep convert signed distance to analytic partial coverage; the shader contains no fragment discard.

UI colors are persistent sRGB. RenderModel converts RGB channels once to linear space. Alpha remains linear. The fragment shader combines fill alpha, independent object opacity, stroke alpha, and coverage, then returns linear premultiplied RGBA. Native and browser render pipelines both use One / OneMinusSrcAlpha blending for color and alpha.

Phase 1A stroke alignment is centered. Scene bounds expand by half the stroke width, and the vertex quad expands by the full stroke width so the exterior half is not clipped. Rounded radii are normalized against primitive dimensions before coverage evaluation.

## Consequences

Native and browser backends compile the same generated shader contract. Edge coverage is testable with pixel readback without MSAA being mandatory. MSAA may only be added later if actual rotated-corner evidence proves analytic coverage insufficient.

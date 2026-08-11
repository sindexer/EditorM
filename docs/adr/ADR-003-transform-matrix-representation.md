# ADR-003 - Transform and matrix representation

Status: Accepted  
Date: 2026-08-07

## Context

Professional nested editing requires translation, rotation, non-uniform scale, reflection,
composition, inversion, world/local conversion, and reparenting without reducing transforms
to x/y/width/height fields.

## Decision

Use a project-owned `Affine2` 2x3 affine matrix with six `f64` components. Points are column
vectors and composition is:

```text
WorldTransform = ParentWorldTransform * LocalTransform
```

Document coordinates are +X right and +Y down, so positive mathematical rotation appears
clockwise on screen. Matrix inversion returns `Option<Affine2>` and rejects non-finite,
singular, and scale-relative near-singular matrices. Zero scale remains representable; only
operations requiring an inverse fail.

## Alternatives considered

- Translation/rotation/scale component bags: convenient for inspectors but cannot represent
  every affine result of nested non-uniform scale and rotation without loss.
- A third-party math crate: capable, but a six-component implementation is small and keeps
  composition, precision, and failure semantics explicit.
- `f32`: suitable for future GPU payloads, but insufficient as document/editor truth.

## Why this decision

The representation is closed under affine composition, supports reflection, and preserves
the exact matrix produced when reparenting. Tests cover deterministic and property-generated
combined transforms.

## Tradeoffs

Inspector-friendly decomposition is deferred and may be non-unique, especially with flips
and skew-like composed matrices. The matrix remains the authoritative transform.

## Boundary impact

Only `core_math` owns matrix operations. `document` consumes them for semantic transforms
and geometry bounds. No camera, renderer, GPU, or browser type enters the math crate.

## Migration / reversibility

The six values form a stable affine boundary. A future math dependency can replace the
implementation if it preserves multiplication order and `f64` semantics.

## Future impact

The matrix supports future skew and pivot operations and can be wrapped by static/animated
property models without changing scene or renderer ownership.

# ADR-048: Aggregate World Transform and Screen-Space Rotation Sign

- Status: Implemented for Phase 1C review
- Date: 2026-08-22
- Follows: ADR-047

## Context

The original single-node rotation handler replaced the node angle with the pointer's absolute polar angle plus a fixed quarter turn. It did not retain the pointer-down angle or the starting transform. This made the result depend on handle placement, produced direction surprises, and could jump when crossing the 0/360-degree boundary. Single-node geometry resize also could not preserve the geometry of a multiple selection.

## Decision

`begin_transaction` captures every editable selected node's local transform and parent world transform. `transform_selection` accepts one finite, invertible, positive-determinant world matrix, converts it through each captured parent, and applies the resulting local transforms through the existing batched transaction path. Any batch failure rolls the whole gesture back.

Resize authors an oriented world matrix around the opposite handle, or the common center for Alt. Shift uses uniform scale. Scale that crosses zero, mirrors, becomes singular, or makes the aggregate box smaller than one world unit is rejected without mutation. A single rotated shape uses its oriented axes; a multiple selection uses its common world-aligned bounds.

Rotation stores the pointer-down polar angle and sends the normalized signed delta. In the screen coordinate system, a positive angle is clockwise, matching the affine matrix, overlay, and Inspector display. Shift rounds the delta to `π/12` radians. The Inspector is the only degree/radian conversion boundary.

Inspector multi-edit uses `command_batch`, which validates all typed commands and commits them as one atomic transaction and one undo step.

## Consequences

Move, resize, rotate, and Inspector edits share the runtime document and history as their single source of truth. Transform previews do not accumulate drift, nested parents remain correct, multi-node relative layout is preserved, and Worker errors cannot leave a partially transformed selection. The request additions do not change the render binary schema.

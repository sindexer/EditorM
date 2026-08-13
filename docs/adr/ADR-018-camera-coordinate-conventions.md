# ADR-018 - Camera coordinate conventions and DPR

Status: Accepted  
Date: 2026-08-08

## Decision

Camera is editor-session state owned by `EngineRuntime`. It stores a world-space center,
positive finite zoom, positive logical viewport size, and positive finite device-pixel ratio.
`WorldPoint`, `ViewportPoint`, and `DevicePoint` newtypes make spaces explicit.

Viewport center maps to Camera world center. +X is right and +Y is down. Pan receives a
viewport delta and moves Camera center by the opposite delta divided by zoom, so content
tracks the pointer. Zoom-around-pointer preserves the world point under that viewport point.
DPR is applied only by explicit viewport-to-device conversion. Fit-bounds uses logical
viewport padding and rejects empty/inverted bounds.

All inputs and results are checked for finiteness. Camera mutations never dispatch commands,
change history/revision, mutate node transforms, or enter Document serialization.


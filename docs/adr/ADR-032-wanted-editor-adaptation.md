# ADR-032: Wanted Design System Adaptation for Dense Editor Chrome

- Status: Accepted for Phase 0E
- Date: 2026-08-10

## Context

Wanted Design System is the visual authority, but its general product controls do not directly define a graphics-editor tool rail, virtual Layers tree, selection handles, numeric transform panel, or runtime debug surface.

## Decision

Representative Wanted Button, Textfield, Select, Checkbox, Switch, Segmented Control, Tab, Tooltip, Menu, and Card nodes were inspected through actual Figma design context before implementation. Verified colors, typography, spacing, radii, borders, and elevation are centralized as CSS aliases.

Editor controls compress common 40-48 px source heights to 28-36 px while retaining Wanted contrast, type rhythm, focus blue, state overlays, corner language, and spacing cadence. Editor-specific components are explicitly documented as adaptations. `lucide-react` 0.468 is the sole implementation icon dependency under ISC; it is not represented as a Wanted icon export or byte match.

## Consequences

- The product has one documented visual language rather than unrelated per-component styling.
- Dense editor extensions remain traceable to inspected tokens and state behavior.
- Component mapping and token provenance are independently reviewable in `docs/UI_COMPONENT_MAPPING.md` and `docs/UI_TOKEN_PROVENANCE.md`.

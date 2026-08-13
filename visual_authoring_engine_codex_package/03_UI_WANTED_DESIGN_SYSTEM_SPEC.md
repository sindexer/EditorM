# UI SPEC — WANTED DESIGN SYSTEM REFERENCE

## 1. Authority

The supplied Figma file:

`reference/wanted-design-system/Wanted Design System (Community).fig`

is the required visual and component reference for the web editor UI.

It is not merely inspiration. It is the baseline design system for editor chrome and application UI unless a specific professional editor interaction requires an extension.

The extracted `thumbnail.png` is included for quick visual identification only. The `.fig` source is authoritative.

## 2. Mandatory implementation intent

When UI implementation begins:

- inspect the Figma reference before writing UI code;
- identify reusable component families and visual tokens;
- derive the editor UI token layer from the reference rather than inventing a new palette or spacing system;
- preserve the recognizable Wanted visual language while adapting it to a dense professional graphics editor.

## 3. What to follow

Match the reference as closely as practical for:

- typography hierarchy;
- type scale relationships;
- font weights;
- spacing rhythm;
- grid/alignment logic;
- corner radius conventions;
- border treatment;
- surface elevation and separation;
- input/button/select patterns;
- icon sizing and alignment;
- default/hover/pressed/focus/disabled states;
- menus;
- dropdowns;
- tabs;
- cards/panels;
- form controls;
- labels and helper text;
- density and padding logic;
- component composition patterns.

## 4. What not to do

Do not:

- substitute Material Design defaults;
- substitute Ant Design defaults;
- substitute Bootstrap defaults;
- substitute Chakra defaults;
- substitute stock shadcn visual defaults without restyling;
- use a generic dark SaaS dashboard appearance;
- invent neon gradients or AI-tool styling unrelated to Wanted;
- copy Figma's visual UI wholesale when it conflicts with Wanted's system;
- use browser-default controls as final UI.

Using a headless behavior library is acceptable if the rendered presentation is restyled to the Wanted system.

## 5. Professional editor adaptation

Wanted Design System is the visual system; Figma/Photoshop/After Effects-class editors are interaction references.

Therefore:

- visual language: Wanted Design System;
- editor interaction model: professional visual authoring conventions;
- engine architecture: this package's architecture spec.

If Wanted lacks a specialized editor component, derive a new component from its tokens and patterns rather than introducing a second unrelated design language.

Examples requiring extension:

- Layers tree;
- transform inspector;
- numeric scrub fields;
- canvas toolbar;
- selection/transform overlay;
- zoom control;
- timeline controls later;
- color/stroke/effect inspectors later.

## 6. Component mapping requirement

Before Phase 0E implementation, create:

`docs/UI_COMPONENT_MAPPING.md`

It must map editor controls to reference components/tokens, for example:

| Editor UI | Wanted reference | Adaptation |
|---|---|---|
| Primary button | corresponding Wanted button | compact editor density |
| Inspector text field | Wanted input | numeric suffix/scrub behavior |
| Dropdown | Wanted select/menu | keyboard navigation retained |
| Toolbar icon button | derived button/icon style | 28-32px dense control |
| Panel tabs | Wanted tabs | editor panel width constraints |

Do not implement Phase 0E production UI until this mapping exists.

## 7. Token extraction requirement

Create a token representation from the reference where feasible:

- colors;
- typography;
- spacing;
- radii;
- borders;
- shadows/elevation;
- interactive state values.

Store the app-facing token representation in a centralized UI token layer. Avoid scattered literal values.

If the `.fig` cannot be programmatically parsed in the implementation environment, document that limitation and inspect the file through available Figma tooling/export. Do not guess token values and claim they came from the source.

## 8. Component-first rule

Do not style every screen independently. Establish reusable components and variants before composing editor panels.

Expected categories eventually include:

- buttons/icon buttons;
- text fields/numeric fields;
- selects/dropdowns;
- tabs;
- segmented controls;
- menus/context menus;
- tooltips;
- dialogs/popovers;
- switches/checkboxes;
- panel headers;
- tree rows;
- badges/status indicators.

## 9. Accessibility and keyboard behavior

Visual fidelity does not justify breaking accessibility.

Use semantic DOM for editor chrome where possible. Support:

- visible keyboard focus;
- keyboard navigation;
- accessible labels;
- sufficient hit targets;
- disabled states;
- predictable tab order.

Canvas-specific interactions may use custom input routing, but the surrounding UI should remain accessible.

## 10. Phase 0A note

Phase 0A does not implement UI. The Wanted design-system file is included now so later UI architecture is not allowed to drift into a different design system.

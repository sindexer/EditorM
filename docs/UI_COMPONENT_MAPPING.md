# Phase 0E UI Component Mapping

## Authority and inspection method

The visual authority is the supplied **Wanted Design System (Community)** source. The editor interaction model is a professional graphics editor, while persistent state and mutations remain owned by the Rust Worker runtime.

The mapping below was established before product UI implementation from:

- remote Figma file `xGUOJQFvJxpZYyvHP18lLw`;
- Figma pages `1 Theme` (`16222:137703`), `2 Element` (`16222:137704`), and `3 Component` (`16222:137705`);
- actual `get_design_context` results for Button, Textfield, Select, Checkbox, Switch, Segmented Control, Tab, Tooltip, and Menu nodes;
- the bundled `.fig`, `meta.json`, thumbnail, and the authoritative Wanted UI specification.

The Figma output is a reference, not code to paste. Phase 0E uses semantic React components and a centralized token layer. Dense editor extensions preserve Wanted shapes, contrast, type rhythm, focus treatment, and interaction states.

## Component mapping

| Editor UI | Wanted reference and evidence | Phase 0E adaptation | Required states and accessibility |
|---|---|---|---|
| App bar primary action | `Button/Button` `16215:37602`; representative variant `16215:37603` | Medium/Small density, 32-36 px height, primary blue only for the principal action | hover, pressed, focus-visible, disabled; semantic `button` |
| Secondary button | Button, `Outlined`, `Assistive` variants in `16215:37602` | Neutral outlined chrome for Group, Ungroup, Fit, and worker actions | hover overlay, pressed overlay, focus ring, disabled |
| Tool rail icon button | Button icon-only variants in `16215:37602`; Wanted Icon family in `1 Theme` | 32 px control with 18-20 px exact SVG asset; selected state uses primary/background blue | `aria-pressed`, tooltip, keyboard shortcut in accessible name |
| Text field | `Textinput/Textfield` `16215:31385`; normal `16215:31386`; focus `16215:31426` | 30 px dense inspector field derived from the 12 px-radius source, using an 8 px editor radius | label, helper/error text, focus-visible, invalid, disabled |
| Numeric transform field | Textfield variants above | Textfield composition with explicit unit suffix; commit on Enter/blur and rollback on Escape | numeric input semantics, labelled unit, invalid typed error |
| Select | `Select/Select` `16215:33116`; normal text render `16215:33117` | Dense 30 px select trigger plus Wanted Menu popover; no browser-default presentation | combobox/listbox roles, arrows, Enter, Escape, disabled |
| Tabs | `Tab/Tab` `16215:21806` | Underline/label pattern retained; compact panel tabs and showcase tabs | tablist/tab/tabpanel roles, roving focus |
| Segmented control | `Segmented Control/Segmented Control` `16215:35115` | 28-32 px tool-mode grouping; selected fill or outlined primary variant | radiogroup semantics, arrow-key navigation, disabled |
| Tooltip | `Tooltip/Tooltip` `16764:137783` | Small inverse surface, 6 px radius, shortcut hint beside label | delayed hover/focus opening; `role=tooltip` |
| Menu/context menu | `Menu/Menu` `16215:18469`; 8/12 px padding variants and radio/checkbox items in its subtree | 32 px editor rows, leading icon/check slot, shortcut column | menu roles, roving focus, Escape, disabled, checked |
| Popover | Menu surface composition and Wanted elevated background tokens | Non-modal inspector/showcase popover with 8-12 px radius and XSmall shadow | focus return, outside-click/Escape close |
| Checkbox | `Control/Checkbox` `16215:33897` | 16 px visual control in 24 px hit target for visibility options | checked, mixed, focus, disabled; native input retained |
| Switch | `Switch/Switch` `16215:34983` | Small switch variant for debug/settings, not used as a substitute for visibility icons | checked, focus, disabled; native checkbox switch semantics |
| Panel header | Derived from Wanted Card/List Cell and Tab composition | 36 px header, neutral label, optional collapse and actions | heading semantics, labelled collapse button |
| Layers tree row | Derived from Wanted List Cell selection variants and Menu item density | 28 px virtualized row; indentation, disclosure, type icon, name, visibility, lock | `tree`/`treeitem`, `aria-level`, selected/expanded, keyboard navigation |
| Status badge | Wanted Badge/Chip families and `Chip/Chip` `16215:42078` | XSmall status pill for WebGPU, Worker, sequence, and fixture status | text is never color-only; status/live-region use is restrained |
| Inspector/card section | `Card/Card` `16215:29264`, `Card/List Card` `16215:29433` | Flat professional panel with line separation; shadows reserved for floating surfaces | grouped fieldsets and headings |
| Error state | Textfield negative state family and status negative token | Inline typed error callout; no silent fallback | `role=alert`, actionable recovery copy |
| Empty state | Card/panel composition with alternative label | Compact instructional state for no selection | descriptive text, no disabled fake controls |
| Loading state | Button loading variants in `16215:37602` | Spinner only for bounded async operations; editor remains structurally stable | busy labelling and preserved button width |
| Selection overlay | Editor-specific extension derived from primary blue, radii, and 18-20 px icon rhythm | DOM/SVG overlay above actual WebGPU canvas; never a Document node | pointer handles, focusable canvas, visible tool state |
| Debug panel | Card/List Cell, Badge, Tabs, and Switch composition | Collapsible metric grid; mounted-row and projection counters exposed | disclosure semantics and copyable values |
| Component showcase | All mapped primitives | Dedicated in-product audit surface showing default/hover/focus/pressed/disabled | keyboard reachable; stable selectors for browser proof |

## Editor-specific extensions

Wanted does not define a graphics-editor Layers tree, transform overlay, numeric transform inspector, or zoom/status instrumentation. These are extensions, not a second design language:

- spacing uses a 4 px base and Wanted-derived 6/8/10/12 px gaps;
- component heights are compressed to 28/30/32/36 px while retaining Wanted type, borders, focus blue, and state overlays;
- selected tools and overlays use `Primary/Normal`; destructive or invalid states use `Status/Negative`;
- floating menus/tooltips use Wanted elevated/inverse backgrounds and shadows;
- persistent interactions dispatch stable-`NodeId` typed requests to the Worker. React does not own a second node graph.

## Icon system

Phase 0E uses a single `lucide-react` 0.468 icon system under the ISC license. It is an implementation dependency, not an export or byte match of the Wanted Icon family. Icon size, alignment, neutral color, selected color, button geometry, and interaction states are adapted from the inspected Wanted components. Icons render through one React icon path with explicit 16/18/20/24 px square sizing. Hand-authored SVG paths, emoji controls, font icons, and mixed icon libraries are prohibited. Decorative icons use empty alternative text; labelled controls expose an accessible name on the button.

## Interaction density

The Wanted source commonly uses 40-48 px product controls. The professional editor adaptation uses smaller chrome while keeping a minimum 28 px pointer target and an accessible keyboard route. Canvas handles may be visually smaller but receive an expanded hit region. Focus indicators are never removed.

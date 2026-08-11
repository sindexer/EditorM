# Phase 0E UI Token Provenance

## Sources

| Source | Identity |
|---|---|
| Remote Figma | `https://www.figma.com/design/xGUOJQFvJxpZYyvHP18lLw/Wanted-Design-System--Community-` |
| Figma file key | `xGUOJQFvJxpZYyvHP18lLw` |
| Local `.fig` | `visual_authoring_engine_codex_package/reference/wanted-design-system/Wanted Design System (Community).fig` — 46,381,631 bytes — SHA-256 `d6f87f906ef2cbf211cae4b7bfe92429c064a4d783529ff14386b0a809fe7887` |
| Local metadata | `meta.json` — 336 bytes — SHA-256 `55ecc90727f2d3088ea2fce879f97935584bc17e01b918a8bc9c9cacddc9f0c2` |
| Local thumbnail | `thumbnail.png` — 19,320 bytes — SHA-256 `3d829ddb689acb82f824fda9ae0d29d962094c5a2f62d2b8b90f4008b17786e0` |
| Local export metadata | file name `Wanted Design System (Community)`, exported `2026-08-07T05:56:03.694Z`, render area 3328 × 6005 |

The `.fig` is a ZIP container whose `canvas.fig` payload uses Figma's `fig-kiwi` binary representation. It was not treated as a text token source. The remote Figma metadata and design-context responses provide the verifiable node and token values; local hashes establish that the supplied reference was not altered.

## Inspected Figma evidence

| Family | Node evidence |
|---|---|
| Button | `16215:37602`, representative `16215:37603` |
| Textfield | `16215:31385`, normal `16215:31386`, focus `16215:31426` |
| Select | `16215:33116`, normal `16215:33117` |
| Checkbox | `16215:33897` |
| Switch | `16215:34983` |
| Segmented control | `16215:35115` |
| Tab | `16215:21806` |
| Tooltip | `16764:137783` |
| Menu | `16215:18469` and its 8/12 px item variants |
| Card | `16215:29264`, `16215:29433` |
| Icon family | Theme page section `16215:13694` |

## Color tokens

The app-facing CSS token names are aliases. Source names and values below came from Figma variable definitions or design-context CSS variables; translucent values keep their alpha.

| App token | Wanted source | Verified value | Usage |
|---|---|---:|---|
| `--ui-primary` | `Primary/Normal` | `#0066ff` | selected tool, primary action, focus |
| `--ui-label-normal` | `Label/Normal` | `#171719` | primary text |
| `--ui-label-neutral` | `Label/Neutral` | `#2e2f33e0` | editor labels |
| `--ui-label-alternative` | `Label/Alternative` | `#37383c9c` | secondary text |
| `--ui-label-assistive` | `Label/Assistive` | `#37383c47` | placeholder/disabled helper |
| `--ui-line-neutral` | `Line/Normal/Neutral` | `#70737c29` | fields, dividers, panels |
| `--ui-surface` | `Background/Transparent/Normal` semantic base | `#ffffff` with component alpha where specified | panels and fields |
| `--ui-surface-elevated` | `Background/Elevated/Normal` | source variable used by focus/segmented variants | floating controls |
| `--ui-disable` | `Interaction/Disable` | `#f4f4f5` | disabled button surface |
| `--ui-negative` | `Status/Negative` | `#ff4242` | typed validation/error |
| `--ui-inverse-bg` | `Inverse/Background` | `#1b1c1e` | tooltip/menu inverse surface |
| `--ui-inverse-label` | `Inverse/Label` | `#f7f7f8` | tooltip label |
| `--ui-static-white` | `Static/White` | `#ffffff` | primary button content |
| `--ui-canvas-bg` | local metadata background and editor adaptation | `#f5f5f5` | canvas surround; not a Document color |

## Typography

| App role | Wanted source | Verified definition | Editor adaptation |
|---|---|---|---|
| UI body | `Body 1/Normal - Regular` | Pretendard JP Regular, 16 px, line-height 1.5 | 13 px / 18 px for dense panels |
| UI label | `Label 1/Normal - Bold` | Pretendard JP SemiBold, 14 px, line-height 1.429 | 12 px / 16 px for field labels |
| Button large | Button representative variant | Pretendard JP SemiBold, 16 px, 1.5 | 13-14 px in editor buttons |
| Button medium | Button representative variant | Pretendard JP SemiBold, 15 px, 1.467 | 13 px in editor chrome |
| Field text | Textfield/Select contexts | Pretendard JP Regular/Medium/SemiBold | 12-13 px, tabular numbers for transforms |

The CSS stack uses locally available `Pretendard`, `Pretendard JP`, `Inter`, and system sans-serif fallbacks. The project does not download a font during build or runtime.

## Spacing and density

The Figma variants expose recurring gaps and padding of 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 14, 20, and 28 px. Phase 0E centralizes a compact subset:

- base: 4 px;
- tight: 2/4/6 px;
- control inset: 8/10/12 px;
- panel inset: 12/16 px;
- normal editor controls: 28/30/32/36 px;
- icon sizes: 16/18/20/24 px.

No component may introduce unrelated one-off spacing without recording an editor-specific extension.

## Radii, borders, and elevation

| App token | Figma evidence | Value/adaptation |
|---|---|---|
| `--radius-xs` | Tooltip | 6 px |
| `--radius-sm` | Segmented variants | 8 px |
| `--radius-md` | Button medium / segmented | 10 px |
| `--radius-lg` | Button large, Textfield, Select | 12 px |
| `--radius-pill` | Switch/Tab controls | 1000 px |
| `--border-normal` | `Line/Normal/Neutral` | 1 px solid `#70737c29` |
| `--shadow-xsmall` | `Shadow/Normal/Xsmall` from Select variables | `0 1px 2px -1px #1717171a` |
| `--shadow-popover` | editor extension from Wanted XSmall shadow rhythm | layered subtle neutral shadow, no glow |

## Interaction states

- hover/pressed: a `Label/Normal` overlay with low opacity, matching the Button interaction layer;
- focus: 1-2 px `Primary/Normal` ring, matching Textfield focus node `16215:31426`;
- disabled: `Interaction/Disable` surface plus `Label/Assistive` content;
- selected: primary blue foreground or pale primary background, as in Segmented Control;
- invalid: `Status/Negative` border/helper text;
- locked/hidden: alternative/assistive labels plus an explicit icon and accessible state.

## Provenance limits

Remote Figma access succeeded and representative design context was captured. A later batch of variable-definition calls reached the Figma Starter plan call limit; values not returned by that endpoint are recorded only when present in the successful design-context output. No unavailable token was guessed and labelled as a source value. The centralized CSS layer distinguishes verified Wanted values from professional-editor adaptations.

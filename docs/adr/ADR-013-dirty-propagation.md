# ADR-013 - Dirty categories and propagation

Status: Accepted  
Date: 2026-08-08

## Decision

Scene records track Transform, Geometry, Appearance, Hierarchy, and Visibility dirty meaning
with the revision that last changed them.

| Change | Descendants | Ancestors | Spatial |
|---|---|---|---|
| Local transform | recompute world/bounds | update aggregate until stable | update affected geometry |
| Geometry | target bounds only | update aggregate until stable | update target |
| Visibility | effective visibility only | none | insert/remove subtree entries |
| Hierarchy | refresh moved subtree | old/new aggregate chains | insert/remove/update subtree |
| Appearance | mark target | none | none |
| Name/metadata/locked | none | none | none |

Propagation uses iterative traversal. Transaction commit performs no derived work after its
preview updates. Same-parent reorder updates structural child order without recomputing world
transforms or dense global order keys.

## Performance invariant

No normal change path rebuilds the full Scene. A leaf update touches the leaf and necessary
ancestor aggregate records; unrelated siblings are not revisited. Full traversal is reserved
for initial build, explicit replacement, or measured recovery fallback.


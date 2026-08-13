//! Production R*-tree spatial index used by scene queries and viewport culling.

use std::collections::{BTreeMap, BTreeSet};

use rstar::{RTree, RTreeObject, AABB};
use visual_authoring_core_math::{Rect, Vec2};
use visual_authoring_document::NodeId;

use crate::{SpatialError, SpatialIndex, SpatialQuery};

#[derive(Clone, Copy, Debug, PartialEq)]
struct SpatialRecord {
    id: NodeId,
    bounds: Rect,
}

impl RTreeObject for SpatialRecord {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        envelope(self.bounds)
    }
}

/// Dynamic R*-tree over finite world-space AABBs.
///
/// Entries of every size share the same hierarchy; there is no overflow bucket and wide
/// queries walk the tree instead of enumerating a fixed grid.
#[derive(Clone, Debug)]
pub struct RTreeIndex {
    tree: RTree<SpatialRecord>,
    entries: BTreeMap<NodeId, Rect>,
}

impl Default for RTreeIndex {
    fn default() -> Self {
        Self {
            tree: RTree::new(),
            entries: BTreeMap::new(),
        }
    }
}

impl PartialEq for RTreeIndex {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}

impl RTreeIndex {
    pub const IMPLEMENTATION: &'static str = "rstar-r*-tree";

    #[must_use]
    pub fn bounds(&self, id: NodeId) -> Option<Rect> {
        self.entries.get(&id).copied()
    }

    fn record(id: NodeId, bounds: Rect) -> SpatialRecord {
        SpatialRecord { id, bounds }
    }

    fn validate_bounds(id: NodeId, bounds: Rect) -> Result<(), SpatialError> {
        if !valid_rect(bounds) {
            return Err(SpatialError::InvalidBounds(id));
        }
        Ok(())
    }

    fn query_envelope(&self, bounds: Rect) -> SpatialQuery {
        let ids = self
            .tree
            .locate_in_envelope_intersecting(&envelope(bounds))
            .filter(|record| intersects(record.bounds, bounds))
            .map(|record| record.id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        SpatialQuery { ids }
    }
}

impl SpatialIndex for RTreeIndex {
    fn insert(&mut self, id: NodeId, bounds: Rect) -> Result<(), SpatialError> {
        Self::validate_bounds(id, bounds)?;
        if self.entries.contains_key(&id) {
            return Err(SpatialError::DuplicateEntry(id));
        }
        self.tree.insert(Self::record(id, bounds));
        self.entries.insert(id, bounds);
        Ok(())
    }

    fn remove(&mut self, id: NodeId) -> Result<(), SpatialError> {
        let bounds = self
            .entries
            .get(&id)
            .copied()
            .ok_or(SpatialError::MissingEntry(id))?;
        let record = Self::record(id, bounds);
        if self.tree.remove(&record).is_none() {
            return Err(SpatialError::IndexCorrupted(id));
        }
        self.entries.remove(&id);
        Ok(())
    }

    fn update(&mut self, id: NodeId, bounds: Rect) -> Result<(), SpatialError> {
        Self::validate_bounds(id, bounds)?;
        let previous = self
            .entries
            .get(&id)
            .copied()
            .ok_or(SpatialError::MissingEntry(id))?;
        let previous_record = Self::record(id, previous);
        if self.tree.remove(&previous_record).is_none() {
            return Err(SpatialError::IndexCorrupted(id));
        }
        self.tree.insert(Self::record(id, bounds));
        self.entries.insert(id, bounds);
        Ok(())
    }

    fn query_point(&self, point: Vec2) -> Result<SpatialQuery, SpatialError> {
        if !point.is_finite() {
            return Err(SpatialError::CoordinateOutOfRange);
        }
        let point_bounds = Rect::from_min_max(point, point);
        Ok(self.query_envelope(point_bounds))
    }

    fn query_rect(&self, bounds: Rect) -> Result<SpatialQuery, SpatialError> {
        if !valid_rect(bounds) {
            return Err(SpatialError::InvalidQueryBounds);
        }
        Ok(self.query_envelope(bounds))
    }

    fn clear(&mut self) {
        self.tree = RTree::new();
        self.entries.clear();
    }

    fn rebuild(&mut self, entries: &[(NodeId, Rect)]) -> Result<(), SpatialError> {
        let mut unique = BTreeMap::new();
        let mut records = Vec::with_capacity(entries.len());
        for (id, bounds) in entries {
            Self::validate_bounds(*id, *bounds)?;
            if unique.insert(*id, *bounds).is_some() {
                return Err(SpatialError::DuplicateEntry(*id));
            }
            records.push(Self::record(*id, *bounds));
        }
        let rebuilt = Self {
            tree: RTree::bulk_load(records),
            entries: unique,
        };
        *self = rebuilt;
        Ok(())
    }

    fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

fn envelope(bounds: Rect) -> AABB<[f64; 2]> {
    AABB::from_corners([bounds.min.x, bounds.min.y], [bounds.max.x, bounds.max.y])
}

fn valid_rect(bounds: Rect) -> bool {
    bounds.is_finite() && bounds.min.x <= bounds.max.x && bounds.min.y <= bounds.max.y
}

fn intersects(left: Rect, right: Rect) -> bool {
    left.min.x <= right.max.x
        && left.max.x >= right.min.x
        && left.min.y <= right.max.y
        && left.max.y >= right.min.y
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn id(value: u128) -> NodeId {
        NodeId::from_uuid(Uuid::from_u128(value))
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect::from_min_max(Vec2::new(x, y), Vec2::new(x + width, y + height))
    }

    fn oracle(entries: &BTreeMap<NodeId, Rect>, query: Rect) -> Vec<NodeId> {
        entries
            .iter()
            .filter_map(|(id, bounds)| intersects(*bounds, query).then_some(*id))
            .collect()
    }

    #[test]
    fn mixed_sizes_dense_and_large_geometry_share_one_tree() {
        let entries = [
            (id(1), rect(0.0, 0.0, 1.0, 1.0)),
            (id(2), rect(0.25, 0.25, 100.0, 100.0)),
            (id(3), rect(-1.0e9, -1.0e9, 2.0e9, 2.0e9)),
            (id(4), rect(50_000.0, 50_000.0, 2.0, 2.0)),
        ];
        let mut index = RTreeIndex::default();
        index.rebuild(&entries).unwrap();

        assert_eq!(
            index.query_rect(rect(0.0, 0.0, 2.0, 2.0)).unwrap().ids(),
            &[id(1), id(2), id(3)]
        );
        assert_eq!(
            index
                .query_rect(rect(50_000.0, 50_000.0, 1.0, 1.0))
                .unwrap()
                .ids(),
            &[id(3), id(4)]
        );
        assert_eq!(index.entry_count(), entries.len());
    }

    #[test]
    fn sparse_one_hundred_thousand_fixture_prunes_narrow_query() {
        let entries: Vec<_> = (1..=100_000_u128)
            .map(|value| {
                let zero = value - 1;
                let x = (zero % 1_000) as f64 * 100.0;
                let y = (zero / 1_000) as f64 * 100.0;
                (id(value), rect(x, y, 4.0, 4.0))
            })
            .collect();
        let mut index = RTreeIndex::default();
        index.rebuild(&entries).unwrap();

        let result = index.query_rect(rect(32_100.0, 54_00.0, 4.0, 4.0)).unwrap();
        assert!(result.candidate_count() < entries.len());
        assert_eq!(result.candidate_count(), 1);
    }

    #[test]
    fn extreme_zoom_out_returns_all_visible_entries_without_query_too_large() {
        let mut index = RTreeIndex::default();
        for value in 1..=1_000_u128 {
            index
                .insert(
                    id(value),
                    rect(value as f64 * 1.0e6, -(value as f64) * 1.0e6, 10.0, 10.0),
                )
                .unwrap();
        }
        let result = index
            .query_rect(rect(-1.0e15, -1.0e15, 2.0e15, 2.0e15))
            .unwrap();
        assert_eq!(result.candidate_count(), 1_000);
    }

    #[test]
    fn repeated_insert_update_remove_matches_brute_force_oracle() {
        let mut index = RTreeIndex::default();
        let mut entries = BTreeMap::new();
        for value in 1..=500_u128 {
            let bounds = rect(value as f64 * 7.0, value as f64 * 3.0, 5.0, 9.0);
            index.insert(id(value), bounds).unwrap();
            entries.insert(id(value), bounds);
        }

        for step in 0..1_000_u128 {
            let value = step % 500 + 1;
            let node = id(value);
            if step % 5 == 0 {
                index.remove(node).unwrap();
                entries.remove(&node);
                let replacement = rect(
                    -(step as f64) * 2.0,
                    step as f64 * 1.5,
                    3.0 + (step % 11) as f64,
                    7.0,
                );
                index.insert(node, replacement).unwrap();
                entries.insert(node, replacement);
            } else {
                let updated = rect(
                    value as f64 * 2.0 - step as f64,
                    value as f64 + step as f64 * 0.25,
                    4.0,
                    6.0 + (step % 7) as f64,
                );
                index.update(node, updated).unwrap();
                entries.insert(node, updated);
            }

            if step % 17 == 0 {
                let query = rect(
                    -(step as f64),
                    step as f64 * 0.1,
                    800.0 + step as f64,
                    500.0,
                );
                assert_eq!(
                    index.query_rect(query).unwrap().ids(),
                    oracle(&entries, query)
                );
            }
        }
    }

    #[test]
    fn rebuild_and_mutation_errors_are_atomic() {
        let mut index = RTreeIndex::default();
        index.insert(id(1), rect(0.0, 0.0, 10.0, 10.0)).unwrap();
        let before = index.clone();

        assert_eq!(
            index.insert(id(1), rect(1.0, 1.0, 2.0, 2.0)),
            Err(SpatialError::DuplicateEntry(id(1)))
        );
        assert_eq!(index, before);
        assert!(matches!(
            index.update(
                id(1),
                Rect::from_min_max(Vec2::ZERO, Vec2::new(f64::INFINITY, 1.0))
            ),
            Err(SpatialError::InvalidBounds(_))
        ));
        assert_eq!(index, before);

        let invalid = [
            (id(2), rect(0.0, 0.0, 1.0, 1.0)),
            (id(2), rect(2.0, 2.0, 1.0, 1.0)),
        ];
        assert_eq!(
            index.rebuild(&invalid),
            Err(SpatialError::DuplicateEntry(id(2)))
        );
        assert_eq!(index, before);
    }

    #[test]
    fn non_finite_queries_remain_typed_errors() {
        let index = RTreeIndex::default();
        assert_eq!(
            index.query_rect(Rect::from_min_max(
                Vec2::ZERO,
                Vec2::new(f64::INFINITY, 1.0)
            )),
            Err(SpatialError::InvalidQueryBounds)
        );
        assert_eq!(
            index.query_point(Vec2::new(f64::NAN, 0.0)),
            Err(SpatialError::CoordinateOutOfRange)
        );
    }
}

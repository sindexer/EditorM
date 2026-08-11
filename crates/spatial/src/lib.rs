//! Replaceable spatial-query acceleration. This crate stores only NodeId/AABB cache data.

mod rtree;
pub use rtree::RTreeIndex;

use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;
use visual_authoring_core_math::{Rect, Vec2};
use visual_authoring_document::NodeId;

const DEFAULT_CELL_SIZE: f64 = 256.0;
const MAX_CELLS_PER_ENTRY: u128 = 4_096;
const MAX_QUERY_CELLS: u128 = 1_000_000;

#[derive(Clone, Debug, Error, PartialEq)]
pub enum SpatialError {
    #[error("cell size must be finite and greater than zero")]
    InvalidCellSize,
    #[error("bounds for node {0} are non-finite or inverted")]
    InvalidBounds(NodeId),
    #[error("query bounds are non-finite or inverted")]
    InvalidQueryBounds,
    #[error("spatial entry {0} already exists")]
    DuplicateEntry(NodeId),
    #[error("spatial entry {0} does not exist")]
    MissingEntry(NodeId),
    #[error("spatial index lost its record for node {0}")]
    IndexCorrupted(NodeId),
    #[error("coordinate lies outside the supported grid range")]
    CoordinateOutOfRange,
    #[error("rectangle query covers too many grid cells")]
    QueryTooLarge,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SpatialQuery {
    ids: Vec<NodeId>,
}

impl SpatialQuery {
    #[must_use]
    pub fn ids(&self) -> &[NodeId] {
        &self.ids
    }

    #[must_use]
    pub fn candidate_count(&self) -> usize {
        self.ids.len()
    }
}

/// A replaceable AABB index boundary. Implementations never own document semantics.
pub trait SpatialIndex {
    fn insert(&mut self, id: NodeId, bounds: Rect) -> Result<(), SpatialError>;
    fn remove(&mut self, id: NodeId) -> Result<(), SpatialError>;
    fn update(&mut self, id: NodeId, bounds: Rect) -> Result<(), SpatialError>;
    fn query_point(&self, point: Vec2) -> Result<SpatialQuery, SpatialError>;
    fn query_rect(&self, bounds: Rect) -> Result<SpatialQuery, SpatialError>;
    fn clear(&mut self);
    fn rebuild(&mut self, entries: &[(NodeId, Rect)]) -> Result<(), SpatialError>;
    fn entry_count(&self) -> usize;
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Cell {
    x: i64,
    y: i64,
}

#[derive(Clone, Debug, PartialEq)]
struct StoredEntry {
    bounds: Rect,
    cells: Vec<Cell>,
    overflow: bool,
}

/// Deterministic uniform-grid index with an overflow bucket for very large entries.
#[derive(Clone, Debug, PartialEq)]
pub struct UniformGridIndex {
    cell_size: f64,
    cells: BTreeMap<Cell, BTreeSet<NodeId>>,
    overflow: BTreeSet<NodeId>,
    entries: BTreeMap<NodeId, StoredEntry>,
}

impl Default for UniformGridIndex {
    fn default() -> Self {
        Self::new(DEFAULT_CELL_SIZE).expect("the default spatial cell size is valid")
    }
}

impl UniformGridIndex {
    pub fn new(cell_size: f64) -> Result<Self, SpatialError> {
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return Err(SpatialError::InvalidCellSize);
        }
        Ok(Self {
            cell_size,
            cells: BTreeMap::new(),
            overflow: BTreeSet::new(),
            entries: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn bounds(&self, id: NodeId) -> Option<Rect> {
        self.entries.get(&id).map(|entry| entry.bounds)
    }

    fn validate_bounds(id: NodeId, bounds: Rect) -> Result<(), SpatialError> {
        if !bounds.is_finite() || bounds.min.x > bounds.max.x || bounds.min.y > bounds.max.y {
            return Err(SpatialError::InvalidBounds(id));
        }
        Ok(())
    }

    fn scalar_cell(&self, value: f64) -> Result<i64, SpatialError> {
        let cell = (value / self.cell_size).floor();
        if !cell.is_finite() {
            return Err(SpatialError::CoordinateOutOfRange);
        }
        if cell < i64::MIN as f64 {
            return Ok(i64::MIN);
        }
        if cell >= i64::MAX as f64 {
            return Ok(i64::MAX);
        }
        Ok(cell as i64)
    }

    fn cell_range(&self, bounds: Rect) -> Result<(Cell, Cell, u128), SpatialError> {
        let min = Cell {
            x: self.scalar_cell(bounds.min.x)?,
            y: self.scalar_cell(bounds.min.y)?,
        };
        let max = Cell {
            x: self.scalar_cell(bounds.max.x)?,
            y: self.scalar_cell(bounds.max.y)?,
        };
        let width = (i128::from(max.x) - i128::from(min.x) + 1) as u128;
        let height = (i128::from(max.y) - i128::from(min.y) + 1) as u128;
        Ok((min, max, width.saturating_mul(height)))
    }

    fn enumerate_cells(min: Cell, max: Cell) -> Vec<Cell> {
        let mut cells = Vec::new();
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                cells.push(Cell { x, y });
            }
        }
        cells
    }

    fn remove_stored(&mut self, id: NodeId, entry: &StoredEntry) {
        if entry.overflow {
            self.overflow.remove(&id);
        }
        let mut empty = Vec::new();
        for cell in &entry.cells {
            if let Some(ids) = self.cells.get_mut(cell) {
                ids.remove(&id);
                if ids.is_empty() {
                    empty.push(*cell);
                }
            }
        }
        for cell in empty {
            self.cells.remove(&cell);
        }
    }

    fn candidate_ids_for_cells(&self, cells: impl IntoIterator<Item = Cell>) -> BTreeSet<NodeId> {
        let mut candidates = self.overflow.clone();
        for cell in cells {
            if let Some(ids) = self.cells.get(&cell) {
                candidates.extend(ids.iter().copied());
            }
        }
        candidates
    }
}

impl SpatialIndex for UniformGridIndex {
    fn insert(&mut self, id: NodeId, bounds: Rect) -> Result<(), SpatialError> {
        Self::validate_bounds(id, bounds)?;
        if self.entries.contains_key(&id) {
            return Err(SpatialError::DuplicateEntry(id));
        }
        let (min, max, count) = self.cell_range(bounds)?;
        let overflow = count > MAX_CELLS_PER_ENTRY;
        let cells = if overflow {
            Vec::new()
        } else {
            Self::enumerate_cells(min, max)
        };
        if overflow {
            self.overflow.insert(id);
        } else {
            for cell in &cells {
                self.cells.entry(*cell).or_default().insert(id);
            }
        }
        self.entries.insert(
            id,
            StoredEntry {
                bounds,
                cells,
                overflow,
            },
        );
        Ok(())
    }

    fn remove(&mut self, id: NodeId) -> Result<(), SpatialError> {
        let entry = self
            .entries
            .remove(&id)
            .ok_or(SpatialError::MissingEntry(id))?;
        self.remove_stored(id, &entry);
        Ok(())
    }

    fn update(&mut self, id: NodeId, bounds: Rect) -> Result<(), SpatialError> {
        Self::validate_bounds(id, bounds)?;
        let previous = self
            .entries
            .remove(&id)
            .ok_or(SpatialError::MissingEntry(id))?;
        self.remove_stored(id, &previous);
        if let Err(error) = self.insert(id, bounds) {
            let _ = self.insert(id, previous.bounds);
            return Err(error);
        }
        Ok(())
    }

    fn query_point(&self, point: Vec2) -> Result<SpatialQuery, SpatialError> {
        if !point.is_finite() {
            return Err(SpatialError::CoordinateOutOfRange);
        }
        let cell = Cell {
            x: self.scalar_cell(point.x)?,
            y: self.scalar_cell(point.y)?,
        };
        let ids = self
            .candidate_ids_for_cells([cell])
            .into_iter()
            .filter(|id| contains(self.entries[id].bounds, point))
            .collect();
        Ok(SpatialQuery { ids })
    }

    fn query_rect(&self, bounds: Rect) -> Result<SpatialQuery, SpatialError> {
        if !bounds.is_finite() || bounds.min.x > bounds.max.x || bounds.min.y > bounds.max.y {
            return Err(SpatialError::InvalidQueryBounds);
        }
        let (min, max, count) = self.cell_range(bounds)?;
        if count > MAX_QUERY_CELLS {
            return Err(SpatialError::QueryTooLarge);
        }
        let ids = self
            .candidate_ids_for_cells(Self::enumerate_cells(min, max))
            .into_iter()
            .filter(|id| intersects(self.entries[id].bounds, bounds))
            .collect();
        Ok(SpatialQuery { ids })
    }

    fn clear(&mut self) {
        self.cells.clear();
        self.overflow.clear();
        self.entries.clear();
    }

    fn rebuild(&mut self, entries: &[(NodeId, Rect)]) -> Result<(), SpatialError> {
        let mut rebuilt = Self::new(self.cell_size)?;
        for (id, bounds) in entries {
            rebuilt.insert(*id, *bounds)?;
        }
        *self = rebuilt;
        Ok(())
    }

    fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

fn contains(bounds: Rect, point: Vec2) -> bool {
    point.x >= bounds.min.x
        && point.x <= bounds.max.x
        && point.y >= bounds.min.y
        && point.y <= bounds.max.y
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

    #[test]
    fn insert_update_remove_and_count_are_exact() {
        let mut index = UniformGridIndex::default();
        let node = id(1);
        index
            .insert(node, Rect::from_min_max(Vec2::ZERO, Vec2::new(10.0, 10.0)))
            .unwrap();
        assert_eq!(index.entry_count(), 1);
        index
            .update(
                node,
                Rect::from_min_max(Vec2::new(500.0, 0.0), Vec2::new(510.0, 10.0)),
            )
            .unwrap();
        assert!(index
            .query_point(Vec2::new(5.0, 5.0))
            .unwrap()
            .ids()
            .is_empty());
        assert_eq!(
            index.query_point(Vec2::new(505.0, 5.0)).unwrap().ids(),
            &[node]
        );
        index.remove(node).unwrap();
        assert_eq!(index.entry_count(), 0);
    }

    #[test]
    fn rectangle_query_returns_only_intersecting_aabb_candidates() {
        let mut index = UniformGridIndex::default();
        index
            .insert(
                id(1),
                Rect::from_min_max(Vec2::new(10.0, 10.0), Vec2::new(20.0, 20.0)),
            )
            .unwrap();
        index
            .insert(
                id(2),
                Rect::from_min_max(Vec2::new(300.0, 10.0), Vec2::new(320.0, 20.0)),
            )
            .unwrap();
        let result = index
            .query_rect(Rect::from_min_max(Vec2::ZERO, Vec2::new(30.0, 30.0)))
            .unwrap();
        assert_eq!(result.ids(), &[id(1)]);
    }

    #[test]
    fn randomized_queries_match_test_only_brute_force_oracle() {
        let mut index = UniformGridIndex::new(32.0).unwrap();
        let mut entries = Vec::new();
        let mut seed = 0x1234_5678_u64;
        for value in 1..=500_u128 {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let x = ((seed >> 16) % 10_000) as f64 - 5_000.0;
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let y = ((seed >> 16) % 10_000) as f64 - 5_000.0;
            let bounds = Rect::from_min_max(Vec2::new(x, y), Vec2::new(x + 8.0, y + 8.0));
            index.insert(id(value), bounds).unwrap();
            entries.push((id(value), bounds));
        }
        for query_index in 0..100_u64 {
            let point = Vec2::new(
                query_index as f64 * 97.0 - 4_500.0,
                query_index as f64 * 41.0 - 2_000.0,
            );
            let indexed = index.query_point(point).unwrap().ids().to_vec();
            let brute: Vec<_> = entries
                .iter()
                .filter_map(|(id, bounds)| contains(*bounds, point).then_some(*id))
                .collect();
            assert_eq!(indexed, brute);
        }
    }

    #[test]
    fn rebuild_is_atomic_and_clear_empties_all_storage() {
        let mut index = UniformGridIndex::default();
        let entries = [(id(1), Rect::from_min_max(Vec2::ZERO, Vec2::new(10.0, 10.0)))];
        index.rebuild(&entries).unwrap();
        let before = index.clone();
        let invalid = [(
            id(2),
            Rect::from_min_max(Vec2::new(2.0, 0.0), Vec2::new(1.0, 1.0)),
        )];
        assert!(index.rebuild(&invalid).is_err());
        assert_eq!(index, before);
        index.clear();
        assert_eq!(index.entry_count(), 0);
    }

    #[test]
    fn non_finite_bounds_and_excessive_queries_are_typed_errors() {
        let mut index = UniformGridIndex::default();
        assert!(matches!(
            index.insert(
                id(1),
                Rect::from_min_max(Vec2::ZERO, Vec2::new(f64::INFINITY, 1.0))
            ),
            Err(SpatialError::InvalidBounds(_))
        ));
        assert_eq!(
            index.query_rect(Rect::from_min_max(
                Vec2::new(-1.0e9, -1.0e9),
                Vec2::new(1.0e9, 1.0e9)
            )),
            Err(SpatialError::QueryTooLarge)
        );
    }
}

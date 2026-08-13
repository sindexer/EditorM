use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use visual_authoring_core_math::Rect;
use visual_authoring_document::NodeId;

#[derive(Clone, Copy, Debug)]
struct BoundKey {
    value: f64,
    node: NodeId,
}

impl PartialEq for BoundKey {
    fn eq(&self, other: &Self) -> bool {
        self.value.total_cmp(&other.value) == Ordering::Equal && self.node == other.node
    }
}

impl Eq for BoundKey {}

impl PartialOrd for BoundKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BoundKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value
            .total_cmp(&other.value)
            .then_with(|| self.node.cmp(&other.node))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BoundsAggregate {
    contributions: BTreeMap<NodeId, Rect>,
    min_x: BTreeSet<BoundKey>,
    min_y: BTreeSet<BoundKey>,
    max_x: BTreeSet<BoundKey>,
    max_y: BTreeSet<BoundKey>,
}

impl BoundsAggregate {
    pub(crate) fn set(&mut self, node: NodeId, bounds: Option<Rect>) -> bool {
        if self.contributions.get(&node).copied() == bounds {
            return false;
        }
        if let Some(previous) = self.contributions.remove(&node) {
            self.remove_keys(node, previous);
        }
        if let Some(bounds) = bounds {
            self.insert_keys(node, bounds);
            self.contributions.insert(node, bounds);
        }
        true
    }

    pub(crate) fn remove(&mut self, node: NodeId) -> bool {
        self.set(node, None)
    }

    pub(crate) fn bounds(&self) -> Option<Rect> {
        let min_x = self.min_x.first()?.value;
        let min_y = self.min_y.first()?.value;
        let max_x = self.max_x.last()?.value;
        let max_y = self.max_y.last()?.value;
        Some(Rect::from_min_max(
            visual_authoring_core_math::Vec2::new(min_x, min_y),
            visual_authoring_core_math::Vec2::new(max_x, max_y),
        ))
    }

    fn insert_keys(&mut self, node: NodeId, bounds: Rect) {
        self.min_x.insert(BoundKey {
            value: bounds.min.x,
            node,
        });
        self.min_y.insert(BoundKey {
            value: bounds.min.y,
            node,
        });
        self.max_x.insert(BoundKey {
            value: bounds.max.x,
            node,
        });
        self.max_y.insert(BoundKey {
            value: bounds.max.y,
            node,
        });
    }

    fn remove_keys(&mut self, node: NodeId, bounds: Rect) {
        self.min_x.remove(&BoundKey {
            value: bounds.min.x,
            node,
        });
        self.min_y.remove(&BoundKey {
            value: bounds.min.y,
            node,
        });
        self.max_x.remove(&BoundKey {
            value: bounds.max.x,
            node,
        });
        self.max_y.remove(&BoundKey {
            value: bounds.max.y,
            node,
        });
    }
}

pub(crate) fn union(left: Option<Rect>, right: Option<Rect>) -> Option<Rect> {
    match (left, right) {
        (Some(left), Some(right)) => Some(Rect::from_min_max(
            visual_authoring_core_math::Vec2::new(
                left.min.x.min(right.min.x),
                left.min.y.min(right.min.y),
            ),
            visual_authoring_core_math::Vec2::new(
                left.max.x.max(right.max.x),
                left.max.y.max(right.max.y),
            ),
        )),
        (Some(bounds), None) | (None, Some(bounds)) => Some(bounds),
        (None, None) => None,
    }
}

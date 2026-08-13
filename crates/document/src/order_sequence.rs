use std::collections::HashMap;
use std::fmt;
use std::mem::size_of;
use std::ops::{AddAssign, Index};

use crate::NodeId;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SequenceWork {
    pub entries_examined: u64,
    pub entries_copied: u64,
    pub entries_moved_or_shifted: u64,
    pub rank_order_comparisons: u64,
    pub sequence_nodes_allocated: u64,
    pub allocated_bytes: u64,
    pub tree_rebalances: u64,
    pub maximum_sequence_depth: u64,
    pub fallback_or_rebuild_count: u64,
    pub full_sequence_scans: u64,
    pub full_sequence_copies: u64,
    pub dense_index_rewrites: u64,
}

impl AddAssign for SequenceWork {
    fn add_assign(&mut self, rhs: Self) {
        self.entries_examined += rhs.entries_examined;
        self.entries_copied += rhs.entries_copied;
        self.entries_moved_or_shifted += rhs.entries_moved_or_shifted;
        self.rank_order_comparisons += rhs.rank_order_comparisons;
        self.sequence_nodes_allocated += rhs.sequence_nodes_allocated;
        self.allocated_bytes += rhs.allocated_bytes;
        self.tree_rebalances += rhs.tree_rebalances;
        self.maximum_sequence_depth = self.maximum_sequence_depth.max(rhs.maximum_sequence_depth);
        self.fallback_or_rebuild_count += rhs.fallback_or_rebuild_count;
        self.full_sequence_scans += rhs.full_sequence_scans;
        self.full_sequence_copies += rhs.full_sequence_copies;
        self.dense_index_rewrites += rhs.dense_index_rewrites;
    }
}

#[derive(Clone)]
struct SequenceNode {
    value: NodeId,
    left: Option<usize>,
    right: Option<usize>,
    parent: Option<usize>,
    previous: Option<usize>,
    next: Option<usize>,
    size: usize,
    height: usize,
}

/// Arena-backed implicit order-statistic AVL sequence for stable node IDs.
///
/// Rank lookup, rank-by-ID, insertion, and removal are worst-case O(log N). Tree shape is
/// independent of external IDs. Iteration is reserved for explicit snapshot, persistence, and
/// full-build boundaries. Construction is always tracked so structural callers cannot omit work.
#[derive(Clone, Default)]
pub struct OrderSequence {
    root: Option<usize>,
    nodes: Vec<Option<SequenceNode>>,
    free: Vec<usize>,
    by_id: HashMap<NodeId, usize>,
}

impl OrderSequence {
    #[must_use]
    pub fn from_ids_tracked(
        ids: impl IntoIterator<Item = NodeId>,
        work: &mut SequenceWork,
    ) -> Self {
        let values = ids.into_iter().collect::<Vec<_>>();
        work.entries_examined += values.len() as u64;
        work.entries_copied += values.len() as u64;
        let mut sequence = Self {
            root: None,
            nodes: Vec::with_capacity(values.len()),
            free: Vec::new(),
            by_id: HashMap::with_capacity(values.len()),
        };
        for value in values.iter().copied() {
            assert!(
                !sequence.by_id.contains_key(&value),
                "duplicate sequence ID"
            );
            let index = sequence.nodes.len();
            sequence.nodes.push(Some(SequenceNode::new(value)));
            sequence.by_id.insert(value, index);
        }
        for index in 0..values.len() {
            let previous = index.checked_sub(1);
            let next = (index + 1 < values.len()).then_some(index + 1);
            let node = sequence.node_mut(index);
            node.previous = previous;
            node.next = next;
        }
        work.sequence_nodes_allocated += values.len() as u64;
        work.allocated_bytes += (values.len() * size_of::<SequenceNode>()) as u64;
        work.entries_moved_or_shifted += values.len() as u64;
        sequence.root = sequence.build_balanced(0, values.len(), None);
        sequence.record_depth(work);
        sequence
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.node_size(self.root)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    #[must_use]
    pub fn contains(&self, id: &NodeId) -> bool {
        self.by_id.contains_key(id)
    }

    #[must_use]
    pub fn get(&self, rank: usize) -> Option<&NodeId> {
        let mut work = SequenceWork::default();
        self.get_tracked(rank, &mut work)
    }

    #[must_use]
    pub fn get_tracked(&self, rank: usize, work: &mut SequenceWork) -> Option<&NodeId> {
        let mut current = self.root?;
        let mut remaining = rank;
        loop {
            work.entries_examined += 1;
            work.rank_order_comparisons += 1;
            let node = self.node(current);
            let left_size = self.node_size(node.left);
            if remaining < left_size {
                current = node.left?;
            } else if remaining == left_size {
                return Some(&node.value);
            } else {
                remaining -= left_size + 1;
                current = node.right?;
            }
        }
    }

    #[must_use]
    pub fn rank_of(&self, id: NodeId) -> Option<usize> {
        let mut work = SequenceWork::default();
        self.rank_of_tracked(id, &mut work)
    }

    #[must_use]
    pub fn rank_of_tracked(&self, id: NodeId, work: &mut SequenceWork) -> Option<usize> {
        let mut current = *self.by_id.get(&id)?;
        let mut rank = self.node_size(self.node(current).left);
        work.entries_examined += 1;
        while let Some(parent) = self.node(current).parent {
            work.entries_examined += 1;
            work.rank_order_comparisons += 1;
            if self.node(parent).right == Some(current) {
                rank += self.node_size(self.node(parent).left) + 1;
            }
            current = parent;
        }
        Some(rank)
    }

    /// Returns rank and immediate neighbors from the same validated node lookup.
    #[must_use]
    pub fn placement_of_tracked(
        &self,
        id: NodeId,
        work: &mut SequenceWork,
    ) -> Option<(usize, Option<NodeId>, Option<NodeId>)> {
        let mut current = *self.by_id.get(&id)?;
        let node = self.node(current);
        let previous = node.previous.map(|index| self.node(index).value);
        let next = node.next.map(|index| self.node(index).value);
        let mut rank = self.node_size(node.left);
        work.entries_examined += 1;
        while let Some(parent) = self.node(current).parent {
            work.entries_examined += 1;
            work.rank_order_comparisons += 1;
            if self.node(parent).right == Some(current) {
                rank += self.node_size(self.node(parent).left) + 1;
            }
            current = parent;
        }
        Some((rank, previous, next))
    }

    /// Returns the immediate in-order neighbors of a validated ID without recomputing its rank.
    #[must_use]
    pub fn neighbors_of_tracked(
        &self,
        id: NodeId,
        work: &mut SequenceWork,
    ) -> Option<(Option<NodeId>, Option<NodeId>)> {
        let index = *self.by_id.get(&id)?;
        work.entries_examined += 1;
        let node = self.node(index);
        Some((
            node.previous.map(|previous| self.node(previous).value),
            node.next.map(|next| self.node(next).value),
        ))
    }

    pub fn push(&mut self, id: NodeId) -> SequenceWork {
        self.insert(self.len(), id)
    }

    pub fn insert(&mut self, rank: usize, id: NodeId) -> SequenceWork {
        let mut work = SequenceWork::default();
        self.insert_tracked(rank, id, &mut work);
        work
    }

    pub fn insert_tracked(&mut self, rank: usize, id: NodeId, work: &mut SequenceWork) {
        assert!(rank <= self.len(), "sequence insertion rank out of bounds");
        assert!(!self.by_id.contains_key(&id), "duplicate sequence ID");
        let allocated_new = self.free.is_empty();
        let index = if let Some(index) = self.free.pop() {
            self.nodes[index] = Some(SequenceNode::new(id));
            index
        } else {
            self.nodes.push(Some(SequenceNode::new(id)));
            self.nodes.len() - 1
        };
        self.by_id.insert(id, index);
        if allocated_new {
            work.sequence_nodes_allocated += 1;
            work.allocated_bytes += size_of::<SequenceNode>() as u64;
        }
        work.entries_moved_or_shifted += 1;

        let Some(mut current) = self.root else {
            self.root = Some(index);
            self.record_depth(work);
            return;
        };
        let mut remaining = rank;
        let mut previous = None;
        let mut next = None;
        loop {
            work.entries_examined += 1;
            work.rank_order_comparisons += 1;
            let left_size = self.node_size(self.node(current).left);
            if remaining <= left_size {
                next = Some(current);
                if let Some(left) = self.node(current).left {
                    current = left;
                } else {
                    self.node_mut(current).left = Some(index);
                    self.node_mut(index).parent = Some(current);
                    self.rebalance_from(Some(current), work);
                    break;
                }
            } else {
                remaining -= left_size + 1;
                previous = Some(current);
                if let Some(right) = self.node(current).right {
                    current = right;
                } else {
                    self.node_mut(current).right = Some(index);
                    self.node_mut(index).parent = Some(current);
                    self.rebalance_from(Some(current), work);
                    break;
                }
            }
        }
        {
            let inserted = self.node_mut(index);
            inserted.previous = previous;
            inserted.next = next;
        }
        if let Some(previous) = previous {
            self.node_mut(previous).next = Some(index);
            work.entries_moved_or_shifted += 1;
        }
        if let Some(next) = next {
            self.node_mut(next).previous = Some(index);
            work.entries_moved_or_shifted += 1;
        }
        self.record_depth(work);
    }

    pub fn remove_id(&mut self, id: NodeId) -> Option<(usize, SequenceWork)> {
        let mut work = SequenceWork::default();
        let rank = self.rank_of_tracked(id, &mut work)?;
        let index = *self.by_id.get(&id)?;
        self.remove_index(index, id, false, &mut work);
        Some((rank, work))
    }

    /// Removes a validated ID without repeating a rank lookup already performed by the caller.
    pub fn remove_known_tracked(&mut self, id: NodeId, work: &mut SequenceWork) -> bool {
        let Some(index) = self.by_id.get(&id).copied() else {
            return false;
        };
        self.remove_index(index, id, false, work);
        true
    }

    /// Removes a validated singleton-run ID, preferring its predecessor on an equal-height tie.
    pub fn remove_known_tracked_prefer_predecessor(
        &mut self,
        id: NodeId,
        work: &mut SequenceWork,
    ) -> bool {
        let Some(index) = self.by_id.get(&id).copied() else {
            return false;
        };
        self.remove_index(index, id, true, work);
        true
    }

    pub fn remove_at(&mut self, rank: usize) -> Option<(NodeId, SequenceWork)> {
        let mut work = SequenceWork::default();
        let id = self.remove_at_tracked(rank, &mut work)?;
        Some((id, work))
    }

    pub fn remove_at_tracked(&mut self, rank: usize, work: &mut SequenceWork) -> Option<NodeId> {
        let id = *self.get_tracked(rank, work)?;
        let index = *self.by_id.get(&id)?;
        self.remove_index(index, id, false, work);
        Some(id)
    }

    #[must_use]
    pub fn iter(&self) -> OrderSequenceIter<'_> {
        OrderSequenceIter::new(self)
    }

    #[must_use]
    pub fn to_vec(&self) -> Vec<NodeId> {
        self.iter().copied().collect()
    }

    #[must_use]
    pub fn maximum_depth(&self) -> usize {
        self.node_height(self.root)
    }

    #[must_use]
    pub fn logarithmic_depth_bound(len: usize) -> usize {
        if len == 0 {
            0
        } else {
            2 * (usize::BITS - len.leading_zeros()) as usize + 1
        }
    }

    fn build_balanced(&mut self, start: usize, end: usize, parent: Option<usize>) -> Option<usize> {
        if start >= end {
            return None;
        }
        let middle = start + (end - start) / 2;
        let left = self.build_balanced(start, middle, Some(middle));
        let right = self.build_balanced(middle + 1, end, Some(middle));
        {
            let node = self.node_mut(middle);
            node.parent = parent;
            node.left = left;
            node.right = right;
        }
        self.update(middle);
        Some(middle)
    }

    fn remove_index(
        &mut self,
        index: usize,
        id: NodeId,
        prefer_predecessor: bool,
        work: &mut SequenceWork,
    ) {
        let mut removal = index;
        if let (Some(left), Some(right)) = (self.node(index).left, self.node(index).right) {
            let left_height = self.node_height(Some(left));
            let right_height = self.node_height(Some(right));
            let use_predecessor =
                left_height > right_height || (prefer_predecessor && left_height == right_height);
            let mut replacement = if use_predecessor { left } else { right };
            if use_predecessor {
                while let Some(next) = self.node(replacement).right {
                    work.entries_examined += 1;
                    work.rank_order_comparisons += 1;
                    replacement = next;
                }
            } else {
                while let Some(next) = self.node(replacement).left {
                    work.entries_examined += 1;
                    work.rank_order_comparisons += 1;
                    replacement = next;
                }
            }
            let replacement_id = self.node(replacement).value;
            self.node_mut(index).value = replacement_id;
            self.node_mut(replacement).value = id;
            self.by_id.insert(replacement_id, index);
            self.by_id.insert(id, replacement);
            work.entries_moved_or_shifted += 2;
            removal = replacement;
        }

        let previous = self.node(removal).previous;
        let next = self.node(removal).next;
        if let Some(previous) = previous {
            self.node_mut(previous).next = next;
            work.entries_moved_or_shifted += 1;
        }
        if let Some(next) = next {
            self.node_mut(next).previous = previous;
            work.entries_moved_or_shifted += 1;
        }

        let parent = self.node(removal).parent;
        let child = self.node(removal).left.or(self.node(removal).right);
        self.replace_child(parent, Some(removal), child);
        self.set_parent(child, parent);
        self.by_id.remove(&id);
        self.nodes[removal] = None;
        self.free.push(removal);
        work.entries_moved_or_shifted += 1;
        self.rebalance_from(parent.or(child), work);
        self.record_depth(work);
    }

    fn rebalance_from(&mut self, mut current: Option<usize>, work: &mut SequenceWork) {
        while let Some(index) = current {
            work.entries_examined += 1;
            self.update(index);
            let balance = self.balance_factor(index);
            let subtree_root = if balance > 1 {
                let left = self.node(index).left.expect("left-heavy node");
                if self.balance_factor(left) < 0 {
                    self.rotate_left(left, work);
                }
                self.rotate_right(index, work)
            } else if balance < -1 {
                let right = self.node(index).right.expect("right-heavy node");
                if self.balance_factor(right) > 0 {
                    self.rotate_right(right, work);
                }
                self.rotate_left(index, work)
            } else {
                index
            };
            current = self.node(subtree_root).parent;
        }
    }

    fn rotate_left(&mut self, pivot: usize, work: &mut SequenceWork) -> usize {
        let promoted = self
            .node(pivot)
            .right
            .expect("left rotation requires right child");
        let parent = self.node(pivot).parent;
        let middle = self.node(promoted).left;
        self.replace_child(parent, Some(pivot), Some(promoted));
        self.node_mut(promoted).parent = parent;
        self.node_mut(promoted).left = Some(pivot);
        self.node_mut(pivot).parent = Some(promoted);
        self.node_mut(pivot).right = middle;
        self.set_parent(middle, Some(pivot));
        self.update(pivot);
        self.update(promoted);
        work.tree_rebalances += 1;
        work.entries_moved_or_shifted += 3;
        promoted
    }

    fn rotate_right(&mut self, pivot: usize, work: &mut SequenceWork) -> usize {
        let promoted = self
            .node(pivot)
            .left
            .expect("right rotation requires left child");
        let parent = self.node(pivot).parent;
        let middle = self.node(promoted).right;
        self.replace_child(parent, Some(pivot), Some(promoted));
        self.node_mut(promoted).parent = parent;
        self.node_mut(promoted).right = Some(pivot);
        self.node_mut(pivot).parent = Some(promoted);
        self.node_mut(pivot).left = middle;
        self.set_parent(middle, Some(pivot));
        self.update(pivot);
        self.update(promoted);
        work.tree_rebalances += 1;
        work.entries_moved_or_shifted += 3;
        promoted
    }

    fn replace_child(
        &mut self,
        parent: Option<usize>,
        old_child: Option<usize>,
        new_child: Option<usize>,
    ) {
        if let Some(parent) = parent {
            if self.node(parent).left == old_child {
                self.node_mut(parent).left = new_child;
            } else {
                debug_assert_eq!(self.node(parent).right, old_child);
                self.node_mut(parent).right = new_child;
            }
        } else {
            debug_assert_eq!(self.root, old_child);
            self.root = new_child;
        }
    }

    fn balance_factor(&self, index: usize) -> isize {
        self.node_height(self.node(index).left) as isize
            - self.node_height(self.node(index).right) as isize
    }

    fn node(&self, index: usize) -> &SequenceNode {
        self.nodes[index].as_ref().expect("live sequence node")
    }

    fn node_mut(&mut self, index: usize) -> &mut SequenceNode {
        self.nodes[index].as_mut().expect("live sequence node")
    }

    fn node_size(&self, index: Option<usize>) -> usize {
        index.map_or(0, |index| self.node(index).size)
    }

    fn node_height(&self, index: Option<usize>) -> usize {
        index.map_or(0, |index| self.node(index).height)
    }

    fn update(&mut self, index: usize) {
        let (left, right) = {
            let node = self.node(index);
            (node.left, node.right)
        };
        let size = 1 + self.node_size(left) + self.node_size(right);
        let height = 1 + self.node_height(left).max(self.node_height(right));
        let node = self.node_mut(index);
        node.size = size;
        node.height = height;
    }

    fn set_parent(&mut self, child: Option<usize>, parent: Option<usize>) {
        if let Some(child) = child {
            self.node_mut(child).parent = parent;
        }
    }

    fn record_depth(&self, work: &mut SequenceWork) {
        work.maximum_sequence_depth = work.maximum_sequence_depth.max(self.maximum_depth() as u64);
    }
}

impl SequenceNode {
    fn new(value: NodeId) -> Self {
        Self {
            value,
            left: None,
            right: None,
            parent: None,
            previous: None,
            next: None,
            size: 1,
            height: 1,
        }
    }
}

pub struct OrderSequenceIter<'a> {
    sequence: &'a OrderSequence,
    front_stack: Vec<usize>,
    front_current: Option<usize>,
    back_stack: Vec<usize>,
    back_current: Option<usize>,
    remaining: usize,
}

impl<'a> OrderSequenceIter<'a> {
    fn new(sequence: &'a OrderSequence) -> Self {
        Self {
            sequence,
            front_stack: Vec::new(),
            front_current: sequence.root,
            back_stack: Vec::new(),
            back_current: sequence.root,
            remaining: sequence.len(),
        }
    }
}

impl<'a> Iterator for OrderSequenceIter<'a> {
    type Item = &'a NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        while let Some(current) = self.front_current {
            self.front_stack.push(current);
            self.front_current = self.sequence.node(current).left;
        }
        let index = self.front_stack.pop()?;
        let node = self.sequence.node(index);
        self.front_current = node.right;
        self.remaining -= 1;
        Some(&node.value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl DoubleEndedIterator for OrderSequenceIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        while let Some(current) = self.back_current {
            self.back_stack.push(current);
            self.back_current = self.sequence.node(current).right;
        }
        let index = self.back_stack.pop()?;
        let node = self.sequence.node(index);
        self.back_current = node.left;
        self.remaining -= 1;
        Some(&node.value)
    }
}

impl ExactSizeIterator for OrderSequenceIter<'_> {}

impl<'a> IntoIterator for &'a OrderSequence {
    type Item = &'a NodeId;
    type IntoIter = OrderSequenceIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl Index<usize> for OrderSequence {
    type Output = NodeId;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("sequence index out of bounds")
    }
}

impl PartialEq for OrderSequence {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl Eq for OrderSequence {}

impl PartialEq<Vec<NodeId>> for &OrderSequence {
    fn eq(&self, other: &Vec<NodeId>) -> bool {
        self.iter().copied().eq(other.iter().copied())
    }
}

impl PartialEq<Vec<NodeId>> for OrderSequence {
    fn eq(&self, other: &Vec<NodeId>) -> bool {
        self.iter().copied().eq(other.iter().copied())
    }
}

impl<const N: usize> PartialEq<[NodeId; N]> for OrderSequence {
    fn eq(&self, other: &[NodeId; N]) -> bool {
        self.iter().copied().eq(other.iter().copied())
    }
}

impl fmt::Debug for OrderSequence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_list().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn id(value: u128) -> NodeId {
        NodeId::from_uuid(Uuid::from_u128(value))
    }

    #[test]
    fn rank_insert_remove_match_dense_oracle_without_dense_rewrites() {
        let mut build = SequenceWork::default();
        let mut sequence = OrderSequence::from_ids_tracked((1..=1_000).map(id), &mut build);
        assert_eq!(build.sequence_nodes_allocated, 1_000);
        for rank in [0, 500, 999] {
            assert_eq!(sequence.rank_of(id((rank + 1) as u128)), Some(rank));
            assert_eq!(sequence.get(rank), Some(&id((rank + 1) as u128)));
        }
        let inserted = id(10_000);
        let insert_work = sequence.insert(500, inserted);
        assert_eq!(sequence.rank_of(inserted), Some(500));
        assert_eq!(insert_work.full_sequence_scans, 0);
        assert_eq!(insert_work.full_sequence_copies, 0);
        assert_eq!(insert_work.dense_index_rewrites, 0);
        let (removed_rank, remove_work) = sequence.remove_id(inserted).expect("inserted ID");
        assert_eq!(removed_rank, 500);
        assert_eq!(remove_work.full_sequence_scans, 0);
        assert_eq!(remove_work.full_sequence_copies, 0);
        assert_eq!(remove_work.dense_index_rewrites, 0);
        assert_eq!(sequence.to_vec(), (1..=1_000).map(id).collect::<Vec<_>>());
        assert!(sequence.maximum_depth() <= OrderSequence::logarithmic_depth_bound(sequence.len()));
    }

    #[test]
    fn monotonic_reverse_and_edge_insertions_remain_logarithmic() {
        for values in [
            (1..=10_000).collect::<Vec<_>>(),
            (1..=10_000).rev().collect::<Vec<_>>(),
        ] {
            let mut work = SequenceWork::default();
            let mut sequence = OrderSequence::from_ids_tracked(
                values.into_iter().map(|value| id(value as u128)),
                &mut work,
            );
            for offset in 0..1_000 {
                let value = id(100_000 + offset);
                let rank = match offset % 3 {
                    0 => 0,
                    1 => sequence.len() / 2,
                    _ => sequence.len(),
                };
                sequence.insert_tracked(rank, value, &mut work);
            }
            assert!(
                sequence.maximum_depth() <= OrderSequence::logarithmic_depth_bound(sequence.len())
            );
            assert!(
                work.maximum_sequence_depth as usize
                    <= OrderSequence::logarithmic_depth_bound(sequence.len())
            );
            assert!(work.tree_rebalances > 0);
        }
    }
}

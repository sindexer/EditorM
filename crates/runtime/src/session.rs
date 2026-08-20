use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;
use visual_authoring_document::{
    Document, DocumentChange, DocumentChangeSet, NodeId, NodeKind, Selection,
};

use crate::Camera;

#[derive(Clone, Debug, Error, PartialEq)]
pub enum SlideSessionError {
    #[error("document has no root-level Frame Slide")]
    NoSlides,
    #[error("node {0} does not exist")]
    NodeNotFound(NodeId),
    #[error("node {0} is not a root-level Frame Slide")]
    NotSlide(NodeId),
    #[error("node {node} is outside active Slide {active_slide}")]
    NodeOutsideActiveSlide { node: NodeId, active_slide: NodeId },
    #[error("Slide name must contain a non-whitespace character")]
    EmptySlideName,
    #[error("Slide index {index} is outside 0..={maximum}")]
    InvalidSlideIndex { index: usize, maximum: usize },
    #[error("the last remaining Slide cannot be deleted")]
    LastSlideDeletion,
}

/// Worker-owned interaction state. Nothing in this type is serialized with Document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditorSession {
    slide_ids: Vec<NodeId>,
    preserved_root_items: Vec<NodeId>,
    active_slide_id: Option<NodeId>,
    selection_by_slide: BTreeMap<NodeId, Selection>,
    camera_by_slide: BTreeMap<NodeId, Camera>,
    visited_slides: BTreeSet<NodeId>,
    thumbnail_revision_by_slide: BTreeMap<NodeId, u64>,
}

impl EditorSession {
    #[must_use]
    pub fn new(document: &Document) -> Self {
        let (slide_ids, preserved_root_items) = Self::classify_root_items(document);
        Self {
            active_slide_id: slide_ids.first().copied(),
            slide_ids,
            preserved_root_items,
            ..Self::default()
        }
    }

    #[must_use]
    pub const fn active_slide_id(&self) -> Option<NodeId> {
        self.active_slide_id
    }

    #[must_use]
    pub fn thumbnail_revision(&self, slide: NodeId) -> u64 {
        self.thumbnail_revision_by_slide
            .get(&slide)
            .copied()
            .unwrap_or(0)
    }

    #[must_use]
    pub fn slide_ids(&self) -> &[NodeId] {
        &self.slide_ids
    }

    #[must_use]
    pub fn preserved_root_items(&self) -> &[NodeId] {
        &self.preserved_root_items
    }

    fn classify_root_items(document: &Document) -> (Vec<NodeId>, Vec<NodeId>) {
        let Some(root) = document.node(document.root_id()) else {
            return (Vec::new(), Vec::new());
        };
        let mut slides = Vec::new();
        let mut preserved = Vec::new();
        for id in root.children().iter().copied() {
            match document.node(id).map(|node| node.kind()) {
                Some(NodeKind::Frame) => slides.push(id),
                Some(_) => preserved.push(id),
                None => {}
            }
        }
        if slides.is_empty() {
            preserved.clear();
        }
        (slides, preserved)
    }

    pub fn validate_slide(document: &Document, id: NodeId) -> Result<(), SlideSessionError> {
        let node = document
            .node(id)
            .ok_or(SlideSessionError::NodeNotFound(id))?;
        if node.kind() != NodeKind::Frame || node.parent() != Some(document.root_id()) {
            return Err(SlideSessionError::NotSlide(id));
        }
        Ok(())
    }

    #[must_use]
    pub fn node_belongs_to_slide(document: &Document, node: NodeId, slide: NodeId) -> bool {
        let mut current = Some(node);
        let mut visited = BTreeSet::new();
        while let Some(id) = current {
            if id == slide {
                return true;
            }
            if !visited.insert(id) || id == document.root_id() {
                return false;
            }
            current = document.node(id).and_then(|entry| entry.parent());
        }
        false
    }

    pub(crate) fn record_changes(&mut self, document: &Document, change_set: &DocumentChangeSet) {
        let mut dirty = BTreeSet::new();
        for change in change_set.changes() {
            match change {
                DocumentChange::NodesInserted {
                    root, placement, ..
                } => {
                    if let Some(slide) = Self::containing_slide(document, *root).or_else(|| {
                        placement.and_then(|entry| Self::containing_slide(document, entry.parent))
                    }) {
                        dirty.insert(slide);
                    }
                }
                DocumentChange::NodesRemoved { placement, .. } => {
                    if let Some(slide) =
                        placement.and_then(|entry| Self::containing_slide(document, entry.parent))
                    {
                        dirty.insert(slide);
                    }
                }
                DocumentChange::PlacementChanged {
                    root,
                    before,
                    after,
                    ..
                } => {
                    if let Some(slide) = Self::containing_slide(document, *root) {
                        dirty.insert(slide);
                    }
                    for parent in [before, after]
                        .into_iter()
                        .flatten()
                        .map(|entry| entry.parent)
                    {
                        if let Some(slide) = Self::containing_slide(document, parent) {
                            dirty.insert(slide);
                        }
                    }
                }
                DocumentChange::StructuralGroupChanged { parent, .. } => {
                    if let Some(slide) = Self::containing_slide(document, *parent) {
                        dirty.insert(slide);
                    }
                }
                DocumentChange::LocalTransformChanged { node }
                | DocumentChange::GeometryChanged { node }
                | DocumentChange::VisibilityChanged { node }
                | DocumentChange::AppearanceChanged { node, .. }
                | DocumentChange::PersistentPropertyChanged { node, .. } => {
                    if let Some(slide) = Self::containing_slide(document, *node) {
                        dirty.insert(slide);
                    }
                }
                DocumentChange::FullDocumentReset => {
                    dirty.extend(Self::classify_root_items(document).0)
                }
            }
        }
        if self.root_classification_changed(document, change_set) {
            (self.slide_ids, self.preserved_root_items) = Self::classify_root_items(document);
            self.retain_valid_slides();
        }
        let revision = change_set.revision().after;
        for slide in dirty {
            self.thumbnail_revision_by_slide.insert(slide, revision);
        }
    }

    fn root_classification_changed(
        &self,
        document: &Document,
        change_set: &DocumentChangeSet,
    ) -> bool {
        let root = document.root_id();
        change_set.changes().iter().any(|change| match change {
            DocumentChange::FullDocumentReset => true,
            DocumentChange::NodesInserted {
                root: changed,
                placement,
                ..
            }
            | DocumentChange::NodesRemoved {
                root: changed,
                placement,
                ..
            } => {
                placement.is_some_and(|entry| entry.parent == root)
                    && (!self.slide_ids.is_empty()
                        || self.slide_ids.contains(changed)
                        || document
                            .node(*changed)
                            .is_some_and(|node| node.kind() == NodeKind::Frame))
            }
            DocumentChange::PlacementChanged {
                root: changed,
                before,
                after,
                ..
            } => {
                [before, after]
                    .into_iter()
                    .flatten()
                    .any(|entry| entry.parent == root)
                    && (!self.slide_ids.is_empty()
                        || self.slide_ids.contains(changed)
                        || document
                            .node(*changed)
                            .is_some_and(|node| node.kind() == NodeKind::Frame))
            }
            DocumentChange::StructuralGroupChanged { parent, .. } => {
                *parent == root && !self.slide_ids.is_empty()
            }
            DocumentChange::LocalTransformChanged { .. }
            | DocumentChange::GeometryChanged { .. }
            | DocumentChange::VisibilityChanged { .. }
            | DocumentChange::AppearanceChanged { .. }
            | DocumentChange::PersistentPropertyChanged { .. } => false,
        })
    }

    fn containing_slide(document: &Document, node: NodeId) -> Option<NodeId> {
        let mut current = node;
        let mut visited = BTreeSet::new();
        while visited.insert(current) {
            let entry = document.node(current)?;
            if entry.parent() == Some(document.root_id()) {
                return (entry.kind() == NodeKind::Frame).then_some(current);
            }
            current = entry.parent()?;
        }
        None
    }

    pub(crate) fn remember(&mut self, slide: NodeId, selection: Selection, camera: Camera) {
        self.selection_by_slide.insert(slide, selection);
        self.camera_by_slide.insert(slide, camera);
        self.visited_slides.insert(slide);
    }

    pub(crate) fn set_active(&mut self, slide: Option<NodeId>) {
        self.active_slide_id = slide;
        if let Some(slide) = slide {
            self.visited_slides.insert(slide);
        }
    }

    pub(crate) fn stored_selection(&self, slide: NodeId) -> Option<&Selection> {
        self.selection_by_slide.get(&slide)
    }

    pub(crate) fn stored_camera(&self, slide: NodeId) -> Option<&Camera> {
        self.camera_by_slide.get(&slide)
    }

    pub(crate) fn retain_valid_slides(&mut self) {
        let valid = self.slide_ids.iter().copied().collect::<BTreeSet<_>>();
        self.selection_by_slide.retain(|id, _| valid.contains(id));
        self.camera_by_slide.retain(|id, _| valid.contains(id));
        self.visited_slides.retain(|id| valid.contains(id));
        self.thumbnail_revision_by_slide
            .retain(|id, _| valid.contains(id));
        if self.active_slide_id.is_some_and(|id| !valid.contains(&id)) {
            self.active_slide_id = None;
        }
    }
}

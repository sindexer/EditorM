use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;
use visual_authoring_document::{Document, NodeId, NodeKind, Selection};

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
    active_slide_id: Option<NodeId>,
    selection_by_slide: BTreeMap<NodeId, Selection>,
    camera_by_slide: BTreeMap<NodeId, Camera>,
    visited_slides: BTreeSet<NodeId>,
}

impl EditorSession {
    #[must_use]
    pub fn new(document: &Document) -> Self {
        Self {
            active_slide_id: Self::slide_ids(document).first().copied(),
            ..Self::default()
        }
    }

    #[must_use]
    pub const fn active_slide_id(&self) -> Option<NodeId> {
        self.active_slide_id
    }

    #[must_use]
    pub fn slide_ids(document: &Document) -> Vec<NodeId> {
        let Some(root) = document.node(document.root_id()) else {
            return Vec::new();
        };
        root.children()
            .iter()
            .copied()
            .filter(|id| {
                document
                    .node(*id)
                    .is_some_and(|node| node.kind() == NodeKind::Frame)
            })
            .collect()
    }

    #[must_use]
    pub fn preserved_root_items(document: &Document) -> Vec<NodeId> {
        let Some(root) = document.node(document.root_id()) else {
            return Vec::new();
        };
        root.children()
            .iter()
            .copied()
            .filter(|id| {
                document
                    .node(*id)
                    .is_some_and(|node| node.kind() != NodeKind::Frame)
            })
            .collect()
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

    pub(crate) fn retain_valid_slides(&mut self, document: &Document) {
        let valid = Self::slide_ids(document)
            .into_iter()
            .collect::<BTreeSet<_>>();
        self.selection_by_slide.retain(|id, _| valid.contains(id));
        self.camera_by_slide.retain(|id, _| valid.contains(id));
        self.visited_slides.retain(|id| valid.contains(id));
        if self.active_slide_id.is_some_and(|id| !valid.contains(&id)) {
            self.active_slide_id = None;
        }
    }
}

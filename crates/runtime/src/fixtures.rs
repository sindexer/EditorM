//! Deterministic structural proof fixtures. Construction uses the validated load boundary,
//! not the normal incremental editing path.

use uuid::Uuid;
use visual_authoring_core_math::{Affine2, Vec2};
use visual_authoring_document::{
    Document, DocumentError, DocumentSnapshot, NodeId, NodeSnapshot, NodeSpec,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureKind {
    Preview,
    BenchA,
    BenchB,
    BenchC,
    BenchD,
}

impl FixtureKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Preview => "PREVIEW",
            Self::BenchA => "BENCH-A",
            Self::BenchB => "BENCH-B",
            Self::BenchC => "BENCH-C",
            Self::BenchD => "BENCH-D",
        }
    }
}

#[must_use]
pub fn fixture_node_id(index: u128) -> NodeId {
    NodeId::from_uuid(Uuid::from_u128(index + 1))
}

pub fn build_fixture(kind: FixtureKind) -> Result<Document, DocumentError> {
    match kind {
        FixtureKind::Preview => preview_shapes(),
        FixtureKind::BenchA => flat_rectangles(1_000, 40.0),
        FixtureKind::BenchB => flat_rectangles(10_000, 40.0),
        FixtureKind::BenchC => flat_rectangles(100_000, 512.0),
        FixtureKind::BenchD => deep_hierarchy(10_000),
    }
}

fn preview_shapes() -> Result<Document, DocumentError> {
    let root_id = fixture_node_id(0);
    let rectangle_id = fixture_node_id(1);
    let ellipse_id = fixture_node_id(2);
    let mut rectangle =
        NodeSpec::rectangle(rectangle_id, "Proof Rectangle", Vec2::new(240.0, 140.0));
    rectangle.local_transform = Affine2::translation(Vec2::new(-160.0, -80.0));
    let mut ellipse = NodeSpec::ellipse(ellipse_id, "Proof Ellipse", Vec2::new(180.0, 180.0));
    ellipse.local_transform = Affine2::translation(Vec2::new(70.0, -50.0));
    Document::from_snapshot(DocumentSnapshot {
        root_id,
        nodes: vec![
            NodeSnapshot {
                spec: NodeSpec::document(root_id, "Phase 0D Preview"),
                parent: None,
                children: vec![rectangle_id, ellipse_id],
                group_restoration: None,
            },
            NodeSnapshot {
                spec: rectangle,
                parent: Some(root_id),
                children: Vec::new(),
                group_restoration: None,
            },
            NodeSnapshot {
                spec: ellipse,
                parent: Some(root_id),
                children: Vec::new(),
                group_restoration: None,
            },
        ],
    })
}
fn flat_rectangles(count: usize, spacing: f64) -> Result<Document, DocumentError> {
    let root_id = fixture_node_id(0);
    let children: Vec<_> = (1..=count as u128).map(fixture_node_id).collect();
    let mut nodes = Vec::with_capacity(count + 1);
    nodes.push(NodeSnapshot {
        spec: NodeSpec::document(root_id, "Proof Root"),
        parent: None,
        children: children.clone(),
        group_restoration: None,
    });
    let columns = (count as f64).sqrt().ceil() as usize;
    for (index, id) in children.into_iter().enumerate() {
        let mut spec = NodeSpec::rectangle(id, format!("Rectangle {index}"), Vec2::new(10.0, 10.0));
        let x = (index % columns) as f64 * spacing;
        let y = (index / columns) as f64 * spacing;
        spec.local_transform = Affine2::translation(Vec2::new(x, y));
        nodes.push(NodeSnapshot {
            spec,
            parent: Some(root_id),
            children: Vec::new(),
            group_restoration: None,
        });
    }
    Document::from_snapshot(DocumentSnapshot { root_id, nodes })
}

fn deep_hierarchy(depth: usize) -> Result<Document, DocumentError> {
    let root_id = fixture_node_id(0);
    let mut nodes = Vec::with_capacity(depth + 1);
    nodes.push(NodeSnapshot {
        spec: NodeSpec::document(root_id, "Deep Root"),
        parent: None,
        children: (depth > 0)
            .then(|| fixture_node_id(1))
            .into_iter()
            .collect(),
        group_restoration: None,
    });
    for level in 1..=depth {
        let id = fixture_node_id(level as u128);
        let parent = fixture_node_id(level as u128 - 1);
        let children = (level < depth)
            .then(|| fixture_node_id(level as u128 + 1))
            .into_iter()
            .collect();
        nodes.push(NodeSnapshot {
            spec: NodeSpec::group(id, format!("Depth {level}")),
            parent: Some(parent),
            children,
            group_restoration: None,
        });
    }
    Document::from_snapshot(DocumentSnapshot { root_id, nodes })
}

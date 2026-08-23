//! Deterministic structural proof fixtures. Construction uses the validated load boundary,
//! not the normal incremental editing path.

use uuid::Uuid;
use visual_authoring_core_math::{Affine2, Vec2};
use visual_authoring_document::{
    Appearance, ColorRgba, Document, DocumentError, DocumentSnapshot, NodeId, NodeSnapshot,
    NodeSpec, PathAnchor, PathAnchorId, PathGeometry, Stroke,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureKind {
    Preview,
    Editor,
    BenchA,
    BenchB,
    BenchC,
    BenchD,
    /// Phase 2A path proof: an open straight path, an open cubic, a closed filled path and a
    /// closed stroked cubic.
    PathA,
}

impl FixtureKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Preview => "PREVIEW",
            Self::Editor => "EDITOR",
            Self::BenchA => "BENCH-A",
            Self::BenchB => "BENCH-B",
            Self::BenchC => "BENCH-C",
            Self::BenchD => "BENCH-D",
            Self::PathA => "PATH-A",
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
        FixtureKind::Editor => editor_document(),
        FixtureKind::BenchA => flat_rectangles(1_000, 40.0),
        FixtureKind::BenchB => flat_rectangles(10_000, 40.0),
        FixtureKind::BenchC => flat_rectangles(100_000, 512.0),
        FixtureKind::BenchD => deep_hierarchy(10_000),
        FixtureKind::PathA => path_proof(),
    }
}

/// Node index of each path in the `PATH-A` fixture, in document order.
pub const PATH_A_OPEN_STRAIGHT: u128 = 1;
pub const PATH_A_OPEN_CUBIC: u128 = 2;
pub const PATH_A_CLOSED_FILLED: u128 = 3;
pub const PATH_A_CLOSED_STROKED_CUBIC: u128 = 4;

fn path_anchor(index: u128, position: Vec2) -> PathAnchor {
    PathAnchor::new(PathAnchorId::from_uuid(Uuid::from_u128(index)), position)
}

/// Deterministic path fixture. Every anchor UUID is fixed so a proof can compare identities
/// across a save and load round trip.
fn path_proof() -> Result<Document, DocumentError> {
    let root_id = fixture_node_id(0);
    let ids = [
        PATH_A_OPEN_STRAIGHT,
        PATH_A_OPEN_CUBIC,
        PATH_A_CLOSED_FILLED,
        PATH_A_CLOSED_STROKED_CUBIC,
    ]
    .map(fixture_node_id);

    let stroke_color = ColorRgba::new(0.05, 0.07, 0.11, 1.0);
    let stroked = |width: f64| Appearance {
        fill: ColorRgba::new(0.0, 0.0, 0.0, 0.0),
        stroke: Stroke {
            color: stroke_color,
            width,
        },
        ..Appearance::default()
    };

    // 1. Open straight path: two anchors, no handles.
    let mut straight = NodeSpec::path(
        ids[0],
        "Open Straight",
        PathGeometry {
            closed: false,
            anchors: vec![
                path_anchor(0x2a01, Vec2::new(0.0, 0.0)),
                path_anchor(0x2a02, Vec2::new(240.0, 0.0)),
            ],
        },
    );
    straight.local_transform = Affine2::translation(Vec2::new(-360.0, -220.0));
    straight.appearance = stroked(6.0);

    // 2. Open cubic: one curve with both handles.
    let mut curve_anchors = vec![
        path_anchor(0x2a03, Vec2::new(0.0, 0.0)),
        path_anchor(0x2a04, Vec2::new(240.0, 0.0)),
    ];
    curve_anchors[0].handle_out = Some(Vec2::new(80.0, -140.0));
    curve_anchors[1].handle_in = Some(Vec2::new(160.0, 140.0));
    let mut cubic = NodeSpec::path(
        ids[1],
        "Open Cubic",
        PathGeometry {
            closed: false,
            anchors: curve_anchors,
        },
    );
    cubic.local_transform = Affine2::translation(Vec2::new(-360.0, -60.0));
    cubic.appearance = stroked(6.0);

    // 3. Closed filled triangle: solid fill, no stroke.
    let mut filled = NodeSpec::path(
        ids[2],
        "Closed Filled",
        PathGeometry {
            closed: true,
            anchors: vec![
                path_anchor(0x2a05, Vec2::new(0.0, 0.0)),
                path_anchor(0x2a06, Vec2::new(200.0, 0.0)),
                path_anchor(0x2a07, Vec2::new(100.0, 170.0)),
            ],
        },
    );
    filled.local_transform = Affine2::translation(Vec2::new(80.0, -220.0));
    filled.appearance = Appearance {
        fill: ColorRgba::new(0.16, 0.55, 0.95, 1.0),
        ..Appearance::default()
    };

    // 4. Closed stroked cubic: a curved outline that is both filled and stroked.
    let mut closed_anchors = vec![
        path_anchor(0x2a08, Vec2::new(100.0, 0.0)),
        path_anchor(0x2a09, Vec2::new(200.0, 100.0)),
        path_anchor(0x2a0a, Vec2::new(100.0, 200.0)),
        path_anchor(0x2a0b, Vec2::new(0.0, 100.0)),
    ];
    closed_anchors[0].handle_in = Some(Vec2::new(45.0, 0.0));
    closed_anchors[0].handle_out = Some(Vec2::new(155.0, 0.0));
    closed_anchors[1].handle_in = Some(Vec2::new(200.0, 45.0));
    closed_anchors[1].handle_out = Some(Vec2::new(200.0, 155.0));
    closed_anchors[2].handle_in = Some(Vec2::new(155.0, 200.0));
    closed_anchors[2].handle_out = Some(Vec2::new(45.0, 200.0));
    closed_anchors[3].handle_in = Some(Vec2::new(0.0, 155.0));
    closed_anchors[3].handle_out = Some(Vec2::new(0.0, 45.0));
    let mut closed_cubic = NodeSpec::path(
        ids[3],
        "Closed Stroked Cubic",
        PathGeometry {
            closed: true,
            anchors: closed_anchors,
        },
    );
    closed_cubic.local_transform = Affine2::translation(Vec2::new(80.0, -20.0));
    closed_cubic.appearance = Appearance {
        fill: ColorRgba::new(0.98, 0.72, 0.18, 1.0),
        stroke: Stroke {
            color: stroke_color,
            width: 8.0,
        },
        ..Appearance::default()
    };

    Document::from_snapshot(DocumentSnapshot {
        root_id,
        nodes: vec![
            NodeSnapshot {
                spec: NodeSpec::document(root_id, "Phase 2A Path Proof"),
                parent: None,
                children: ids.to_vec(),
                group_restoration: None,
            },
            NodeSnapshot {
                spec: straight,
                parent: Some(root_id),
                children: Vec::new(),
                group_restoration: None,
            },
            NodeSnapshot {
                spec: cubic,
                parent: Some(root_id),
                children: Vec::new(),
                group_restoration: None,
            },
            NodeSnapshot {
                spec: filled,
                parent: Some(root_id),
                children: Vec::new(),
                group_restoration: None,
            },
            NodeSnapshot {
                spec: closed_cubic,
                parent: Some(root_id),
                children: Vec::new(),
                group_restoration: None,
            },
        ],
    })
}

fn editor_document() -> Result<Document, DocumentError> {
    let root_id = fixture_node_id(0);
    let frame_id = fixture_node_id(1);
    let mut frame = NodeSpec::frame(frame_id, "Frame 1920×1080", Vec2::new(1920.0, 1080.0));
    frame.local_transform = Affine2::translation(Vec2::new(-960.0, -540.0));
    frame.appearance = Appearance {
        fill: ColorRgba::new(0.97, 0.975, 0.985, 1.0),
        stroke: Stroke {
            width: 1.0,
            ..Stroke::default()
        },
        ..Appearance::default()
    };
    Document::from_snapshot(DocumentSnapshot {
        root_id,
        nodes: vec![
            NodeSnapshot {
                spec: NodeSpec::document(root_id, "EditorM"),
                parent: None,
                children: vec![frame_id],
                group_restoration: None,
            },
            NodeSnapshot {
                spec: frame,
                parent: Some(root_id),
                children: Vec::new(),
                group_restoration: None,
            },
        ],
    })
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

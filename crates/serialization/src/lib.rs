//! Versioned persistence boundary for visual-authoring documents.
//!
//! The JSON schema is intentionally represented by private `Stored*` types
//! instead of deriving serialization for `Document`. Loading always crosses
//! `Document::from_snapshot`, so malformed hierarchy data cannot enter the
//! persistent model.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use visual_authoring_core_math::{Affine2, Vec2};
use visual_authoring_document::{
    Appearance, ColorRgba, CornerRadii, DashPattern, DashPatternError, Document, DocumentError,
    DocumentSnapshot, Geometry, GroupRestoration, GroupRestorationRun, NodeId, NodeKind,
    NodeSnapshot, NodeSpec, PathAnchor, PathAnchorId, PathGeometry, Stroke, StrokeCap, StrokeJoin,
};

pub const DOCUMENT_FORMAT: &str = "visual-authoring-document";
pub const CURRENT_VERSION: u32 = 4;

#[derive(Debug, Error)]
pub enum SerializationError {
    #[error("invalid JSON document: {0}")]
    Json(#[from] serde_json::Error),
    #[error("missing or invalid top-level field `{0}`")]
    InvalidEnvelopeField(&'static str),
    #[error("unsupported document format `{0}`")]
    UnsupportedFormat(String),
    #[error("unsupported document version {0}")]
    UnsupportedVersion(u64),
    #[error("document data is invalid: {0}")]
    InvalidDocument(#[from] DocumentError),
    #[error("invalid stroke dash pattern: {0}")]
    InvalidDashPattern(#[from] DashPatternError),
}

/// Serializes a validated semantic document using the current versioned schema.
pub fn to_json_pretty(document: &Document) -> Result<String, SerializationError> {
    document
        .validate_invariants()
        .map_err(DocumentError::from)?;
    let envelope = StoredEnvelopeV1::from_document(document)?;
    Ok(serde_json::to_string_pretty(&envelope)?)
}

/// Deserializes and validates a versioned semantic document.
pub fn from_json(json: &str) -> Result<Document, SerializationError> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let format = value
        .get("format")
        .and_then(serde_json::Value::as_str)
        .ok_or(SerializationError::InvalidEnvelopeField("format"))?;
    if format != DOCUMENT_FORMAT {
        return Err(SerializationError::UnsupportedFormat(format.to_owned()));
    }

    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(SerializationError::InvalidEnvelopeField("version"))?;
    match version {
        1..=4 => serde_json::from_value::<StoredEnvelopeV1>(value)?.into_document(),
        unsupported => Err(SerializationError::UnsupportedVersion(unsupported)),
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredEnvelopeV1 {
    format: String,
    version: u32,
    document: StoredDocumentV1,
}

impl StoredEnvelopeV1 {
    fn from_document(document: &Document) -> Result<Self, SerializationError> {
        let snapshot = document.snapshot();
        let nodes = snapshot
            .nodes
            .into_iter()
            .map(StoredNodeV1::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            format: DOCUMENT_FORMAT.to_owned(),
            version: CURRENT_VERSION,
            document: StoredDocumentV1 {
                root_id: snapshot.root_id,
                nodes,
            },
        })
    }

    fn into_document(self) -> Result<Document, SerializationError> {
        if self.format != DOCUMENT_FORMAT {
            return Err(SerializationError::UnsupportedFormat(self.format));
        }
        if !matches!(self.version, 1..=4) {
            return Err(SerializationError::UnsupportedVersion(u64::from(
                self.version,
            )));
        }

        let nodes = self
            .document
            .nodes
            .into_iter()
            .map(StoredNodeV1::into_snapshot)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Document::from_snapshot(DocumentSnapshot {
            root_id: self.document.root_id,
            nodes,
        })?)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredDocumentV1 {
    root_id: NodeId,
    nodes: Vec<StoredNodeV1>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredNodeV1 {
    id: NodeId,
    name: String,
    kind: StoredNodeKindV1,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    local_transform: StoredAffine2V1,
    visible: bool,
    locked: bool,
    appearance: StoredAppearanceV1,
    metadata: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    internal_group_restoration: Option<StoredGroupRestorationV1>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredGroupRestorationV1 {
    version: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    entries: Vec<StoredGroupRestorationEntryV1>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    runs: Vec<StoredGroupRestorationRunV2>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredGroupRestorationEntryV1 {
    child: NodeId,
    before_anchor: Option<NodeId>,
    after_anchor: Option<NodeId>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredGroupRestorationRunV2 {
    children: Vec<NodeId>,
    before_anchor: Option<NodeId>,
    after_anchor: Option<NodeId>,
}

impl From<GroupRestoration> for StoredGroupRestorationV1 {
    fn from(restoration: GroupRestoration) -> Self {
        Self {
            version: restoration.version,
            entries: Vec::new(),
            runs: restoration
                .runs
                .into_iter()
                .map(|run| StoredGroupRestorationRunV2 {
                    children: run.children,
                    before_anchor: run.before_anchor,
                    after_anchor: run.after_anchor,
                })
                .collect(),
        }
    }
}

impl From<StoredGroupRestorationV1> for GroupRestoration {
    fn from(restoration: StoredGroupRestorationV1) -> Self {
        if restoration.version == 1 {
            let mut runs: Vec<GroupRestorationRun> = Vec::new();
            for entry in restoration.entries {
                if let Some(run) = runs.last_mut().filter(|run| {
                    run.before_anchor == entry.before_anchor
                        && run.after_anchor == entry.after_anchor
                }) {
                    run.children.push(entry.child);
                } else {
                    runs.push(GroupRestorationRun {
                        children: vec![entry.child],
                        before_anchor: entry.before_anchor,
                        after_anchor: entry.after_anchor,
                    });
                }
            }
            Self {
                version: GroupRestoration::VERSION,
                runs,
            }
        } else {
            Self {
                version: restoration.version,
                runs: restoration
                    .runs
                    .into_iter()
                    .map(|run| GroupRestorationRun {
                        children: run.children,
                        before_anchor: run.before_anchor,
                        after_anchor: run.after_anchor,
                    })
                    .collect(),
            }
        }
    }
}

impl TryFrom<NodeSnapshot> for StoredNodeV1 {
    type Error = SerializationError;

    fn try_from(snapshot: NodeSnapshot) -> Result<Self, Self::Error> {
        let kind = StoredNodeKindV1::from_spec(&snapshot.spec)?;
        Ok(Self {
            id: snapshot.spec.id,
            name: snapshot.spec.name,
            kind,
            parent: snapshot.parent,
            children: snapshot.children,
            local_transform: snapshot.spec.local_transform.into(),
            visible: snapshot.spec.visible,
            locked: snapshot.spec.locked,
            appearance: snapshot.spec.appearance.into(),
            metadata: snapshot.spec.metadata,
            internal_group_restoration: snapshot.group_restoration.map(Into::into),
        })
    }
}

impl StoredNodeV1 {
    fn into_snapshot(self) -> Result<NodeSnapshot, SerializationError> {
        let (kind, geometry) = self.kind.into_domain();
        Ok(NodeSnapshot {
            spec: NodeSpec {
                id: self.id,
                name: self.name,
                kind,
                local_transform: self.local_transform.into(),
                visible: self.visible,
                locked: self.locked,
                geometry,
                appearance: self.appearance.try_into()?,
                metadata: self.metadata,
            },
            parent: self.parent,
            children: self.children,
            group_restoration: self.internal_group_restoration.map(Into::into),
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StoredNodeKindV1 {
    Document,
    Frame {
        width: f64,
        height: f64,
    },
    Group,
    Rectangle {
        width: f64,
        height: f64,
    },
    Ellipse {
        width: f64,
        height: f64,
    },
    Path {
        closed: bool,
        anchors: Vec<StoredPathAnchorV3>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredPathAnchorV3 {
    id: PathAnchorId,
    position: StoredPointV3,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    handle_in: Option<StoredPointV3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    handle_out: Option<StoredPointV3>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredPointV3 {
    x: f64,
    y: f64,
}

impl From<Vec2> for StoredPointV3 {
    fn from(point: Vec2) -> Self {
        Self {
            x: point.x,
            y: point.y,
        }
    }
}

impl From<StoredPointV3> for Vec2 {
    fn from(point: StoredPointV3) -> Self {
        Self::new(point.x, point.y)
    }
}

impl StoredNodeKindV1 {
    fn from_spec(spec: &NodeSpec) -> Result<Self, SerializationError> {
        let invalid = || DocumentError::InvalidGeometry(spec.id).into();
        match (spec.kind, spec.geometry.as_ref()) {
            (NodeKind::Document, None) => Ok(Self::Document),
            (NodeKind::Frame, Some(Geometry::Frame { size })) => Ok(Self::Frame {
                width: size.x,
                height: size.y,
            }),
            (NodeKind::Group, None) => Ok(Self::Group),
            (NodeKind::Rectangle, Some(Geometry::Rectangle { size })) => Ok(Self::Rectangle {
                width: size.x,
                height: size.y,
            }),
            (NodeKind::Ellipse, Some(Geometry::Ellipse { size })) => Ok(Self::Ellipse {
                width: size.x,
                height: size.y,
            }),
            (NodeKind::Path, Some(Geometry::Path(path))) => Ok(Self::Path {
                closed: path.closed,
                anchors: path
                    .anchors
                    .iter()
                    .map(|anchor| StoredPathAnchorV3 {
                        id: anchor.id,
                        position: anchor.position.into(),
                        handle_in: anchor.handle_in.map(Into::into),
                        handle_out: anchor.handle_out.map(Into::into),
                    })
                    .collect(),
            }),
            _ => Err(invalid()),
        }
    }

    fn into_domain(self) -> (NodeKind, Option<Geometry>) {
        match self {
            Self::Document => (NodeKind::Document, None),
            Self::Frame { width, height } => (
                NodeKind::Frame,
                Some(Geometry::Frame {
                    size: Vec2::new(width, height),
                }),
            ),
            Self::Group => (NodeKind::Group, None),
            Self::Rectangle { width, height } => (
                NodeKind::Rectangle,
                Some(Geometry::Rectangle {
                    size: Vec2::new(width, height),
                }),
            ),
            Self::Ellipse { width, height } => (
                NodeKind::Ellipse,
                Some(Geometry::Ellipse {
                    size: Vec2::new(width, height),
                }),
            ),
            Self::Path { closed, anchors } => (
                NodeKind::Path,
                Some(Geometry::Path(PathGeometry {
                    closed,
                    anchors: anchors
                        .into_iter()
                        .map(|anchor| PathAnchor {
                            id: anchor.id,
                            position: anchor.position.into(),
                            handle_in: anchor.handle_in.map(Into::into),
                            handle_out: anchor.handle_out.map(Into::into),
                        })
                        .collect(),
                })),
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredAffine2V1 {
    m11: f64,
    m12: f64,
    m21: f64,
    m22: f64,
    tx: f64,
    ty: f64,
}

impl From<Affine2> for StoredAffine2V1 {
    fn from(transform: Affine2) -> Self {
        Self {
            m11: transform.m11,
            m12: transform.m12,
            m21: transform.m21,
            m22: transform.m22,
            tx: transform.tx,
            ty: transform.ty,
        }
    }
}

impl From<StoredAffine2V1> for Affine2 {
    fn from(transform: StoredAffine2V1) -> Self {
        Self::from_components(
            transform.m11,
            transform.m12,
            transform.m21,
            transform.m22,
            transform.tx,
            transform.ty,
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredAppearanceV1 {
    opacity: f64,
    #[serde(default)]
    fill: StoredColorV2,
    #[serde(default)]
    corner_radii: StoredCornerRadiiV2,
    #[serde(default)]
    stroke: StoredStrokeV2,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredColorV2 {
    r: f64,
    g: f64,
    b: f64,
    a: f64,
}

impl Default for StoredColorV2 {
    fn default() -> Self {
        ColorRgba::default().into()
    }
}

impl From<ColorRgba> for StoredColorV2 {
    fn from(color: ColorRgba) -> Self {
        Self {
            r: color.r,
            g: color.g,
            b: color.b,
            a: color.a,
        }
    }
}

impl From<StoredColorV2> for ColorRgba {
    fn from(color: StoredColorV2) -> Self {
        Self::new(color.r, color.g, color.b, color.a)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredCornerRadiiV2 {
    top_left: f64,
    top_right: f64,
    bottom_right: f64,
    bottom_left: f64,
}

impl From<CornerRadii> for StoredCornerRadiiV2 {
    fn from(radii: CornerRadii) -> Self {
        Self {
            top_left: radii.top_left,
            top_right: radii.top_right,
            bottom_right: radii.bottom_right,
            bottom_left: radii.bottom_left,
        }
    }
}

impl From<StoredCornerRadiiV2> for CornerRadii {
    fn from(radii: StoredCornerRadiiV2) -> Self {
        Self {
            top_left: radii.top_left,
            top_right: radii.top_right,
            bottom_right: radii.bottom_right,
            bottom_left: radii.bottom_left,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredStrokeV2 {
    color: StoredColorV2,
    width: f64,
    #[serde(default)]
    cap: StoredStrokeCapV4,
    #[serde(default)]
    join: StoredStrokeJoinV4,
    #[serde(default = "default_miter_limit")]
    miter_limit: f64,
    #[serde(default)]
    dash_pattern: Vec<f64>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum StoredStrokeCapV4 {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum StoredStrokeJoinV4 {
    #[default]
    Miter,
    Round,
    Bevel,
}

const fn default_miter_limit() -> f64 {
    4.0
}

impl Default for StoredStrokeV2 {
    fn default() -> Self {
        Stroke::default().into()
    }
}

impl From<Stroke> for StoredStrokeV2 {
    fn from(stroke: Stroke) -> Self {
        Self {
            color: stroke.color.into(),
            width: stroke.width,
            cap: match stroke.cap {
                StrokeCap::Butt => StoredStrokeCapV4::Butt,
                StrokeCap::Round => StoredStrokeCapV4::Round,
                StrokeCap::Square => StoredStrokeCapV4::Square,
            },
            join: match stroke.join {
                StrokeJoin::Miter => StoredStrokeJoinV4::Miter,
                StrokeJoin::Round => StoredStrokeJoinV4::Round,
                StrokeJoin::Bevel => StoredStrokeJoinV4::Bevel,
            },
            miter_limit: stroke.miter_limit,
            dash_pattern: stroke.dash_pattern.as_slice().to_vec(),
        }
    }
}

impl TryFrom<StoredStrokeV2> for Stroke {
    type Error = SerializationError;

    fn try_from(stroke: StoredStrokeV2) -> Result<Self, Self::Error> {
        Ok(Self {
            color: stroke.color.into(),
            width: stroke.width,
            cap: match stroke.cap {
                StoredStrokeCapV4::Butt => StrokeCap::Butt,
                StoredStrokeCapV4::Round => StrokeCap::Round,
                StoredStrokeCapV4::Square => StrokeCap::Square,
            },
            join: match stroke.join {
                StoredStrokeJoinV4::Miter => StrokeJoin::Miter,
                StoredStrokeJoinV4::Round => StrokeJoin::Round,
                StoredStrokeJoinV4::Bevel => StrokeJoin::Bevel,
            },
            miter_limit: stroke.miter_limit,
            dash_pattern: DashPattern::new(&stroke.dash_pattern)?,
        })
    }
}

impl From<Appearance> for StoredAppearanceV1 {
    fn from(appearance: Appearance) -> Self {
        Self {
            opacity: appearance.opacity,
            fill: appearance.fill.into(),
            corner_radii: appearance.corner_radii.into(),
            stroke: appearance.stroke.into(),
        }
    }
}

impl TryFrom<StoredAppearanceV1> for Appearance {
    type Error = SerializationError;

    fn try_from(appearance: StoredAppearanceV1) -> Result<Self, Self::Error> {
        Ok(Self {
            fill: appearance.fill.into(),
            opacity: appearance.opacity,
            corner_radii: appearance.corner_radii.into(),
            stroke: appearance.stroke.try_into()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use visual_authoring_document::{Command, HeadlessEditorCore};

    #[test]
    fn current_path_round_trip_preserves_anchor_ids_and_handles() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let path_id = NodeId::new();
        let first = PathAnchorId::new();
        let second = PathAnchorId::new();
        let path = PathGeometry {
            closed: false,
            anchors: vec![
                PathAnchor {
                    id: first,
                    position: Vec2::new(5.0, 10.0),
                    handle_in: None,
                    handle_out: Some(Vec2::new(15.0, 20.0)),
                },
                PathAnchor {
                    id: second,
                    position: Vec2::new(40.0, 50.0),
                    handle_in: Some(Vec2::new(30.0, 35.0)),
                    handle_out: None,
                },
            ],
        };
        editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::path(path_id, "Curve", path),
                parent: root,
                index: 0,
            })
            .unwrap();

        let encoded = to_json_pretty(editor.document()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["version"], CURRENT_VERSION);
        let restored = from_json(&encoded).unwrap();
        assert_eq!(restored, *editor.document());
    }

    #[test]
    fn saving_a_loaded_path_document_reproduces_the_same_bytes() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let mut anchors = vec![
            PathAnchor::new(PathAnchorId::new(), Vec2::new(0.0, 0.0)),
            PathAnchor::new(PathAnchorId::new(), Vec2::new(120.0, 0.0)),
            PathAnchor::new(PathAnchorId::new(), Vec2::new(60.0, 90.0)),
        ];
        anchors[1].handle_in = Some(Vec2::new(40.0, -30.0));
        anchors[2].handle_out = Some(Vec2::new(10.0, 70.0));
        let identities = anchors.iter().map(|anchor| anchor.id).collect::<Vec<_>>();
        editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::path(
                    NodeId::new(),
                    "Closed curve",
                    PathGeometry {
                        closed: true,
                        anchors,
                    },
                ),
                parent: root,
                index: 0,
            })
            .unwrap();

        let first = to_json_pretty(editor.document()).unwrap();
        let loaded = from_json(&first).unwrap();
        let second = to_json_pretty(&loaded).unwrap();
        assert_eq!(first, second, "save -> load -> save must be byte stable");

        let stored = loaded
            .nodes()
            .find_map(|node| match node.geometry() {
                Some(Geometry::Path(path)) => Some(path.clone()),
                _ => None,
            })
            .expect("the reloaded document keeps its path");
        assert_eq!(
            stored
                .anchors
                .iter()
                .map(|anchor| anchor.id)
                .collect::<Vec<_>>(),
            identities,
            "anchor identities survive a round trip"
        );
        assert!(stored.closed);
        assert_eq!(stored.anchors[1].handle_in, Some(Vec2::new(40.0, -30.0)));
        assert_eq!(stored.anchors[2].handle_out, Some(Vec2::new(10.0, 70.0)));
    }

    #[test]
    fn duplicate_path_anchor_ids_are_rejected_during_load() {
        let mut editor = HeadlessEditorCore::blank("Root");
        let root = editor.document().root_id();
        let anchor_id = PathAnchorId::new();
        editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::path(
                    NodeId::new(),
                    "Path",
                    PathGeometry {
                        closed: false,
                        anchors: vec![
                            PathAnchor::new(anchor_id, Vec2::ZERO),
                            PathAnchor::new(PathAnchorId::new(), Vec2::new(10.0, 10.0)),
                        ],
                    },
                ),
                parent: root,
                index: 0,
            })
            .unwrap();
        let mut value: serde_json::Value =
            serde_json::from_str(&to_json_pretty(editor.document()).unwrap()).unwrap();
        let path = value["document"]["nodes"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|node| node["kind"]["type"] == "path")
            .unwrap();
        path["kind"]["anchors"][1]["id"] = path["kind"]["anchors"][0]["id"].clone();

        assert!(matches!(
            from_json(&value.to_string()),
            Err(SerializationError::InvalidDocument(
                DocumentError::InvalidGeometry(_)
            ))
        ));
    }

    #[test]
    fn versions_one_through_three_remain_loadable_after_version_four_bump() {
        let (document, ..) = sample_document();
        let current: serde_json::Value =
            serde_json::from_str(&to_json_pretty(&document).unwrap()).unwrap();
        for legacy_version in [1_u64, 2, 3] {
            let mut legacy = current.clone();
            legacy["version"] = legacy_version.into();
            assert_eq!(from_json(&legacy.to_string()).unwrap(), document);
        }
    }

    fn sample_document() -> (Document, NodeId, NodeId, NodeId) {
        let root_id = NodeId::new();
        let mut root = NodeSpec::document(root_id, "Broadcast package");
        root.metadata.insert("author".into(), "Codex".into());
        let document = Document::with_root(root).unwrap();
        let mut editor = HeadlessEditorCore::new(document).unwrap();

        let frame_id = NodeId::new();
        let rectangle_id = NodeId::new();
        let ellipse_id = NodeId::new();
        editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::frame(frame_id, "Main frame", Vec2::new(1920.0, 1080.0)),
                parent: root_id,
                index: 0,
            })
            .unwrap();
        editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::rectangle(rectangle_id, "Background", Vec2::new(1920.0, 1080.0)),
                parent: frame_id,
                index: 0,
            })
            .unwrap();
        editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::ellipse(ellipse_id, "Badge", Vec2::new(120.0, 80.0)),
                parent: frame_id,
                index: 1,
            })
            .unwrap();
        editor
            .dispatch(Command::SetLocalTransform {
                target: ellipse_id,
                transform: Affine2::translation(Vec2::new(140.0, 90.0))
                    * Affine2::rotation(0.42)
                    * Affine2::scale(Vec2::new(-1.2, 0.75)),
            })
            .unwrap();
        (
            editor.into_document().unwrap(),
            frame_id,
            rectangle_id,
            ellipse_id,
        )
    }

    #[test]
    fn stable_ids_and_semantics_survive_round_trip() {
        let (document, frame_id, rectangle_id, ellipse_id) = sample_document();
        let json = to_json_pretty(&document).unwrap();
        let restored = from_json(&json).unwrap();

        assert_eq!(restored, document);
        assert!(restored.node(frame_id).is_some());
        assert!(restored.node(rectangle_id).is_some());
        assert!(restored.node(ellipse_id).is_some());
    }

    #[test]
    fn child_order_persists() {
        let (document, frame_id, rectangle_id, ellipse_id) = sample_document();
        let restored = from_json(&to_json_pretty(&document).unwrap()).unwrap();
        assert_eq!(
            restored.node(frame_id).unwrap().children(),
            &[rectangle_id, ellipse_id]
        );
    }

    #[test]
    fn reordered_child_order_persists_after_save_and_load() {
        let (document, frame_id, rectangle_id, ellipse_id) = sample_document();
        let mut editor = HeadlessEditorCore::new(document).unwrap();
        editor
            .dispatch(Command::Reparent {
                child: rectangle_id,
                new_parent: frame_id,
                index: 1,
            })
            .unwrap();
        assert_eq!(
            editor.document().node(frame_id).unwrap().children(),
            &[ellipse_id, rectangle_id]
        );

        let restored = from_json(&to_json_pretty(editor.document()).unwrap()).unwrap();
        assert_eq!(
            restored.node(frame_id).unwrap().children(),
            &[ellipse_id, rectangle_id]
        );
    }

    #[test]
    fn envelope_is_explicit_and_versioned() {
        let (document, ..) = sample_document();
        let value: serde_json::Value =
            serde_json::from_str(&to_json_pretty(&document).unwrap()).unwrap();
        assert_eq!(value["format"], DOCUMENT_FORMAT);
        assert_eq!(value["version"], CURRENT_VERSION);
        assert!(value["document"]["nodes"].is_array());
    }

    #[test]
    fn unsupported_format_and_version_are_rejected() {
        let (document, ..) = sample_document();
        let mut value: serde_json::Value =
            serde_json::from_str(&to_json_pretty(&document).unwrap()).unwrap();
        value["format"] = "other-format".into();
        assert!(matches!(
            from_json(&value.to_string()),
            Err(SerializationError::UnsupportedFormat(_))
        ));

        value["format"] = DOCUMENT_FORMAT.into();
        value["version"] = 99_u64.into();
        assert!(matches!(
            from_json(&value.to_string()),
            Err(SerializationError::UnsupportedVersion(99))
        ));
    }

    #[test]
    fn dangling_parent_data_is_rejected() {
        let (document, _frame_id, _rectangle_id, ellipse_id) = sample_document();
        let mut envelope = StoredEnvelopeV1::from_document(&document).unwrap();
        let ellipse = envelope
            .document
            .nodes
            .iter_mut()
            .find(|node| node.id == ellipse_id)
            .unwrap();
        ellipse.parent = Some(NodeId::new());

        let json = serde_json::to_string(&envelope).unwrap();
        assert!(matches!(
            from_json(&json),
            Err(SerializationError::InvalidDocument(_))
        ));
    }

    #[test]
    fn cyclic_hierarchy_data_is_rejected() {
        let root_id = NodeId::new();
        let first_id = NodeId::new();
        let second_id = NodeId::new();
        let identity = StoredAffine2V1::from(Affine2::IDENTITY);
        let appearance: StoredAppearanceV1 = Appearance {
            opacity: 1.0,
            ..Appearance::default()
        }
        .into();
        let node = |id, name: &str, parent, children| StoredNodeV1 {
            id,
            name: name.into(),
            kind: StoredNodeKindV1::Group,
            parent,
            children,
            local_transform: identity,
            visible: true,
            locked: false,
            appearance: appearance.clone(),
            metadata: BTreeMap::new(),
            internal_group_restoration: None,
        };
        let envelope = StoredEnvelopeV1 {
            format: DOCUMENT_FORMAT.into(),
            version: CURRENT_VERSION,
            document: StoredDocumentV1 {
                root_id,
                nodes: vec![
                    StoredNodeV1 {
                        id: root_id,
                        name: "Root".into(),
                        kind: StoredNodeKindV1::Document,
                        parent: None,
                        children: Vec::new(),
                        local_transform: identity,
                        visible: true,
                        locked: false,
                        appearance: appearance.clone(),
                        metadata: BTreeMap::new(),
                        internal_group_restoration: None,
                    },
                    node(first_id, "First", Some(second_id), vec![second_id]),
                    node(second_id, "Second", Some(first_id), vec![first_id]),
                ],
            },
        };

        assert!(matches!(
            from_json(&serde_json::to_string(&envelope).unwrap()),
            Err(SerializationError::InvalidDocument(
                DocumentError::Invariant(visual_authoring_document::InvariantViolation::Cycle(_))
            ))
        ));
    }

    #[test]
    fn duplicate_ids_are_rejected_during_load() {
        let (document, ..) = sample_document();
        let mut envelope = StoredEnvelopeV1::from_document(&document).unwrap();
        let duplicate = StoredNodeV1 {
            id: envelope.document.nodes[0].id,
            name: "Duplicate".into(),
            kind: StoredNodeKindV1::Group,
            parent: None,
            children: Vec::new(),
            local_transform: Affine2::IDENTITY.into(),
            visible: true,
            locked: false,
            appearance: Appearance::default().into(),
            metadata: BTreeMap::new(),
            internal_group_restoration: None,
        };
        envelope.document.nodes.push(duplicate);
        assert!(matches!(
            from_json(&serde_json::to_string(&envelope).unwrap()),
            Err(SerializationError::InvalidDocument(
                DocumentError::DuplicateNodeId(_)
            ))
        ));
    }

    #[test]
    fn save_load_save_is_semantically_and_textually_stable() {
        let (document, ..) = sample_document();
        let first = to_json_pretty(&document).unwrap();
        let second = to_json_pretty(&from_json(&first).unwrap()).unwrap();
        assert_eq!(second, first);
    }
    #[test]
    fn editor_session_history_selection_and_transaction_are_not_serialized() {
        let (document, _frame_id, rectangle_id, _ellipse_id) = sample_document();
        let mut editor = HeadlessEditorCore::new(document).unwrap();
        editor.select_only(rectangle_id).unwrap();
        editor
            .dispatch(Command::SetName {
                target: rectangle_id,
                name: "History exists".into(),
            })
            .unwrap();
        editor.begin_transaction().unwrap();
        editor
            .update_transaction(Command::SetVisible {
                target: rectangle_id,
                visible: false,
            })
            .unwrap();

        let value: serde_json::Value =
            serde_json::from_str(&to_json_pretty(editor.document()).unwrap()).unwrap();
        assert!(value.get("history").is_none());
        assert!(value.get("selection").is_none());
        assert!(value.get("transaction").is_none());
        assert!(value["document"].get("history").is_none());
        assert!(value["document"].get("selection").is_none());
        assert!(value["document"].get("transaction").is_none());
        assert_eq!(value["version"], CURRENT_VERSION);
    }

    #[test]
    fn failed_load_preserves_editor_and_successful_replacement_resets_session_policy() {
        let (document, _frame_id, rectangle_id, _ellipse_id) = sample_document();
        let mut editor = HeadlessEditorCore::new(document).unwrap();
        editor.select_only(rectangle_id).unwrap();
        editor
            .dispatch(Command::SetName {
                target: rectangle_id,
                name: "Edited".into(),
            })
            .unwrap();
        let before = editor.document().snapshot();
        let history_before = editor.history_state();
        let selection_before = editor.selection().clone();

        let invalid = r#"{"format":"visual-authoring-document","version":1,"document":{"root_id":"not-a-uuid","nodes":[]}}"#;
        assert!(from_json(invalid).is_err());
        assert_eq!(editor.document().snapshot(), before);
        assert_eq!(editor.history_state(), history_before);
        assert_eq!(editor.selection(), &selection_before);

        let replacement = Document::new("Loaded replacement");
        let loaded = from_json(&to_json_pretty(&replacement).unwrap()).unwrap();
        editor.replace_document(loaded).unwrap();
        assert_eq!(
            editor.history_state(),
            visual_authoring_document::HistoryState::default()
        );
        assert!(editor.selection().is_empty());
        assert_eq!(
            editor
                .document()
                .node(editor.document().root_id())
                .unwrap()
                .name(),
            "Loaded replacement"
        );
    }

    #[test]
    fn group_restoration_round_trip_survives_sibling_edits_before_ungroup() {
        let mut editor = HeadlessEditorCore::blank("Phase 0E R2");
        let root = editor.document().root_id();
        let mut ids = Vec::new();
        for name in ["A", "B", "C", "D", "E", "F"] {
            let id = NodeId::new();
            editor
                .dispatch(Command::CreateNode {
                    spec: NodeSpec::rectangle(id, name, Vec2::new(10.0, 10.0)),
                    parent: root,
                    index: ids.len(),
                })
                .unwrap();
            ids.push(id);
        }
        let [a, b, c, d, e, f]: [NodeId; 6] = ids.try_into().unwrap();
        let group = NodeId::new();
        editor
            .dispatch(Command::Group {
                group: NodeSpec::group(group, "Internal restoration"),
                targets: vec![b, d],
            })
            .unwrap();
        let x = NodeId::new();
        editor
            .dispatch(Command::CreateNode {
                spec: NodeSpec::rectangle(x, "X", Vec2::new(10.0, 10.0)),
                parent: root,
                index: 0,
            })
            .unwrap();
        editor
            .dispatch(Command::DeleteSubtree { target: c })
            .unwrap();
        editor
            .dispatch(Command::Reparent {
                child: f,
                new_parent: root,
                index: 1,
            })
            .unwrap();

        let encoded = to_json_pretty(editor.document()).unwrap();
        assert!(encoded.contains("internal_group_restoration"));
        assert!(!encoded.contains("__phase0e_r1_group_positions"));
        let loaded = from_json(&encoded).unwrap();
        let mut restored = HeadlessEditorCore::new(loaded).unwrap();
        restored
            .dispatch(Command::Ungroup { target: group })
            .unwrap();
        assert_eq!(
            restored.document().node(root).unwrap().children(),
            &[x, f, a, b, d, e]
        );
    }

    #[test]
    fn phase1a_appearance_round_trips_and_version_one_migrates_defaults() {
        let (document, _frame_id, rectangle_id, _ellipse_id) = sample_document();
        let mut editor = HeadlessEditorCore::new(document).unwrap();
        let appearance = Appearance {
            fill: ColorRgba::new(0.91, 0.22, 0.13, 0.8),
            opacity: 0.65,
            corner_radii: CornerRadii {
                top_left: 4.0,
                top_right: 8.0,
                bottom_right: 12.0,
                bottom_left: 16.0,
            },
            stroke: Stroke {
                color: ColorRgba::new(0.1, 0.2, 0.3, 0.75),
                width: 6.0,
                ..Stroke::default()
            },
        };
        editor
            .dispatch(Command::SetAppearance {
                target: rectangle_id,
                appearance,
            })
            .unwrap();

        let json = to_json_pretty(editor.document()).unwrap();
        let restored = from_json(&json).unwrap();
        assert_eq!(
            restored.node(rectangle_id).unwrap().appearance(),
            appearance
        );

        let mut legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
        legacy["version"] = 1_u64.into();
        for node in legacy["document"]["nodes"].as_array_mut().unwrap() {
            let stored = node["appearance"].as_object_mut().unwrap();
            stored.remove("fill");
            stored.remove("corner_radii");
            stored.remove("stroke");
        }
        let migrated = from_json(&legacy.to_string()).unwrap();
        let migrated_appearance = migrated.node(rectangle_id).unwrap().appearance();
        assert_eq!(migrated_appearance.opacity, appearance.opacity);
        assert_eq!(migrated_appearance.fill, ColorRgba::default());
        assert_eq!(migrated_appearance.corner_radii, CornerRadii::default());
        assert_eq!(migrated_appearance.stroke, Stroke::default());
    }

    #[test]
    fn phase2b_extended_stroke_round_trips_and_version_three_migrates_defaults() {
        let (document, _frame_id, rectangle_id, _ellipse_id) = sample_document();
        let mut editor = HeadlessEditorCore::new(document).unwrap();
        let stroke = Stroke {
            color: ColorRgba::new(0.2, 0.4, 0.6, 0.8),
            width: 7.5,
            cap: StrokeCap::Square,
            join: StrokeJoin::Bevel,
            miter_limit: 9.0,
            dash_pattern: DashPattern::new(&[8.0, 3.0, 0.0, 2.0]).unwrap(),
        };
        let mut appearance = editor.document().node(rectangle_id).unwrap().appearance();
        appearance.stroke = stroke;
        editor
            .dispatch(Command::SetAppearance {
                target: rectangle_id,
                appearance,
            })
            .unwrap();

        let json = to_json_pretty(editor.document()).unwrap();
        let restored = from_json(&json).unwrap();
        assert_eq!(
            restored.node(rectangle_id).unwrap().appearance().stroke,
            stroke
        );

        let mut legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
        legacy["version"] = 3_u64.into();
        for node in legacy["document"]["nodes"].as_array_mut().unwrap() {
            let stored = node["appearance"]["stroke"].as_object_mut().unwrap();
            stored.remove("cap");
            stored.remove("join");
            stored.remove("miter_limit");
            stored.remove("dash_pattern");
        }
        let migrated = from_json(&legacy.to_string()).unwrap();
        let migrated_stroke = migrated.node(rectangle_id).unwrap().appearance().stroke;
        assert_eq!(migrated_stroke.cap, StrokeCap::Butt);
        assert_eq!(migrated_stroke.join, StrokeJoin::Miter);
        assert_eq!(migrated_stroke.miter_limit, 4.0);
        assert!(migrated_stroke.dash_pattern.is_empty());

        let saved: serde_json::Value =
            serde_json::from_str(&to_json_pretty(&migrated).unwrap()).unwrap();
        assert_eq!(saved["version"], CURRENT_VERSION);
    }

    #[test]
    fn malformed_dash_patterns_are_rejected_without_fallback() {
        let (document, ..) = sample_document();
        let mut value: serde_json::Value =
            serde_json::from_str(&to_json_pretty(&document).unwrap()).unwrap();
        value["document"]["nodes"][0]["appearance"]["stroke"]["dash_pattern"] =
            serde_json::json!([0.0, 0.0]);
        assert!(matches!(
            from_json(&value.to_string()),
            Err(SerializationError::InvalidDashPattern(
                DashPatternError::AllZero
            ))
        ));

        value["document"]["nodes"][0]["appearance"]["stroke"]["dash_pattern"] =
            serde_json::Value::Array(
                (0..=visual_authoring_document::MAX_DASH_ENTRIES)
                    .map(|_| serde_json::json!(1.0))
                    .collect(),
            );
        assert!(matches!(
            from_json(&value.to_string()),
            Err(SerializationError::InvalidDashPattern(
                DashPatternError::TooMany { .. }
            ))
        ));
    }
}

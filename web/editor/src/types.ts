export type NodeKind = "document" | "frame" | "group" | "rectangle" | "ellipse";

export interface ProjectionNode {
  id: string;
  parent_id: string | null;
  children: string[];
  name: string;
  kind: NodeKind;
  visible: boolean;
  locked: boolean;
  opacity: number;
  appearance?: {
    fill: [number, number, number, number];
    corner_radii: [number, number, number, number];
    stroke: { color: [number, number, number, number]; width: number };
  };
  local_transform: [number, number, number, number, number, number];
  world_transform: [number, number, number, number, number, number] | null;
  geometry: { width: number; height: number } | null;
  world_bounds: { min: [number, number]; max: [number, number] } | null;
}

export type StructuralProjectionOperation =
  | { kind: "attach"; node_id: string; parent_id: string; index: number }
  | { kind: "detach"; node_id: string; parent_id: string; index: number }
  | { kind: "move"; node_id: string; before_parent_id: string | null; before_index: number | null; parent_id: string | null; index: number | null }
  | {
      kind: "group" | "ungroup";
      group_id: string;
      parent_id: string;
      index: number;
      children: string[];
      child_local_transforms?: [number, number, number, number, number, number][];
      positions: number[];
    };

export interface ProjectionDelta {
  schema_version: number;
  full: boolean;
  upserts: ProjectionNode[];
  removed: string[];
  structural_ops: StructuralProjectionOperation[];
}

export interface EngineResponse {
  type: "engine_response";
  protocol_version: number;
  render_binary_schema_version: number;
  engine_sequence: number;
  request_id: string;
  ok: boolean;
  result: Record<string, unknown> | null;
  error: { code: string; message: string } | null;
  runtime_owner: "dedicated-worker";
  fixture: string;
  revisions: { document: number; scene: number; render: number };
  projection: ProjectionDelta;
  selection: { ordered: string[]; primary: string | null };
  history: { undo_depth: number; redo_depth: number; transaction_active: boolean };
  camera: { center: [number, number]; zoom: number; viewport: [number, number]; dpr: number };
  culling: Record<string, number>;
  render_delta: {
    full: boolean;
    dirty_slots: number;
    dirty_ranges: number;
    removed_slots: number;
    upload_bytes: number;
    scene_full_rebuilds: number;
    render_full_rebuilds: number;
  };
  resources: {
    instance_stride_bytes: number;
    dirty_record_stride_bytes: number;
    instance_capacity: number;
    total_render_items: number;
    renderable_items: number;
    gpu_encodable_items: number;
    gpu_omitted_items: number;
    allocated_slots: number;
    free_slots: number;
    path_instance_stride_bytes?: number;
    path_vertex_stride_bytes?: number;
    path_instance_count?: number;
    path_vertex_count?: number;
    path_vertex_revision?: number;
    draw_batches?: DrawBatch[];
  };
  render_encoding: { gpu_omitted_items: number; diagnostics: unknown[] };
  binary: Record<string, boolean>;
  metrics: Record<string, number>;
}

// One ordered unit of GPU work. Paths and analytic primitives keep their scene order by
// alternating batches rather than by drawing all of one kind first.
export type DrawBatch =
  | { kind: "primitives"; first_visible: number; count: number }
  | { kind: "paths"; first_vertex: number; vertex_count: number };

export interface BinaryPayload {
  fullInstances?: ArrayBuffer;
  dirtyInstances?: ArrayBuffer;
  removedSlots?: ArrayBuffer;
  visibleSlots?: ArrayBuffer;
  pathInstances?: ArrayBuffer;
  pathVertices?: ArrayBuffer;
}

export interface UiCounters {
  projectionFullSnapshots: number;
  projectionDeltaNodes: number;
  layersFullSerializes: number;
  hierarchyRebuilds: number;
  hierarchyNodesVisited: number;
  hierarchyFullRebuilds: number;
  projectionStructuralOperations: number;
  layersFlattenedNodesVisited: number;
  mountedRows: number;
  pointerRawIntents: number;
  pointerRequestsSent: number;
  pointerRequestsCoalesced: number;
  sequenceEntriesExamined: number;
  sequenceEntriesCopied: number;
  sequenceEntriesMovedOrShifted: number;
  sequenceRankOrderComparisons: number;
  sequenceNodesAllocated: number;
  sequenceAllocatedBytes: number;
  sequenceTreeRebalances: number;
  sequenceMaximumDepth: number;
  sequenceFallbackOrRebuildCount: number;
  sequenceFullScans: number;
  sequenceFullCopies: number;
  sequenceDenseIndexRewrites: number;
}

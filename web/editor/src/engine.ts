// The Phase 0D renderer is the established actual-WebGPU backend. Phase 0E reuses it without
// modifying the diagnostic preview or its proof artifacts.
import { WebGpuRenderer } from "../../phase0d-preview/src/renderer.js";
import { RankedSequence, emptySequenceWork } from "./rankedSequence";
import type { SequenceFragment, SequenceWork } from "./rankedSequence";
import type { BinaryPayload, EngineResponse, ProjectionNode, SlideSummary, StructuralProjectionOperation, UiCounters } from "./types";

const PROTOCOL_VERSION = 1;

function splitFloat64(value: number): [number, number] {
  const high = Math.fround(value);
  return [high, value - high];
}

export interface ThumbnailState {
  revision: number;
  status: "pending" | "ready" | "error";
  url?: string;
  error?: string;
}

export interface ThumbnailQueueDecision {
  writePending: boolean;
  enqueue: boolean;
}

export function decideThumbnailQueueUpdate(
  current: Pick<ThumbnailState, "revision" | "status"> | undefined,
  requestedRevision: number,
  queued: boolean,
  sameRevisionIsRendering: boolean,
): ThumbnailQueueDecision {
  if (
    current?.revision === requestedRevision &&
    (current.status === "ready" || current.status === "error")
  ) {
    return { writePending: false, enqueue: false };
  }
  if (queued) return { writePending: true, enqueue: false };
  if (sameRevisionIsRendering) return { writePending: false, enqueue: false };
  return { writePending: true, enqueue: true };
}
export class EngineFailure extends Error {
  constructor(public readonly code: string, message: string) {
    super(message);
    this.name = "EngineFailure";
  }
}

export interface StoredProjectionNode extends Omit<ProjectionNode, "children"> {
  children: RankedSequence;
}

interface DetachedBlock {
  fragment: SequenceFragment;
  size: number;
  external?: boolean;
}

export class ProjectionStore {
  readonly nodes = new Map<string, StoredProjectionNode>();
  order = new RankedSequence();
  rootId: string | null = null;
  selection: string[] = [];
  primary: string | null = null;
  version = 0;
  hierarchyVersion = 0;
  private readonly subtreeSizes = new Map<string, number>();
  private readonly detachedBlocks = new Map<string, DetachedBlock>();
  counters: UiCounters = {
    projectionFullSnapshots: 0,
    projectionDeltaNodes: 0,
    layersFullSerializes: 0,
    hierarchyRebuilds: 0,
    hierarchyNodesVisited: 0,
    hierarchyFullRebuilds: 0,
    projectionStructuralOperations: 0,
    layersFlattenedNodesVisited: 0,
    mountedRows: 0,
    pointerRawIntents: 0,
    pointerRequestsSent: 0,
    pointerRequestsCoalesced: 0,
    sequenceEntriesExamined: 0,
    sequenceEntriesCopied: 0,
    sequenceEntriesMovedOrShifted: 0,
    sequenceRankOrderComparisons: 0,
    sequenceNodesAllocated: 0,
    sequenceAllocatedBytes: 0,
    sequenceTreeRebalances: 0,
    sequenceMaximumDepth: 0,
    sequenceFallbackOrRebuildCount: 0,
    sequenceFullScans: 0,
    sequenceFullCopies: 0,
    sequenceDenseIndexRewrites: 0,
  };

  apply(response: EngineResponse): void {
    const projection = response.projection;
    if (projection.schema_version !== 2) {
      throw new EngineFailure("ui_projection_version_mismatch", "Unsupported UI projection schema");
    }
    if (projection.full) {
      this.nodes.clear();
      this.order = new RankedSequence();
      this.subtreeSizes.clear();
      this.detachedBlocks.clear();
      this.counters.projectionFullSnapshots += 1;
      this.counters.layersFullSerializes += 1;
    }
    for (const node of projection.upserts) this.upsertNode(node);
    if (projection.full) {
      this.rebuildHierarchy();
    } else if (projection.structural_ops.length > 0) {
      this.applyStructuralOperations(projection.structural_ops);
    }
    for (const id of projection.removed) {
      this.order.deleteExtractedValue(id);
      this.detachedBlocks.delete(id);
      this.subtreeSizes.delete(id);
      this.nodes.delete(id);
    }
    this.counters.projectionDeltaNodes += projection.full ? 0 : projection.upserts.length;
    this.selection = [...response.selection.ordered];
    this.primary = response.selection.primary;
    this.version += 1;
  }

  viewport(start: number, count: number): string[] {
    const work = emptySequenceWork();
    const values = this.order.slice(start, start + count, work);
    this.recordSequenceWork(work);
    this.counters.layersFlattenedNodesVisited += values.length;
    return values;
  }

  private upsertNode(node: ProjectionNode): void {
    const current = this.nodes.get(node.id);
    let children = current?.children;
    if (!children) {
      const work = this.sequenceWork();
      children = RankedSequence.from(node.children, work);
      this.recordSequenceWork(work);
    }
    this.nodes.set(node.id, { ...node, children });
  }

  private sequenceWork(): SequenceWork {
    return emptySequenceWork();
  }

  private recordSequenceWork(work: SequenceWork): void {
    this.counters.sequenceEntriesExamined += work.entriesExamined;
    this.counters.sequenceEntriesCopied += work.entriesCopied;
    this.counters.sequenceEntriesMovedOrShifted += work.entriesMovedOrShifted;
    this.counters.sequenceRankOrderComparisons += work.rankOrderComparisons;
    this.counters.sequenceNodesAllocated += work.sequenceNodesAllocated;
    this.counters.sequenceAllocatedBytes += work.allocatedBytes;
    this.counters.sequenceTreeRebalances += work.treeRebalances;
    this.counters.sequenceMaximumDepth = Math.max(this.counters.sequenceMaximumDepth, work.maximumSequenceDepth);
    this.counters.sequenceFallbackOrRebuildCount += work.fallbackOrRebuildCount;
    this.counters.sequenceFullScans += work.fullSequenceScans;
    this.counters.sequenceFullCopies += work.fullSequenceCopies;
    this.counters.sequenceDenseIndexRewrites += work.denseIndexRewrites;
  }

  private addAncestorSize(parentId: string | null, delta: number, work: SequenceWork): void {
    let current = parentId;
    while (current) {
      work.entriesExamined += 1;
      this.subtreeSizes.set(current, (this.subtreeSizes.get(current) ?? 1) + delta);
      current = this.nodes.get(current)?.parent_id ?? null;
    }
  }

  private insertionRank(parentId: string, index: number, work: SequenceWork): number {
    const parent = this.requireNode(parentId);
    const nextSibling = parent.children.at(index, work);
    if (nextSibling) {
      const rank = this.order.rankOf(nextSibling, work);
      if (rank !== undefined) return rank;
    }
    const parentRank = this.order.rankOf(parentId, work);
    if (parentRank === undefined) throw new EngineFailure("ui_projection_parent_order_missing", "Projection parent " + parentId + " is not ordered");
    return parentRank + (this.subtreeSizes.get(parentId) ?? 1);
  }

  private detachBlock(parentId: string, nodeId: string, work: SequenceWork): DetachedBlock {
    const parent = this.requireNode(parentId);
    const childIndex = parent.children.remove(nodeId, work);
    if (childIndex === undefined) throw new EngineFailure("ui_projection_child_missing", "Projection child " + nodeId + " is missing");
    const size = this.subtreeSizes.get(nodeId) ?? 1;
    const fragment = this.order.extractByValue(nodeId, size, work);
    this.addAncestorSize(parentId, -size, work);
    return { fragment, size };
  }

  private insertBlock(parentId: string, index: number, nodeId: string, block: DetachedBlock, work: SequenceWork): void {
    const parent = this.requireNode(parentId);
    const insertion = this.insertionRank(parentId, index, work);
    parent.children.insert(index, nodeId, work);
    this.order.insertFragment(insertion, block.fragment, work, block.external ?? false);
    this.addAncestorSize(parentId, block.size, work);
  }

  private explicitSubtree(root: string, work: SequenceWork): DetachedBlock {
    const ids: string[] = [];
    const stack = [root];
    while (stack.length) {
      const id = stack.pop();
      if (!id) break;
      ids.push(id);
      work.entriesExamined += 1;
      const node = this.nodes.get(id);
      if (!node) continue;
      const children = node.children.slice(0, node.children.length, work);
      for (let index = children.length - 1; index >= 0; index -= 1) stack.push(children[index]);
    }
    const temporary = RankedSequence.from(ids, work);
    const fragment = temporary.extract(0, temporary.length, work);
    for (const id of ids) temporary.deleteExtractedValue(id);
    return { fragment, size: ids.length, external: true };
  }

  private applyStructuralOperations(operations: StructuralProjectionOperation[]): void {
    const work = this.sequenceWork();
    for (const operation of operations) {
      if (operation.kind === "attach") {
        const block = this.detachedBlocks.get(operation.node_id) ?? this.explicitSubtree(operation.node_id, work);
        this.detachedBlocks.delete(operation.node_id);
        this.insertBlock(operation.parent_id, operation.index, operation.node_id, block, work);
        continue;
      }
      if (operation.kind === "detach") {
        this.detachedBlocks.set(operation.node_id, this.detachBlock(operation.parent_id, operation.node_id, work));
        continue;
      }
      if (operation.kind === "move") {
        let block = this.detachedBlocks.get(operation.node_id);
        if (operation.before_parent_id) block = this.detachBlock(operation.before_parent_id, operation.node_id, work);
        if (operation.parent_id && operation.index !== null) {
          if (!block) block = this.explicitSubtree(operation.node_id, work);
          this.detachedBlocks.delete(operation.node_id);
          this.insertBlock(operation.parent_id, operation.index, operation.node_id, block, work);
        } else if (block) {
          this.detachedBlocks.set(operation.node_id, block);
        }
        continue;
      }
      const parent = this.requireNode(operation.parent_id);
      const childLocalTransforms =
        operation.child_local_transforms ??
        operation.children.map((child) => this.requireNode(child).local_transform);
      if (childLocalTransforms.length !== operation.children.length) {
        throw new EngineFailure("ui_projection_child_transform_mismatch", "Projection child transform count does not match child count");
      }
      if (operation.kind === "group") {
        const canBatchLeaves =
          operation.children.length >= 64 &&
          operation.positions.length === operation.children.length &&
          (this.subtreeSizes.get(operation.parent_id) ?? 0) === parent.children.length + 1 &&
          operation.children.every((child) => {
            work.entriesExamined += 1;
            return (this.subtreeSizes.get(child) ?? 1) === 1;
          });
        if (canBatchLeaves) {
          const parentRank = this.order.rankOf(operation.parent_id, work);
          if (parentRank === undefined) {
            throw new EngineFailure("ui_projection_parent_order_missing", "Projection parent " + operation.parent_id + " is not ordered");
          }
          const globalRanks = operation.positions.map((position) => parentRank + 1 + position);
          work.entriesCopied += globalRanks.length;
          work.allocatedBytes += globalRanks.length * 8;
          parent.children.extractRanks(operation.positions, work, true, operation.children);
          const fragments = this.order.extractRanks(globalRanks, work, false, operation.children);
          const insertion = parentRank + 1 + operation.index;
          parent.children.insert(operation.index, operation.group_id, work);
          this.order.insert(insertion, operation.group_id, work);
          const combined = RankedSequence.combineSingletonFragments(fragments, work);
          this.order.insertFragment(insertion + 1, combined, work);
          this.subtreeSizes.set(operation.group_id, operation.children.length + 1);
          this.addAncestorSize(operation.parent_id, 1, work);
          for (let index = 0; index < operation.children.length; index += 1) {
            const childNode = this.requireNode(operation.children[index]);
            childNode.parent_id = operation.group_id;
            childNode.local_transform = childLocalTransforms[index];
          }
        } else {
          const blocks: DetachedBlock[] = [];
          for (const child of operation.children) blocks.push(this.detachBlock(operation.parent_id, child, work));
          const insertion = this.insertionRank(operation.parent_id, operation.index, work);
          parent.children.insert(operation.index, operation.group_id, work);
          this.order.insert(insertion, operation.group_id, work);
          this.subtreeSizes.set(operation.group_id, 1);
          this.addAncestorSize(operation.parent_id, 1, work);
          const group = this.requireNode(operation.group_id);
          let childInsertion = insertion + 1;
          for (let index = 0; index < operation.children.length; index += 1) {
            const child = operation.children[index];
            const childNode = this.requireNode(child);
            childNode.parent_id = operation.group_id;
            childNode.local_transform = childLocalTransforms[index];
            const block = blocks[index];
            this.order.insertFragment(childInsertion, block.fragment, work);
            childInsertion += block.size;
            this.subtreeSizes.set(operation.group_id, (this.subtreeSizes.get(operation.group_id) ?? 1) + block.size);
            this.addAncestorSize(operation.parent_id, block.size, work);
          }
        }
      } else {
        const groupRank = parent.children.remove(operation.group_id, work);
        if (groupRank === undefined) throw new EngineFailure("ui_projection_group_missing", "Projection group " + operation.group_id + " is missing");
        const group = this.requireNode(operation.group_id);
        const blocks: DetachedBlock[] = [];
        for (const child of operation.children) {
          const size = this.subtreeSizes.get(child) ?? 1;
          const fragment = this.order.extractByValue(child, size, work);
          group.children.remove(child, work);
          blocks.push({ fragment, size });
          this.subtreeSizes.set(operation.group_id, (this.subtreeSizes.get(operation.group_id) ?? 1) - size);
          this.addAncestorSize(operation.parent_id, -size, work);
        }
        const removedRank = this.order.remove(operation.group_id, work);
        if (removedRank === undefined) throw new EngineFailure("ui_projection_group_order_missing", "Projection group " + operation.group_id + " is not ordered");
        this.subtreeSizes.delete(operation.group_id);
        this.addAncestorSize(operation.parent_id, -1, work);
        for (let index = 0; index < operation.children.length; index += 1) {
          const child = operation.children[index];
          const childNode = this.requireNode(child);
          childNode.parent_id = operation.parent_id;
          childNode.local_transform = childLocalTransforms[index];
          const restoredIndex = operation.positions[index] ?? operation.index + index;
          this.insertBlock(operation.parent_id, restoredIndex, child, blocks[index], work);
        }
      }
      this.counters.hierarchyNodesVisited += operation.children.length + 2;
    }
    this.recordSequenceWork(work);
    this.counters.projectionStructuralOperations += operations.length;
    this.hierarchyVersion += 1;
  }

  private requireNode(id: string): StoredProjectionNode {
    const node = this.nodes.get(id);
    if (!node) throw new EngineFailure("ui_projection_node_missing", "Projection node " + id + " is missing");
    return node;
  }

  private rebuildHierarchy(): void {
    const work = this.sequenceWork();
    this.rootId = [...this.nodes.values()].find((node) => node.kind === "document")?.id ?? null;
    const order: string[] = [];
    this.subtreeSizes.clear();
    if (this.rootId) {
      const stack: Array<{ id: string; visited: boolean }> = [{ id: this.rootId, visited: false }];
      while (stack.length) {
        const entry = stack.pop();
        if (!entry) break;
        const node = this.nodes.get(entry.id);
        if (entry.visited) {
          let size = 1;
          if (node) for (const child of node.children.slice(0, node.children.length, work)) size += this.subtreeSizes.get(child) ?? 1;
          this.subtreeSizes.set(entry.id, size);
          continue;
        }
        order.push(entry.id);
        stack.push({ id: entry.id, visited: true });
        if (!node) continue;
        const children = node.children.slice(0, node.children.length, work);
        for (let index = children.length - 1; index >= 0; index -= 1) stack.push({ id: children[index], visited: false });
      }
    }
    this.order = RankedSequence.from(order, work);
    this.recordSequenceWork(work);
    this.counters.hierarchyRebuilds += 1;
    this.counters.hierarchyFullRebuilds += 1;
    this.counters.hierarchyNodesVisited += order.length;
    this.hierarchyVersion += 1;
  }
}

type Listener = (response: EngineResponse) => void;
type Pending = { resolve: (value: EngineResponse) => void; reject: (reason: unknown) => void };

export class EngineClient {
  readonly projection = new ProjectionStore();
  private worker: Worker | null = null;
  private renderer: InstanceType<typeof WebGpuRenderer> | null = null;
  private canvas: HTMLCanvasElement | null = null;
  private generation = 0;
  private requestSequence = 0;
  private pending = new Map<string, Pending>();
  private listeners = new Set<Listener>();
  private activityListeners = new Set<() => void>();
  private workerReady: Promise<void> | null = null;
  private readyResolve: (() => void) | null = null;
  private readyReject: ((reason: unknown) => void) | null = null;
  private messageChain = Promise.resolve();
  private lastFrameSequence = 0;
  private heartbeat = 0;
  capabilities: Record<string, unknown> = {};
  gpuMetrics: Record<string, number> = {};
  readonly thumbnails = new Map<string, ThumbnailState>();
  thumbnailCaptures = 0;
  thumbnailInvalidations = 0;
  private thumbnailQueue: string[] = [];
  private readonly thumbnailQueued = new Set<string>();
  private readonly visibleThumbnailIds = new Set<string>();
  private thumbnailRunning: string | null = null;
  private thumbnailScheduled = false;
  private latestResponse: EngineResponse | null = null;
  private activeVisibleSlots = new Uint32Array();

  async initialize(canvas: HTMLCanvasElement): Promise<EngineResponse> {
    this.canvas = canvas;
    this.renderer = await WebGpuRenderer.create(canvas);
    this.capabilities = this.renderer.capabilities();
    await this.restartWorker(false);
    await this.send("initialize");
    const rect = canvas.getBoundingClientRect();
    await this.send("camera", {
      camera: { kind: "resize", width: rect.width, height: rect.height, dpr: devicePixelRatio },
    });
    await this.send("load_fixture", { fixture: "editor" });
    return this.send("camera", { camera: { kind: "fit" } });
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  subscribeActivity(listener: () => void): () => void {
    this.activityListeners.add(listener);
    return () => this.activityListeners.delete(listener);
  }

  async restartWorker(reinitialize = true): Promise<EngineResponse | null> {
    this.generation += 1;
    const generation = this.generation;
    this.lastFrameSequence = 0;
    this.heartbeat = 0;
    this.messageChain = Promise.resolve();
    this.worker?.terminate();
    for (const pending of this.pending.values()) {
      pending.reject(new EngineFailure("worker_restarted", "Worker restarted before completion"));
    }
    this.pending.clear();
    this.workerReady = new Promise<void>((resolve, reject) => {
      this.readyResolve = resolve;
      this.readyReject = reject;
    });
    const worker = new Worker("/worker.js", { type: "module", name: "phase0e-engine-host" });
    this.worker = worker;
    worker.addEventListener("message", (event: MessageEvent) => {
      this.messageChain = this.messageChain.then(() => this.processMessage(event.data, generation));
    });
    worker.addEventListener("error", (event) => {
      this.readyReject?.(new EngineFailure("worker_error", event.message));
    });
    await this.workerReady;
    if (!reinitialize) return null;
    await this.send("initialize");
    if (!this.canvas) return null;
    const rect = this.canvas.getBoundingClientRect();
    await this.send("camera", {
      camera: { kind: "resize", width: rect.width, height: rect.height, dpr: devicePixelRatio },
    });
    await this.send("load_fixture", { fixture: "editor" });
    return this.send("camera", { camera: { kind: "fit" } });
  }

  send(type: string, payload: Record<string, unknown> = {}): Promise<EngineResponse> {
    if (!this.worker || !this.workerReady) {
      return Promise.reject(new EngineFailure("worker_not_ready", "Worker is not ready"));
    }
    this.requestSequence += 1;
    const requestId = `${type}-${Date.now()}-${this.requestSequence}`;
    const request = { protocol_version: PROTOCOL_VERSION, request_id: requestId, type, ...payload };
    return new Promise<EngineResponse>((resolve, reject) => {
      this.pending.set(requestId, { resolve, reject });
      this.worker?.postMessage(request);
    });
  }

  private async processMessage(message: unknown, generation: number): Promise<void> {
    if (generation !== this.generation || !message || typeof message !== "object") return;
    const typed = message as {
      type?: string;
      response?: EngineResponse;
      payload?: BinaryPayload;
      error?: { code?: string; message?: string };
      wasm_initialized?: boolean;
    };
    if (typed.type === "worker_ready") {
      this.readyResolve?.();
      return;
    }
    if (typed.type === "worker_boot_error" || typed.type === "worker_request_error") {
      const error = new EngineFailure(
        typed.error?.code ?? "worker_failed",
        typed.error?.message ?? "Worker failed",
      );
      this.readyReject?.(error);
      return;
    }
    if (typed.type !== "engine" || !typed.response) return;
    const response = typed.response;
    if (response.protocol_version !== PROTOCOL_VERSION || response.type !== "engine_response") {
      throw new EngineFailure("protocol_version_mismatch", "Invalid EngineHost response");
    }
    if (response.request_id.startsWith("heartbeat-")) {
      this.heartbeat += 1;
      for (const listener of this.activityListeners) listener();
      return;
    }
    const pending = this.pending.get(response.request_id);
    this.pending.delete(response.request_id);
    if (!response.ok) {
      const failure = new EngineFailure(
        response.error?.code ?? "engine_error",
        response.error?.message ?? "Engine request failed",
      );
      pending?.reject(failure);
      return;
    }
    if (response.result?.kind === "thumbnail_slots") {
      pending?.resolve(response);
      return;
    }
    if (response.engine_sequence <= this.lastFrameSequence) {
      pending?.reject(new EngineFailure("stale_engine_frame", "Stale engine frame rejected"));
      return;
    }
    if (!this.renderer) throw new EngineFailure("renderer_not_ready", "WebGPU renderer is not ready");
    if (typed.payload?.visibleSlots) {
      this.activeVisibleSlots = new Uint32Array(typed.payload.visibleSlots).slice();
    }
    this.gpuMetrics = await this.renderer.applyEngineFrame(response, typed.payload ?? {});
    this.lastFrameSequence = response.engine_sequence;
    this.projection.apply(response);
    this.updateThumbnailQueue(response);
    for (const listener of this.listeners) listener(response);
    pending?.resolve(response);
  }

  setThumbnailVisibility(slideId: string, visible: boolean): void {
    if (visible) this.visibleThumbnailIds.add(slideId);
    else this.visibleThumbnailIds.delete(slideId);
    this.sortThumbnailQueue();
    this.scheduleThumbnailDrain();
  }

  private updateThumbnailQueue(response: EngineResponse): void {
    this.latestResponse = response;
    const validSlides = new Set(response.editor_session.slides.map((slide) => slide.id));
    for (const [slideId, cached] of this.thumbnails) {
      if (!validSlides.has(slideId)) {
        if (cached.url) URL.revokeObjectURL(cached.url);
        this.thumbnails.delete(slideId);
        this.thumbnailQueued.delete(slideId);
        this.visibleThumbnailIds.delete(slideId);
      }
    }
    this.thumbnailQueue = this.thumbnailQueue.filter((slideId) => validSlides.has(slideId));
    for (const slide of response.editor_session.slides) {
      const cached = this.thumbnails.get(slide.id);
      if (cached && cached.revision !== slide.thumbnail_revision) {
        if (cached.url) URL.revokeObjectURL(cached.url);
        this.thumbnails.delete(slide.id);
        this.thumbnailInvalidations += 1;
      }
      const current = this.thumbnails.get(slide.id);
      const sameRevisionIsRendering =
        this.thumbnailRunning === slide.id &&
        current?.revision === slide.thumbnail_revision &&
        current.status === "pending";
      const decision = decideThumbnailQueueUpdate(
        current,
        slide.thumbnail_revision,
        this.thumbnailQueued.has(slide.id),
        sameRevisionIsRendering,
      );
      if (!decision.writePending) continue;
      this.thumbnails.set(slide.id, { revision: slide.thumbnail_revision, status: "pending" });
      if (!decision.enqueue) continue;
      this.thumbnailQueue.push(slide.id);
      this.thumbnailQueued.add(slide.id);
    }
    this.sortThumbnailQueue();
    this.scheduleThumbnailDrain();
  }

  private sortThumbnailQueue(): void {
    const response = this.latestResponse;
    if (!response) return;
    this.thumbnailQueue = orderThumbnailQueue(this.thumbnailQueue, response.editor_session.slides, this.visibleThumbnailIds);
  }

  private scheduleThumbnailDrain(): void {
    if (this.thumbnailRunning || this.thumbnailScheduled || this.thumbnailQueue.length === 0) return;
    this.thumbnailScheduled = true;
    requestAnimationFrame(() => {
      this.thumbnailScheduled = false;
      void this.drainOneThumbnail();
    });
  }

  private async drainOneThumbnail(): Promise<void> {
    if (this.thumbnailRunning) return;
    const slideId = this.thumbnailQueue.shift();
    if (!slideId) return;
    this.thumbnailQueued.delete(slideId);
    const response = this.latestResponse;
    const slide = response?.editor_session.slides.find((candidate) => candidate.id === slideId);
    const cached = this.thumbnails.get(slideId);
    if (!response || !slide || cached?.revision !== slide.thumbnail_revision) {
      this.scheduleThumbnailDrain();
      return;
    }
    this.thumbnailRunning = slideId;
    try {
      const slotsResponse = await this.send("thumbnail_slots", { slide_id: slideId });
      const rawSlots = slotsResponse.result?.slots;
      if (!Array.isArray(rawSlots) || rawSlots.some((slot) => !Number.isInteger(slot) || slot < 0)) {
        throw new EngineFailure("thumbnail_slots_invalid", `Slide ${slideId} returned invalid RenderModel slots`);
      }
      const slots = Uint32Array.from(rawSlots as number[]);
      const serializedCapture = this.messageChain.then(() =>
        this.captureSlideThumbnail(response, slide, slots)
      );
      this.messageChain = serializedCapture.then(() => undefined, () => undefined);
      const url = await serializedCapture;
      const latest = this.latestResponse?.editor_session.slides.find((candidate) => candidate.id === slideId);
      if (!latest || latest.thumbnail_revision !== slide.thumbnail_revision) {
        URL.revokeObjectURL(url);
        return;
      }
      this.thumbnails.set(slideId, {
        revision: slide.thumbnail_revision,
        status: "ready",
        url,
      });
      this.thumbnailCaptures += 1;
    } catch (error) {
      const latest = this.latestResponse?.editor_session.slides.find((candidate) => candidate.id === slideId);
      if (latest?.thumbnail_revision === slide.thumbnail_revision) {
        this.thumbnails.set(slideId, {
          revision: slide.thumbnail_revision,
          status: "error",
          error: error instanceof Error ? error.message : String(error),
        });
      }
    } finally {
      this.thumbnailRunning = null;
      this.scheduleThumbnailDrain();
    }
  }

  private viewUniform(
    center: [number, number],
    viewport: [number, number],
    zoom: number,
  ): Float32Array {
    const [centerXHigh, centerXLow] = splitFloat64(center[0]);
    const [centerYHigh, centerYLow] = splitFloat64(center[1]);
    return new Float32Array([
      centerXHigh,
      centerYHigh,
      centerXLow,
      centerYLow,
      viewport[0],
      viewport[1],
      zoom,
      0,
    ]);
  }

  private async captureSlideThumbnail(
    response: EngineResponse,
    slide: SlideSummary,
    slots: Uint32Array,
  ): Promise<string> {
    const node = this.projection.nodes.get(slide.id);
    const bounds = node?.world_bounds;
    if (!bounds) {
      throw new EngineFailure("thumbnail_bounds_unavailable", `Slide ${slide.id} has no finite bounds`);
    }
    if (typeof OffscreenCanvas === "undefined") {
      throw new EngineFailure("thumbnail_offscreen_unavailable", "WebGPU OffscreenCanvas is unavailable");
    }
    const renderer = this.renderer as unknown as Record<string, any>;
    const device = renderer.device;
    const queue = renderer.queue;
    if (!device || !queue || !renderer.pipeline || !renderer.bindGroup || !renderer.viewBuffer) {
      throw new EngineFailure("thumbnail_renderer_unavailable", "WebGPU thumbnail renderer state is unavailable");
    }
    const width = 320;
    const height = 180;
    const canvas = new OffscreenCanvas(width, height);
    const context = canvas.getContext("webgpu") as unknown as Record<string, any> | null;
    if (!context) {
      throw new EngineFailure("thumbnail_surface_unavailable", "WebGPU thumbnail surface creation failed");
    }
    context.configure({
      device,
      format: renderer.surfaceBaseFormat,
      viewFormats: [renderer.pipelineViewFormat],
      alphaMode: "premultiplied",
    });
    const contentWidth = Math.max(1e-9, bounds.max[0] - bounds.min[0]);
    const contentHeight = Math.max(1e-9, bounds.max[1] - bounds.min[1]);
    const center: [number, number] = [
      (bounds.min[0] + bounds.max[0]) * 0.5,
      (bounds.min[1] + bounds.max[1]) * 0.5,
    ];
    const zoom = Math.min((width - 16) / contentWidth, (height - 16) / contentHeight);
    const thumbnailView = this.viewUniform(center, [width, height], zoom);
    const editorView = this.viewUniform(response.camera.center, response.camera.viewport, response.camera.zoom);
    const activeSlots = this.activeVisibleSlots.slice();
    renderer.ensureVisibleBuffer(Math.max(4, slots.byteLength, activeSlots.byteLength));
    if (slots.byteLength > 0) {
      queue.writeBuffer(renderer.visibleBuffer, 0, slots);
    }
    let renderFailure: unknown = null;
    device.pushErrorScope("validation");
    try {
      queue.writeBuffer(renderer.viewBuffer, 0, thumbnailView);
      const textureView = context
        .getCurrentTexture()
        .createView({ format: renderer.pipelineViewFormat });
      const encoder = device.createCommandEncoder({ label: "Phase 1B thumbnail encoder" });
      const pass = encoder.beginRenderPass({
        label: "Phase 1B WebGPU thumbnail pass",
        colorAttachments: [{
          view: textureView,
          clearValue: { r: 0.035, g: 0.055, b: 0.08, a: 1 },
          loadOp: "clear",
          storeOp: "store",
        }],
      });
      if (slots.length > 0) {
        pass.setPipeline(renderer.pipeline);
        pass.setBindGroup(0, renderer.bindGroup);
        pass.draw(6, slots.length);
      }
      pass.end();
      queue.submit([encoder.finish()]);
      await queue.onSubmittedWorkDone();
    } catch (error) {
      renderFailure = error;
    } finally {
      queue.writeBuffer(renderer.viewBuffer, 0, editorView);
      if (activeSlots.byteLength > 0) {
        queue.writeBuffer(renderer.visibleBuffer, 0, activeSlots);
      }
    }
    const validation = await device.popErrorScope();
    if (validation) {
      context.unconfigure?.();
      throw new EngineFailure("thumbnail_gpu_validation_error", validation.message);
    }
    if (renderFailure) {
      context.unconfigure?.();
      throw renderFailure;
    }
    let blob: Blob;
    try {
      blob = await canvas.convertToBlob({ type: "image/png" });
    } finally {
      context.unconfigure?.();
    }
    return URL.createObjectURL(blob);
  }

  async readPixel(x: number, y: number) {
    if (!this.renderer) throw new EngineFailure("renderer_not_ready", "WebGPU renderer is not ready");
    return this.renderer.readPixel(x, y);
  }

  proof(response: EngineResponse | null, state: Record<string, unknown>): Record<string, unknown> {
    return {
      phase: "0E",
      ready: Boolean(response),
      actual_webgpu: Boolean(this.capabilities.available),
      adapter: this.capabilities.adapter ?? null,
      backend: this.capabilities.backend ?? null,
      surface_base_format: this.capabilities.surface_base_format ?? null,
      pipeline_view_format: this.capabilities.pipeline_view_format ?? null,
      readback_view_format: this.capabilities.readback_view_format ?? null,
      worker_runtime_owner: "dedicated-worker",
      wasm_initialized: Boolean(response),
      heartbeat: this.heartbeat,
      engine_sequence: this.lastFrameSequence,
      gpu_frame_sequence: this.gpuMetrics.frame_sequence ?? 0,
      overlay_sequence: this.lastFrameSequence,
      revisions: response?.revisions ?? null,
      fixture: response?.fixture ?? null,
      metrics: response?.metrics ?? null,
      gpu: this.gpuMetrics,
      ui: { ...this.projection.counters },
      selection: [...this.projection.selection],
      thumbnails: {
        captures: this.thumbnailCaptures,
        invalidations: this.thumbnailInvalidations,
        cached: [...this.thumbnails.values()].filter((entry) => entry.status === "ready").length,
        pending: [...this.thumbnails.values()].filter((entry) => entry.status === "pending").length,
        errors: [...this.thumbnails.values()].filter((entry) => entry.status === "error").length,
        queue_depth: this.thumbnailQueue.length,
        queue_running: this.thumbnailRunning,
        priority_policy: "active-visible-remaining",
        render_source: "rust-render-model-slots",
        max_renders_per_engine_frame: 1,
        canvas2d_fallback_count: 0,
      },
      response_gpu_overlay_sequence_match:
        this.lastFrameSequence > 0 &&
        this.gpuMetrics.frame_sequence === this.lastFrameSequence,
      ...state,
    };
  }
}

export function orderThumbnailQueue(
  slideIds: string[],
  slides: SlideSummary[],
  visibleSlideIds: ReadonlySet<string>,
): string[] {
  const byId = new Map(slides.map((slide) => [slide.id, slide]));
  const priority = (slideId: string) => {
    const slide = byId.get(slideId);
    if (slide?.active) return 0;
    if (visibleSlideIds.has(slideId)) return 1;
    return 2;
  };
  return [...slideIds].sort((left, right) => {
    const priorityDelta = priority(left) - priority(right);
    return priorityDelta !== 0
      ? priorityDelta
      : (byId.get(left)?.index ?? 0) - (byId.get(right)?.index ?? 0);
  });
}

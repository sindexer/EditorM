import {
  Box,
  ChevronDown,
  ChevronRight,
  Circle,
  Code2,
  Eye,
  EyeOff,
  Focus,
  Group,
  Hand,
  Lock,
  MousePointer2,
  PanelLeftClose,
  PanelRightClose,
  Redo2,
  RotateCw,
  Scan,
  Square,
  Ungroup,
  Undo2,
  Unlock,
  X,
  type LucideIcon,
} from "lucide-react";
import {
  type CSSProperties,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { EngineClient, EngineFailure } from "./engine";
import type { StoredProjectionNode } from "./engine";
import { transformAffinePoint } from "./affine";
import {
  axisAlignedBox,
  handlePoint,
  orientedBox,
  RESIZE_HANDLES,
  resizeTransform,
  rotationAround,
  snappedRotationDelta,
  type ResizeHandle,
  type TransformBox,
} from "./transformGeometry";
import {
  arrangeAvailability,
  guideSegment,
  isMarqueeDrag,
  marqueeRect,
  pressStartsMarquee,
  selectionIntent,
  unionWorldBounds,
  SNAP_THRESHOLD_PX,
  type SnapGuideProjection,
  type ViewportRect,
} from "./selectionGeometry";
import type { EngineResponse, ProjectionNode } from "./types";

declare global {
  interface Window {
    __PHASE0E_PROOF__?: Record<string, unknown>;
    __phase0eState?: Record<string, unknown>;
    __phase0eReadPixel?: (x: number, y: number) => Promise<unknown>;
    __phase0eSend?: (type: string, payload?: Record<string, unknown>) => Promise<EngineResponse>;
    __phase0eR2Counters?: () => Record<string, number>;
    __phase0eR2Order?: (start: number, count: number) => string[];
  }
}

type Tool = "select" | "hand" | "frame" | "rectangle" | "ellipse";
type FsmState =
  | "Idle"
  | "Hovering"
  | "Selecting"
  | "Moving"
  | "Resizing"
  | "Rotating"
  | "Panning"
  | "CreatingRectangle"
  | "CreatingEllipse"
  | "CreatingFrame"
  | "MarqueeSelecting"
  | "NestedEditing";

type Interaction = {
  pointerId: number;
  kind: "move" | "resize" | "rotate" | "pan" | "create" | "marquee";
  start: [number, number];
  last: [number, number];
  nodeId?: string;
  nodeKind?: "frame" | "rectangle" | "ellipse";
  matrix?: [number, number, number, number, number, number];
  geometry?: { width: number; height: number };
  created?: boolean;
  panPending?: [number, number];
  additive?: boolean;
  suspendSnap?: boolean;
  marqueeRoot?: string;
  resizeHandle?: ResizeHandle;
  transformBox?: TransformBox;
  rotationStart?: number;
};

const tools: Array<{ id: Tool; label: string; shortcut: string; icon: LucideIcon }> = [
  { id: "select", label: "Select", shortcut: "V", icon: MousePointer2 },
  { id: "hand", label: "Hand", shortcut: "H", icon: Hand },
  { id: "frame", label: "Frame", shortcut: "F", icon: Scan },
  { id: "rectangle", label: "Rectangle", shortcut: "R", icon: Square },
  { id: "ellipse", label: "Ellipse", shortcut: "O", icon: Circle },
];

const arrangeOperations: Array<{ id: string; label: string; kind: "align" | "distribute" }> = [
  { id: "align_left", label: "Align left", kind: "align" },
  { id: "align_horizontal_center", label: "Align horizontal centers", kind: "align" },
  { id: "align_right", label: "Align right", kind: "align" },
  { id: "align_top", label: "Align top", kind: "align" },
  { id: "align_vertical_center", label: "Align vertical centers", kind: "align" },
  { id: "align_bottom", label: "Align bottom", kind: "align" },
  { id: "distribute_horizontal", label: "Distribute horizontally", kind: "distribute" },
  { id: "distribute_vertical", label: "Distribute vertically", kind: "distribute" },
];

const arrangeGlyphs: Record<string, string> = {
  align_left: "⇤",
  align_horizontal_center: "↔",
  align_right: "⇥",
  align_top: "⤒",
  align_vertical_center: "↕",
  align_bottom: "⤓",
  distribute_horizontal: "⇹",
  distribute_vertical: "⇳",
};

/** Reads the snap guides an engine response reported for the current drag frame. */
function readGuides(result: EngineResponse["result"]): SnapGuideProjection[] {
  const guides = (result as { guides?: unknown } | null)?.guides;
  if (!Array.isArray(guides)) return [];
  return guides.filter((guide): guide is SnapGuideProjection => {
    const candidate = guide as Partial<SnapGuideProjection>;
    return (
      (candidate.axis === "vertical" || candidate.axis === "horizontal") &&
      Number.isFinite(candidate.position) &&
      Number.isFinite(candidate.start) &&
      Number.isFinite(candidate.end)
    );
  });
}

type AppearanceProjection = NonNullable<ProjectionNode["appearance"]>;

const DEFAULT_APPEARANCE: AppearanceProjection = {
  fill: [0.2, 0.58, 0.96, 1],
  corner_radii: [0, 0, 0, 0],
  stroke: { color: [0.08, 0.11, 0.16, 1], width: 0 },
};

function shapeName(kind: "frame" | "rectangle" | "ellipse") {
  if (kind === "frame") return "Frame";
  if (kind === "ellipse") return "Ellipse";
  return "Rectangle";
}

function colorToHex(color: [number, number, number, number]) {
  return "#" + color.slice(0, 3).map((component) =>
    Math.round(Math.max(0, Math.min(1, component)) * 255).toString(16).padStart(2, "0")
  ).join("");
}

function hexToColor(value: string, alpha: number): [number, number, number, number] {
  const normalized = value.startsWith("#") ? value.slice(1) : value;
  return [
    Number.parseInt(normalized.slice(0, 2), 16) / 255,
    Number.parseInt(normalized.slice(2, 4), 16) / 255,
    Number.parseInt(normalized.slice(4, 6), 16) / 255,
    alpha,
  ];
}
function IconButton({
  icon: Icon,
  label,
  active = false,
  disabled = false,
  onClick,
  testId,
}: {
  icon: LucideIcon;
  label: string;
  active?: boolean;
  disabled?: boolean;
  onClick?: () => void;
  testId?: string;
}) {
  return (
    <button
      type="button"
      className={`icon-button${active ? " is-active" : ""}`}
      aria-label={label}
      aria-pressed={active || undefined}
      title={label}
      disabled={disabled}
      onClick={onClick}
      data-testid={testId}
    >
      <Icon aria-hidden="true" size={18} strokeWidth={1.8} />
    </button>
  );
}

function PanelHeader({ title, action }: { title: string; action?: React.ReactNode }) {
  return (
    <div className="panel-header">
      <h2>{title}</h2>
      {action}
    </div>
  );
}

function proofProjectionNode(node: StoredProjectionNode | undefined) {
  if (!node) return null;
  const { children, ...semantic } = node;
  return {
    ...semantic,
    children: children.slice(0, Math.min(children.length, 64)),
    child_count: children.length,
  };
}

function kindIcon(kind: ProjectionNode["kind"]): LucideIcon {
  if (kind === "ellipse") return Circle;
  if (kind === "group" || kind === "frame" || kind === "document") return Group;
  return Square;
}

function LayersPanel({
  engine,
  response,
  version,
  onError,
  editRoot,
  activeRoot,
  setEditRoot,
}: {
  engine: EngineClient;
  response: EngineResponse | null;
  version: number;
  onError: (error: unknown) => void;
  editRoot: string | null;
  activeRoot: string | null;
  setEditRoot: (id: string | null) => void;
}) {
  const rowHeight = 28;
  const viewportHeight = 420;
  const [scrollTop, setScrollTop] = useState(0);
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set());
  const order = engine.projection.order;
  const largeFlatProjection = order.length > 5000 && !editRoot;
  const visibleOrder = useMemo(() => {
    const result: Array<{ id: string; depth: number }> = [];
    if (largeFlatProjection) return result;
    const nodes = engine.projection.nodes;
    const root = editRoot ?? activeRoot;
    if (!root) return result;
    const container = nodes.get(root);
    const stack: Array<{ id: string; depth: number }> = [];
    for (let index = (container?.children.length ?? 0) - 1; index >= 0; index -= 1) {
      const child = container?.children.at(index);
      if (child) stack.push({ id: child, depth: 0 });
    }
    while (stack.length) {
      const current = stack.pop();
      if (!current) break;
      result.push(current);
      const node = nodes.get(current.id);
      if (!node) continue;
      const shouldExpand = expanded.has(current.id);
      if (shouldExpand) {
        for (let index = node.children.length - 1; index >= 0; index -= 1) {
          const child = node.children.at(index);
          if (child) stack.push({ id: child, depth: current.depth + 1 });
        }
      }
    }
    engine.projection.counters.layersFlattenedNodesVisited += result.length;
    return result;
  }, [engine, engine.projection.hierarchyVersion, editRoot, activeRoot, expanded, largeFlatProjection]);
  const totalRows = largeFlatProjection ? order.length : visibleOrder.length;
  const start = Math.max(0, Math.floor(scrollTop / rowHeight) - 5);
  const count = Math.ceil(viewportHeight / rowHeight) + 10;
  const mounted = largeFlatProjection
    ? order.slice(start, start + count).map((id) => {
        let depth = 0;
        let current = engine.projection.nodes.get(id)?.parent_id ?? null;
        while (current) {
          depth += 1;
          current = engine.projection.nodes.get(current)?.parent_id ?? null;
        }
        return { id, depth };
      })
    : visibleOrder.slice(start, start + count);
  engine.projection.counters.layersFlattenedNodesVisited += mounted.length;
  engine.projection.counters.mountedRows = mounted.length;

  const select = async (id: string, toggle: boolean) => {
    try {
      await engine.send("selection", { target: id, mode: toggle ? "toggle" : "replace" });
    } catch (error) {
      onError(error);
    }
  };

  const onTreeKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const current = engine.projection.primary;
    const index = visibleOrder.findIndex((entry) => entry.id === current);
    if (event.key === "ArrowDown" && index < visibleOrder.length - 1) {
      event.preventDefault();
      void select(visibleOrder[index + 1].id, false);
    } else if (event.key === "ArrowUp" && index > 0) {
      event.preventDefault();
      void select(visibleOrder[index - 1].id, false);
    }
  };

  return (
    <section className="panel layers-panel" aria-label="Layers panel">
      <PanelHeader
        title="Layers"
        action={
          editRoot ? (
            <button className="text-button compact" onClick={() => setEditRoot(null)}>
              Exit group
            </button>
          ) : undefined
        }
      />
      <div
        className="layers-viewport"
        role="tree"
        aria-label="Document layers"
        tabIndex={0}
        onKeyDown={onTreeKeyDown}
        onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
        style={{ height: viewportHeight }}
        data-testid="layers-viewport"
      >
        <div style={{ height: visibleOrder.length * rowHeight, position: "relative" }}>
          {mounted.map(({ id, depth }, mountedIndex) => {
            const node = engine.projection.nodes.get(id);
            if (!node) return null;
            const Icon = kindIcon(node.kind);
            const selected = engine.projection.selection.includes(id);
            const hasChildren = node.children.length > 0;
            const top = (start + mountedIndex) * rowHeight;
            return (
              <div
                key={id}
                className={`tree-row${selected ? " is-selected" : ""}`}
                style={{ top, height: rowHeight, paddingLeft: 6 + depth * 14 }}
                role="treeitem"
                aria-level={depth + 1}
                aria-selected={selected}
                aria-expanded={hasChildren ? expanded.has(id) : undefined}
                onClick={(event) => void select(id, event.shiftKey)}
                onDoubleClick={() => {
                  if (node.kind === "group" || node.kind === "frame") setEditRoot(id);
                }}
                data-node-id={id}
              >
                <button
                  type="button"
                  className="tree-disclosure"
                  aria-label={hasChildren ? `Toggle ${node.name}` : undefined}
                  disabled={!hasChildren}
                  onClick={(event) => {
                    event.stopPropagation();
                    setExpanded((current) => {
                      const next = new Set(current);
                      if (next.has(id)) next.delete(id);
                      else next.add(id);
                      return next;
                    });
                  }}
                >
                  {hasChildren ? expanded.has(id) ? <ChevronDown size={14} /> : <ChevronRight size={14} /> : null}
                </button>
                <Icon className="tree-kind" size={15} aria-hidden="true" />
                <span className="tree-name" title={node.name}>{node.name}</span>
                <button
                  className="tree-state"
                  aria-label={`${node.visible ? "Hide" : "Show"} ${node.name}`}
                  onClick={(event) => {
                    event.stopPropagation();
                    void engine
                      .send("command", {
                        command: { kind: "set_visible", node_id: id, visible: !node.visible },
                      })
                      .catch(onError);
                  }}
                >
                  {node.visible ? <Eye size={14} /> : <EyeOff size={14} />}
                </button>
                <button
                  className="tree-state"
                  aria-label={`${node.locked ? "Unlock" : "Lock"} ${node.name}`}
                  onClick={(event) => {
                    event.stopPropagation();
                    void engine
                      .send("command", {
                        command: { kind: "set_locked", node_id: id, locked: !node.locked },
                      })
                      .catch(onError);
                  }}
                >
                  {node.locked ? <Lock size={14} /> : <Unlock size={14} />}
                </button>
              </div>
            );
          })}
        </div>
      </div>
      <div className="panel-footnote">
        {totalRows.toLocaleString()} nodes · {mounted.length} mounted
      </div>
    </section>
  );
}

function NumericField({
  label,
  value,
  mixed,
  unit,
  disabled,
  positive,
  onCommit,
}: {
  label: string;
  value: number;
  mixed?: boolean;
  unit?: string;
  disabled?: boolean;
  positive?: boolean;
  onCommit: (value: number) => void;
}) {
  const [draft, setDraft] = useState(mixed ? "" : String(Number(value.toFixed(3))));
  const [validationCode, setValidationCode] = useState<string | null>(null);
  const errorId = "numeric-error-" + label.toLowerCase().replace(/[^a-z0-9]+/g, "-");
  useEffect(() => {
    setDraft(mixed ? "" : String(Number(value.toFixed(3))));
    setValidationCode(null);
  }, [value, mixed]);
  const commit = () => {
    const parsed = Number(draft);
    if (!Number.isFinite(parsed)) {
      setValidationCode("numeric_non_finite");
      return;
    }
    if (positive && parsed <= 0) {
      setValidationCode("numeric_non_positive");
      return;
    }
    setValidationCode(null);
    onCommit(parsed);
  };
  return (
    <label className={"field numeric-field" + (validationCode ? " invalid" : "")}>
      <span>{label}</span>
      <div className="field-control">
        <input
          value={draft}
          placeholder={mixed ? "Mixed" : undefined}
          data-mixed={mixed ? "true" : undefined}
          inputMode="decimal"
          disabled={disabled}
          aria-label={label}
          aria-invalid={validationCode ? "true" : undefined}
          aria-describedby={validationCode ? errorId : undefined}
          data-validation-code={validationCode ?? undefined}
          onChange={(event) => {
            setDraft(event.target.value);
            if (validationCode) setValidationCode(null);
          }}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") event.currentTarget.blur();
            if (event.key === "Escape") {
              setDraft(mixed ? "" : String(Number(value.toFixed(3))));
              setValidationCode(null);
              event.currentTarget.blur();
            }
          }}
        />
        {unit ? <span className="field-suffix">{unit}</span> : null}
      </div>
      {validationCode ? <small id={errorId} role="alert">{validationCode}: Enter a {validationCode === "numeric_non_positive" ? "positive" : "finite"} number.</small> : null}
    </label>
  );
}

function commonValue(values: number[]): { value: number; mixed: boolean } {
  if (values.length === 0) return { value: 0, mixed: false };
  return { value: values[0], mixed: values.some((value) => Math.abs(value - values[0]) > 1e-9) };
}

function Inspector({ engine, version, activeSlideId, onError }: { engine: EngineClient; version: number; activeSlideId: string | null; onError: (error: unknown) => void }) {
  const nodes = engine.projection.selection
    .map((id) => engine.projection.nodes.get(id))
    .filter((candidate): candidate is StoredProjectionNode => Boolean(candidate));
  const isSlideContext = nodes.length === 0 && Boolean(activeSlideId);
  const node = nodes[0] ?? (activeSlideId ? engine.projection.nodes.get(activeSlideId) : undefined);
  const editableNodes = isSlideContext ? (node ? [node] : []) : nodes;
  const xValue = commonValue(editableNodes.map((candidate) => candidate.local_transform[4]));
  const yValue = commonValue(editableNodes.map((candidate) => candidate.local_transform[5]));
  const widthValue = commonValue(editableNodes.flatMap((candidate) => candidate.geometry
    ? [candidate.geometry.width * Math.hypot(candidate.local_transform[0], candidate.local_transform[2])]
    : []));
  const heightValue = commonValue(editableNodes.flatMap((candidate) => candidate.geometry
    ? [candidate.geometry.height * Math.hypot(candidate.local_transform[1], candidate.local_transform[3])]
    : []));
  const rotationValue = commonValue(editableNodes.map((candidate) => Math.atan2(candidate.local_transform[2], candidate.local_transform[0]) * (180 / Math.PI)));
  const appearance = node?.appearance ?? DEFAULT_APPEARANCE;
  const sendBatch = (commands: Array<Record<string, unknown>>) => {
    if (!commands.length) return;
    void engine.send("command_batch", { commands }).catch(onError);
  };
  const setAxis = (axis: "x" | "y", value: number) => {
    if (isSlideContext) return;
    sendBatch(editableNodes.map((candidate) => ({
      kind: "set_transform", node_id: candidate.id,
      matrix: candidate.local_transform.map((entry, index) => index === (axis === "x" ? 4 : 5) ? value : entry),
    })));
  };
  const updateRotation = (degrees: number) => {
    if (isSlideContext) return;
    const radians = degrees * (Math.PI / 180);
    sendBatch(editableNodes.map((candidate) => {
      const current = candidate.local_transform;
      const sx = Math.hypot(current[0], current[2]);
      const sy = Math.hypot(current[1], current[3]);
      return { kind: "set_transform", node_id: candidate.id, matrix: [Math.cos(radians) * sx, -Math.sin(radians) * sy, Math.sin(radians) * sx, Math.cos(radians) * sy, current[4], current[5]] };
    }));
  };
  const setGeometryAxis = (axis: "width" | "height", value: number) => {
    if (!Number.isFinite(value) || value <= 0) return;
    sendBatch(editableNodes.flatMap((candidate) => {
      if (!candidate.geometry || (candidate.kind !== "frame" && candidate.kind !== "rectangle" && candidate.kind !== "ellipse")) return [];
      const scaleX = Math.hypot(candidate.local_transform[0], candidate.local_transform[2]);
      const scaleY = Math.hypot(candidate.local_transform[1], candidate.local_transform[3]);
      if (scaleX <= 0 || scaleY <= 0) return [];
      return [{
        kind: "set_geometry", node_id: candidate.id, shape: candidate.kind,
        width: axis === "width" ? value / scaleX : candidate.geometry.width,
        height: axis === "height" ? value / scaleY : candidate.geometry.height,
      }];
    }));
  };

  const setFill = (value: string) => {
    if (!node) return;
    void engine.send("command", {
      command: { kind: "set_fill", node_id: node.id, color: hexToColor(value, appearance.fill[3]) },
    }).catch(onError);
  };
  const setCornerRadius = (radius: number) => {
    if (!node) return;
    const safeRadius = Math.max(0, radius);
    void engine.send("command", {
      command: { kind: "set_corner_radii", node_id: node.id, radii: [safeRadius, safeRadius, safeRadius, safeRadius] },
    }).catch(onError);
  };
  const setStroke = (color: [number, number, number, number], width: number) => {
    if (!node) return;
    void engine.send("command", {
      command: { kind: "set_stroke", node_id: node.id, color, width: Math.max(0, width) },
    }).catch(onError);
  };
  return (
    <section className="panel inspector" aria-label="Transform inspector" data-version={version}>
      <PanelHeader title="Inspector" />
      {!node || node.kind === "document" ? (
        <div className="empty-state">
          <MousePointer2 size={22} aria-hidden="true" />
          <strong>No editable selection</strong>
          <span>Select a shape or group on the canvas or in Layers.</span>
        </div>
      ) : (
        <div className="inspector-content">
          <div className="node-summary">
            <span className="badge">{isSlideContext ? "Slide" : nodes.length > 1 ? `${nodes.length} selected` : node.kind}</span>
            {isSlideContext && node.geometry ? <span>{Number((node.geometry.width / node.geometry.height).toFixed(3))}:1</span> : <span className="mono">{node.id.slice(0, 8)}</span>}
          </div>
          {nodes.length <= 1 ? <label className="field field-wide">
            <span>Name</span>
            <input
              key={`${node.id}-${node.name}`}
              defaultValue={node.name}
              onBlur={(event) => {
                if (event.currentTarget.value !== node.name) {
                  void engine
                    .send("command", {
                      command: { kind: "set_name", node_id: node.id, name: event.currentTarget.value },
                    })
                    .catch(onError);
                }
              }}
            />
          </label> : null}
          <div className="field-grid">
            <NumericField label="X" value={xValue.value} mixed={xValue.mixed} disabled={isSlideContext} onCommit={(value) => setAxis("x", value)} />
            <NumericField label="Y" value={yValue.value} mixed={yValue.mixed} disabled={isSlideContext} onCommit={(value) => setAxis("y", value)} />
            <NumericField label="W" value={widthValue.value} mixed={widthValue.mixed} positive disabled={!editableNodes.every((candidate) => candidate.geometry)} onCommit={(value) => setGeometryAxis("width", value)} />
            <NumericField label="H" value={heightValue.value} mixed={heightValue.mixed} positive disabled={!editableNodes.every((candidate) => candidate.geometry)} onCommit={(value) => setGeometryAxis("height", value)} />
            <NumericField label="Rotation" value={rotationValue.value} mixed={rotationValue.mixed} disabled={isSlideContext} unit="°" onCommit={updateRotation} />
            <NumericField
              label="Opacity"
              value={node.opacity * 100}
              unit="%"
              onCommit={(value) => void engine.send("command", { command: { kind: "set_opacity", node_id: node.id, opacity: Math.max(0, Math.min(100, value)) / 100 } }).catch(onError)}
            />
          </div>
          {nodes.length <= 1 ? <div className="appearance-section" aria-label="Appearance">
            <div className="appearance-heading">Appearance</div>
            <div className="appearance-grid">
              <label className="field color-field">
                <span>Fill</span>
                <input
                  type="color"
                  aria-label="Fill color"
                  value={colorToHex(appearance.fill)}
                  onChange={(event) => setFill(event.currentTarget.value)}
                />
              </label>
              <label className="field color-field">
                <span>Stroke</span>
                <input
                  type="color"
                  aria-label="Stroke color"
                  value={colorToHex(appearance.stroke.color)}
                  onChange={(event) => setStroke(hexToColor(event.currentTarget.value, appearance.stroke.color[3]), appearance.stroke.width)}
                />
              </label>
              {(node.kind === "frame" || node.kind === "rectangle") ? (
                <NumericField
                  label="Radius"
                  value={appearance.corner_radii[0]}
                  onCommit={setCornerRadius}
                />
              ) : <span />}
              <NumericField
                label="Stroke width"
                value={appearance.stroke.width}
                onCommit={(width) => setStroke(appearance.stroke.color, width)}
              />
            </div>
            <small>Centered stroke · sRGB input · linear premultiplied GPU output</small>
          </div> : null}          <div className="inline-controls">
            <label className="check-control">
              <input
                type="checkbox"
                checked={node.visible}
                onChange={() => void engine.send("command", { command: { kind: "set_visible", node_id: node.id, visible: !node.visible } }).catch(onError)}
              />
              <span>Visible</span>
            </label>
            <label className="check-control">
              <input
                type="checkbox"
                checked={node.locked}
                onChange={() => void engine.send("command", { command: { kind: "set_locked", node_id: node.id, locked: !node.locked } }).catch(onError)}
              />
              <span>Locked</span>
            </label>
          </div>
        </div>
      )}
    </section>
  );
}

function ComponentShowcase({ onClose }: { onClose: () => void }) {
  const dialogRef = useRef<HTMLElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const [segment, setSegment] = useState(0);
  const [tab, setTab] = useState(0);
  const [menuOpen, setMenuOpen] = useState(false);
  const segments = ["Design", "Inspect", "Debug"];
  const tabs = ["Foundations", "Components", "Patterns"];

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    closeRef.current?.focus();
    return () => previous?.focus();
  }, []);

  const onDialogKeyDown = (event: ReactKeyboardEvent<HTMLElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = [...(dialogRef.current?.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex='-1'])") ?? [])];
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  const moveSegment = (next: number) => {
    const normalized = (next + segments.length) % segments.length;
    setSegment(normalized);
    requestAnimationFrame(() => dialogRef.current?.querySelectorAll<HTMLButtonElement>("[role='radio']")[normalized]?.focus());
  };

  return (
    <div className="dialog-backdrop" role="presentation">
      <section ref={dialogRef} className="showcase" role="dialog" aria-modal="true" aria-labelledby="showcase-title" data-testid="component-showcase" onKeyDown={onDialogKeyDown}>
        <div className="panel-header">
          <h2 id="showcase-title">Wanted component showcase</h2>
          <button ref={closeRef} className="icon-button" aria-label="Close showcase" onClick={onClose}><X size={16} /></button>
        </div>
        <div className="showcase-grid">
          <article>
            <h3>Buttons and states</h3>
            <div className="showcase-row">
              <button className="button primary">Default</button>
              <button className="button secondary is-hover-demo">Hover</button>
              <button className="button secondary is-pressed-demo" aria-pressed="true">Pressed</button>
              <button className="button secondary is-focus-demo">Focus</button>
              <button className="button secondary" disabled>Disabled</button>
            </div>
          </article>
          <article>
            <h3>Textfield and Select</h3>
            <label className="field field-wide"><span>Textfield</span><input placeholder="Enter a value" /></label>
            <label className="field field-wide invalid"><span>Error</span><input defaultValue="Invalid" aria-invalid="true" aria-describedby="showcase-field-error" /><small id="showcase-field-error">Use a finite value.</small></label>
            <label className="field field-wide"><span>Select</span><select defaultValue="medium"><option value="small">Small</option><option value="medium">Medium</option><option value="large">Large</option></select></label>
          </article>
          <article>
            <h3>Tabs</h3>
            <div className="showcase-tabs" role="tablist" aria-label="Showcase sections">
              {tabs.map((label, index) => <button key={label} role="tab" aria-selected={tab === index} tabIndex={tab === index ? 0 : -1} onClick={() => setTab(index)}>{label}</button>)}
            </div>
            <div role="tabpanel" className="showcase-tabpanel">{tabs[tab]} tokens and states</div>
          </article>
          <article>
            <h3>Segmented Control</h3>
            <div className="segmented" role="radiogroup" aria-label="Example segmented control">
              {segments.map((label, index) => (
                <button key={label} role="radio" aria-checked={segment === index} tabIndex={segment === index ? 0 : -1} className={segment === index ? "is-selected" : undefined} onClick={() => setSegment(index)} onKeyDown={(event) => {
                  if (event.key === "ArrowRight" || event.key === "ArrowDown") { event.preventDefault(); moveSegment(index + 1); }
                  if (event.key === "ArrowLeft" || event.key === "ArrowUp") { event.preventDefault(); moveSegment(index - 1); }
                  if (event.key === "Home") { event.preventDefault(); moveSegment(0); }
                  if (event.key === "End") { event.preventDefault(); moveSegment(segments.length - 1); }
                }}>{label}</button>
              ))}
            </div>
            <label className="switch-control"><input type="checkbox" defaultChecked role="switch" /><span>Live updates</span></label>
            <label className="check-control"><input type="checkbox" defaultChecked /><span>Show bounds</span></label>
          </article>
          <article>
            <h3>Menu and Tooltip</h3>
            <div className="showcase-row">
              <button aria-haspopup="menu" aria-expanded={menuOpen} onClick={() => setMenuOpen((open) => !open)}>Actions <ChevronDown size={14} /></button>
              <button aria-describedby="showcase-tooltip">Tooltip target</button>
              <span id="showcase-tooltip" className="tooltip-sample" role="tooltip">Shortcut <kbd>V</kbd></span>
            </div>
            {menuOpen ? <div className="showcase-menu" role="menu"><button role="menuitem">Rename</button><button role="menuitem">Delete</button></div> : null}
          </article>
          <article>
            <h3>Badge and selected state</h3>
            <div className="showcase-row"><span className="badge primary-badge">Selected</span><span className="badge">Worker ready</span></div>
          </article>
        </div>
      </section>
    </div>
  );
}

function DebugPanel({ engine, response, onLoad, onError }: { engine: EngineClient; response: EngineResponse | null; onLoad: (fixture: string) => void; onError: string | null }) {
  const [open, setOpen] = useState(false);
  return (
    <section className={`debug-panel${open ? " is-open" : ""}`} aria-label="Debug panel">
      <button className="debug-toggle" data-testid="debug-toggle" aria-expanded={open} onClick={() => setOpen((value) => !value)}>
        <Code2 size={15} /> Debug <ChevronDown size={14} />
      </button>
      {open ? (
        <div className="debug-content" data-testid="debug-content">
          <div className="fixture-actions">
            <button onClick={() => onLoad("preview")}>Demo</button>
            <button data-testid="fixture-10k" onClick={() => onLoad("BENCH-B")}>10k</button>
            <button data-testid="fixture-100k" onClick={() => onLoad("BENCH-C")}>100k</button>
          </div>
          <dl className="metrics-grid">
            <div><dt>Fixture</dt><dd>{response?.fixture ?? "—"}</dd></div>
            <div><dt>Doc / Scene / Render</dt><dd>{response ? `${response.revisions.document} / ${response.revisions.scene} / ${response.revisions.render}` : "—"}</dd></div>
            <div><dt>Nodes / visible</dt><dd>{response ? `${response.metrics.document_nodes?.toLocaleString()} / ${response.culling.visible?.toLocaleString()}` : "—"}</dd></div>
            <div><dt>Mounted rows</dt><dd>{engine.projection.counters.mountedRows}</dd></div>
            <div><dt>UI full snapshots</dt><dd>{response?.metrics.ui_full_snapshots ?? 0}</dd></div>
            <div><dt>UI delta nodes</dt><dd>{response?.metrics.ui_delta_nodes ?? 0}</dd></div>
            <div><dt>Render clones / scans</dt><dd>{response ? `${response.metrics.render_items_cloned} / ${response.metrics.full_render_model_scans}` : "—"}</dd></div>
            <div><dt>Dirty / upload</dt><dd>{response ? `${response.render_delta.dirty_slots} / ${response.metrics.instance_upload_bytes} B` : "—"}</dd></div>
            <div><dt>Pointer raw / sent</dt><dd>{`${engine.projection.counters.pointerRawIntents} / ${engine.projection.counters.pointerRequestsSent}`}</dd></div>
            <div><dt>Pointer coalesced</dt><dd>{engine.projection.counters.pointerRequestsCoalesced}</dd></div>
          </dl>
          {onError ? <div className="error-callout" role="alert">{onError}</div> : null}
        </div>
      ) : null}
    </section>
  );
}

export function App() {
  const engineRef = useRef<EngineClient | null>(null);
  if (!engineRef.current) engineRef.current = new EngineClient();
  const engine = engineRef.current;
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const shellRef = useRef<HTMLDivElement>(null);
  const interaction = useRef<Interaction | null>(null);
  const dragQueue = useRef<{
    generation: number;
    inFlight: boolean;
    scheduled: { generation: number; run: () => Promise<void> } | null;
    latest: { generation: number; run: () => Promise<void> } | null;
  }>({ generation: 0, inFlight: false, scheduled: null, latest: null });
  const [response, setResponse] = useState<EngineResponse | null>(null);
  const [version, setVersion] = useState(0);
  const [heartbeatTick, setHeartbeatTick] = useState(0);
  const [tool, setTool] = useState<Tool>("select");
  const [fsm, setFsm] = useState<FsmState>("Idle");
  const [error, setError] = useState<string | null>(null);
  const [showcase, setShowcase] = useState(false);
  const [editRoot, setEditRoot] = useState<string | null>(null);
  const [ready, setReady] = useState(false);
  const [customFrameWidth, setCustomFrameWidth] = useState(1440);
  const [customFrameHeight, setCustomFrameHeight] = useState(900);
  const [marquee, setMarquee] = useState<ViewportRect | null>(null);
  const [guides, setGuides] = useState<SnapGuideProjection[]>([]);
  const [snapEnabled, setSnapEnabled] = useState(true);

  const fail = useCallback((reason: unknown) => {
    const message = reason instanceof Error ? `${"code" in reason ? `${String((reason as EngineFailure).code)}: ` : ""}${reason.message}` : String(reason);
    setError(message);
    window.__phase0eState = { ready: false, lastError: message };
  }, []);

  useEffect(() => engine.subscribe((next) => {
    setResponse(next);
    setVersion(engine.projection.version);
    setError(null);
    window.__phase0eState = { ready: true, lastError: null };
  }), [engine]);

  useEffect(() => engine.subscribeActivity(() => {
    setHeartbeatTick((current) => current + 1);
  }), [engine]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    let cancelled = false;
    window.__phase0eReadPixel = (x, y) => engine.readPixel(x, y);
    window.__phase0eSend = (type, payload = {}) => engine.send(type, payload);
    window.__phase0eR2Counters = () => ({ ...engine.projection.counters, orderLength: engine.projection.order.length });
    window.__phase0eR2Order = (start, count) => engine.projection.viewport(start, count);
    void engine.initialize(canvas).then((initial) => {
      if (cancelled) return;
      setResponse(initial);
      setVersion(engine.projection.version);
      setReady(true);
      document.body.dataset.ready = "true";
      window.__phase0eState = { ready: true, lastError: null };
    }).catch(fail);
    return () => {
      cancelled = true;
    };
  }, [engine, fail]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !ready) return;
    const observer = new ResizeObserver(([entry]) => {
      const width = entry.contentRect.width;
      const height = entry.contentRect.height;
      if (width > 0 && height > 0) {
        void engine.send("camera", { camera: { kind: "resize", width, height, dpr: devicePixelRatio } }).catch(fail);
      }
    });
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [engine, ready, fail]);

  useEffect(() => {
    window.__PHASE0E_PROOF__ = engine.proof(response, {
      fsm,
      tool,
      nested_edit_root: editRoot,
      console_errors: error ? 1 : 0,
      fallback_rebuild_count: response?.metrics.fallback_rebuild_count ?? 0,
      gpu_validation_errors: engine.gpuMetrics.validation_errors ?? 0,
      history: response?.history ?? null,
      camera: response?.camera ?? null,
      projection_nodes: engine.projection.nodes.size,
      projection_schema_version: response?.projection.schema_version ?? null,
      primary_node: proofProjectionNode(engine.projection.primary ? engine.projection.nodes.get(engine.projection.primary) : undefined),
      render_delta: response?.render_delta ?? null,
      render_binary_schema_version: response?.render_binary_schema_version ?? null,
      resources: response?.resources ?? null,
      binary: response?.binary ?? null,
      interaction_active: interaction.current ? { pointer_id: interaction.current.pointerId, kind: interaction.current.kind } : null,
      selection_count: engine.projection.selection.length,
      snap_enabled: snapEnabled,
      snap_guides: guides.length,
      marquee_active: Boolean(marquee),
      interaction_queue: {
        generation: dragQueue.current.generation,
        in_flight: dragQueue.current.inFlight,
        scheduled: Boolean(dragQueue.current.scheduled),
        latest: Boolean(dragQueue.current.latest),
      },
    });
  }, [engine, response, version, heartbeatTick, fsm, tool, editRoot, error, snapEnabled, guides, marquee]);

  const currentNode = engine.projection.primary ? engine.projection.nodes.get(engine.projection.primary) : undefined;
  const documentRoot = engine.projection.rootId;
  const activeSlideId = response?.active_root && response.active_root !== documentRoot
    ? response.active_root
    : null;
  const selectionRoot = editRoot ?? activeSlideId ?? documentRoot;
  const worldToViewport = useCallback((point: [number, number]): [number, number] => {
    const camera = response?.camera;
    if (!camera) return point;
    return [
      (point[0] - camera.center[0]) * camera.zoom + camera.viewport[0] / 2,
      (point[1] - camera.center[1]) * camera.zoom + camera.viewport[1] / 2,
    ];
  }, [response]);
  const viewportToWorld = useCallback((point: [number, number]): [number, number] => {
    const camera = response?.camera;
    if (!camera) return point;
    return [
      (point[0] - camera.viewport[0] / 2) / camera.zoom + camera.center[0],
      (point[1] - camera.viewport[1] / 2) / camera.zoom + camera.center[1],
    ];
  }, [response]);

  const selectedNodes = useMemo(
    () => engine.projection.selection
      .map((id) => engine.projection.nodes.get(id))
      .filter((node): node is StoredProjectionNode => Boolean(node)),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [engine, version, response],
  );

  const transformOverlay = useMemo(() => {
    if (!response || selectedNodes.length === 0) return null;
    let worldBox: TransformBox | null = null;
    if (selectedNodes.length === 1) {
      const node = selectedNodes[0];
      if (node.geometry && node.world_transform) {
        const corners = ([[0, 0], [node.geometry.width, 0], [node.geometry.width, node.geometry.height], [0, node.geometry.height]] as Array<[number, number]>)
          .map((point) => transformAffinePoint(node.world_transform!, point)) as [[number, number], [number, number], [number, number], [number, number]];
        worldBox = orientedBox(corners);
      }
    }
    if (!worldBox) {
      const bounds = unionWorldBounds(selectedNodes.map((node) => node.world_bounds));
      if (bounds) worldBox = axisAlignedBox(bounds.min, bounds.max);
    }
    if (!worldBox) return null;
    const points = worldBox.points.map(worldToViewport);
    const centerViewport = worldToViewport(worldBox.center);
    const handles = Object.fromEntries(RESIZE_HANDLES.map((handle) => [handle, worldToViewport(handlePoint(worldBox!, handle))])) as Record<ResizeHandle, [number, number]>;
    const top = handles.n;
    const rotate: [number, number] = [
      top[0] - worldBox.axisY[0] * 24,
      top[1] - worldBox.axisY[1] * 24,
    ];
    return { worldBox, points, centerViewport, handles, rotate };
  }, [response, selectedNodes, worldToViewport]);

  const scheduleDrag = useCallback((operation: () => Promise<void>) => {
    engine.projection.counters.pointerRawIntents += 1;
    const queue = dragQueue.current;
    const intent = { generation: queue.generation, run: operation };
    if (queue.inFlight) {
      if (queue.latest) engine.projection.counters.pointerRequestsCoalesced += 1;
      queue.latest = intent;
      return;
    }
    queue.inFlight = true;
    queue.scheduled = intent;

    const scheduleDrain = () => {
      requestAnimationFrame(() => requestAnimationFrame(() => void drainLatest()));
    };
    const drainLatest = async () => {
      const current = queue.scheduled;
      queue.scheduled = null;
      if (!current || current.generation !== queue.generation) {
        queue.inFlight = false;
        return;
      }
      engine.projection.counters.pointerRequestsSent += 1;
      try {
        await current.run();
      } catch (reason) {
        if (current.generation === queue.generation) fail(reason);
      }
      if (current.generation !== queue.generation) {
        queue.scheduled = null;
        queue.latest = null;
        queue.inFlight = false;
      } else if (queue.latest) {
        queue.scheduled = queue.latest;
        queue.latest = null;
        scheduleDrain();
      } else {
        queue.inFlight = false;
      }
    };
    scheduleDrain();
  }, [engine, fail]);

  const cancelInteraction = useCallback(async () => {
    const active = interaction.current;
    interaction.current = null;
    setMarquee(null);
    setGuides([]);
    const queue = dragQueue.current;
    queue.generation += 1;
    queue.scheduled = null;
    queue.latest = null;
    while (queue.inFlight) await new Promise((resolve) => setTimeout(resolve, 4));
    if (active && active.kind !== "pan" && active.kind !== "marquee") {
      await engine.send("rollback_transaction");
    }
    setFsm(editRoot ? "NestedEditing" : "Idle");
  }, [engine, editRoot]);

  const pointerPosition = (event: ReactPointerEvent): [number, number] => {
    const rect = event.currentTarget.getBoundingClientRect();
    return [event.clientX - rect.left, event.clientY - rect.top];
  };

  // The engine captures the transform of every selected node when the transaction opens, so a
  // move only sends one world delta per frame no matter how many nodes are selected.
  const beginMove = async (
    pointerId: number,
    node: StoredProjectionNode,
    point: [number, number],
    captureTarget: HTMLDivElement,
  ) => {
    if (node.locked) return;
    await engine.send("begin_transaction");
    interaction.current = {
      pointerId,
      kind: "move",
      start: point,
      last: point,
      nodeId: node.id,
    };
    captureTarget.setPointerCapture(pointerId);
    setFsm("Moving");
  };

  const beginHandleAt = async (
    pointerId: number,
    clientX: number,
    clientY: number,
    kind: "resize" | "rotate",
    handle?: ResizeHandle,
  ) => {
    if (!currentNode || currentNode.locked || !transformOverlay) return;
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) return;
    const point: [number, number] = [clientX - rect.left, clientY - rect.top];
    await engine.send("begin_transaction");
    interaction.current = {
      pointerId,
      kind,
      start: point,
      last: point,
      resizeHandle: handle,
      transformBox: transformOverlay.worldBox,
      rotationStart: Math.atan2(
        point[1] - transformOverlay.centerViewport[1],
        point[0] - transformOverlay.centerViewport[0],
      ),
    };
    shellRef.current?.setPointerCapture(pointerId);
    setFsm(kind === "resize" ? "Resizing" : "Rotating");
  };
  const onCanvasPointerDown = async (event: ReactPointerEvent<HTMLDivElement>) => {
    if (!ready || event.button !== 0) return;
    const point = pointerPosition(event);
    const captureTarget = event.currentTarget;
    const pointerId = event.pointerId;
    try {
      const handleKind = (event.target as HTMLElement).dataset.testid;
      const resizeHandle = (event.target as HTMLElement).dataset.resizeHandle as ResizeHandle | undefined;
      if (handleKind === "resize-handle" || handleKind === "rotate-handle") {
        await beginHandleAt(pointerId, event.clientX, event.clientY, handleKind === "resize-handle" ? "resize" : "rotate", resizeHandle);
        return;
      }
      if (tool === "hand") {
        interaction.current = { pointerId: event.pointerId, kind: "pan", start: point, last: point, panPending: [0, 0] };
        event.currentTarget.setPointerCapture(event.pointerId);
        setFsm("Panning");
        return;
      }
      if (tool === "frame" || tool === "rectangle" || tool === "ellipse") {
        await engine.send("begin_transaction");
        interaction.current = {
          pointerId: event.pointerId,
          kind: "create",
          start: point,
          last: point,
          nodeId: crypto.randomUUID(),
          nodeKind: tool,
          created: false,
        };
        captureTarget.setPointerCapture(event.pointerId);
        setFsm(tool === "rectangle" ? "CreatingRectangle" : tool === "ellipse" ? "CreatingEllipse" : "CreatingFrame");
        return;
      }
      setFsm("Selecting");
      const hit = await engine.send("hit_test", { x: point[0], y: point[1], root_id: selectionRoot, selectable_only: true });
      const hitIds = Array.isArray((hit.result as { all?: unknown } | null)?.all)
        ? ((hit.result as { all: string[] }).all)
        : [];
      const isInSelectionRoot = (id: string) => {
        if (!selectionRoot || id === selectionRoot) return false;
        let current = engine.projection.nodes.get(id);
        while (current?.parent_id) {
          if (!current.visible || current.locked) return false;
          if (current.parent_id === selectionRoot) return true;
          current = engine.projection.nodes.get(current.parent_id);
        }
        return false;
      };
      const target = hitIds.find(isInSelectionRoot) ?? null;
      const intent = selectionIntent(target, engine.projection.selection, event.shiftKey);
      if (intent.mode === "marquee") {
        interaction.current = { pointerId, kind: "marquee", start: point, last: point, additive: event.shiftKey };
        // No node under the pointer: a release without travel clears the selection.
        setMarquee(marqueeRect(point, point));
        captureTarget.setPointerCapture(pointerId);
        setFsm("MarqueeSelecting");
        return;
      }
      if (intent.mode === "toggle") {
        await engine.send("selection", { target: intent.target, mode: "toggle" });
        setFsm(editRoot ? "NestedEditing" : "Idle");
        return;
      }
      const node = intent.target ? engine.projection.nodes.get(intent.target) : undefined;
      if (pressStartsMarquee(node?.kind, intent.mode === "keep")) {
        // Pressing a Frame's own area bands across its children; a press without travel still
        // selects the Frame itself when the pointer is released.
        interaction.current = {
          pointerId,
          kind: "marquee",
          start: point,
          last: point,
          additive: event.shiftKey,
          nodeId: intent.target ?? undefined,
          // The pressed container is the band's backdrop and is excluded from its result.
          marqueeRoot: intent.target ?? undefined,
        };
        setMarquee(marqueeRect(point, point));
        captureTarget.setPointerCapture(pointerId);
        setFsm("MarqueeSelecting");
        return;
      }
      if (intent.mode === "replace") {
        await engine.send("selection", { target: intent.target, mode: "replace" });
      }
      if (node) await beginMove(pointerId, node, point, captureTarget);
      else setFsm(editRoot ? "NestedEditing" : "Idle");
    } catch (reason) {
      fail(reason);
      setFsm("Idle");
    }
  };

  const onCanvasPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const active = interaction.current;
    if (!active || active.pointerId !== event.pointerId) return;
    const point = pointerPosition(event);
    if (active.kind === "pan") {
      const dx = point[0] - active.last[0];
      const dy = point[1] - active.last[1];
      active.last = point;
      const pending = active.panPending ?? [0, 0];
      pending[0] += dx;
      pending[1] += dy;
      active.panPending = pending;
      scheduleDrag(() => {
        const accumulated = active.panPending ?? [0, 0];
        active.panPending = [0, 0];
        return engine.send("camera", { camera: { kind: "pan", dx: accumulated[0], dy: accumulated[1] } }).then(() => undefined);
      });
      return;
    }
    if (active.kind === "marquee") {
      active.last = point;
      setMarquee(marqueeRect(active.start, point));
      return;
    }
    if (active.kind === "move") {
      const zoom = response?.camera.zoom ?? 1;
      const dx = (point[0] - active.start[0]) / zoom;
      const dy = (point[1] - active.start[1]) / zoom;
      // Alt suspends snapping for the rest of this drag frame, as in other vector editors.
      const snap = snapEnabled && !event.altKey;
      scheduleDrag(() => engine
        .send("translate_selection", { dx, dy, snap, snap_threshold_px: SNAP_THRESHOLD_PX })
        .then((next) => {
          setGuides(snap ? readGuides(next.result) : []);
        }));
      return;
    }
    if (active.kind === "create" && active.nodeId && active.nodeKind) {
      active.last = point;
      const start = viewportToWorld(active.start);
      const end = viewportToWorld(point);
      const x = Math.min(start[0], end[0]);
      const y = Math.min(start[1], end[1]);
      const width = Math.max(1, Math.abs(end[0] - start[0]));
      const height = Math.max(1, Math.abs(end[1] - start[1]));
      const root = active.nodeKind === "frame" ? documentRoot : selectionRoot;
      if (!root) return;
      if (!active.created) {
        active.created = true;
        scheduleDrag(() => engine.send("update_transaction", {
          command: {
            kind: "create_shape", node_id: active.nodeId, parent_id: root,
            index: engine.projection.nodes.get(root)?.children.length ?? 0,
            shape: active.nodeKind, name: shapeName(active.nodeKind!),
            x, y, width, height,
          },
        }).then(() => undefined));
      } else {
        scheduleDrag(async () => {
          await engine.send("update_transaction", { command: { kind: "set_transform", node_id: active.nodeId, matrix: [1, 0, 0, 1, x, y] } });
          await engine.send("update_transaction", { command: { kind: "set_geometry", node_id: active.nodeId, shape: active.nodeKind, width, height } });
        });
      }
      return;
    }
    if (active.kind === "resize" && active.transformBox && active.resizeHandle) {
      const matrix = resizeTransform(
        active.transformBox,
        active.resizeHandle,
        viewportToWorld(point),
        event.shiftKey,
        event.altKey,
      );
      if (matrix) scheduleDrag(() => engine.send("transform_selection", { matrix }).then(() => undefined));
      return;
    }
    if (active.kind === "rotate" && active.transformBox && active.rotationStart !== undefined) {
      const center = worldToViewport(active.transformBox.center);
      const currentAngle = Math.atan2(point[1] - center[1], point[0] - center[0]);
      const delta = snappedRotationDelta(currentAngle - active.rotationStart, event.shiftKey);
      const matrix = rotationAround(active.transformBox.center, delta);
      scheduleDrag(() => engine.send("transform_selection", { matrix }).then(() => undefined));
    }
  };

  const finishInteraction = async (event: ReactPointerEvent<HTMLDivElement>) => {
    const active = interaction.current;
    if (!active || active.pointerId !== event.pointerId) return;
    interaction.current = null;
    setGuides([]);
    try {
      while (dragQueue.current.inFlight) await new Promise((resolve) => setTimeout(resolve, 4));
      if (active.kind === "marquee") {
        setMarquee(null);
        const root = selectionRoot;
        if (isMarqueeDrag(active.start, active.last)) {
          await engine.send("marquee_select", {
            x0: active.start[0],
            y0: active.start[1],
            x1: active.last[0],
            y1: active.last[1],
            additive: Boolean(active.additive),
            root_id: root,
            // The container the band was drawn on is the backdrop, not a target.
            exclude_ids: [
              ...(root ? [root] : []),
              ...(active.marqueeRoot ? [active.marqueeRoot] : []),
              ...[...engine.projection.nodes.values()]
                .filter((node) => node.locked || !node.visible)
                .map((node) => node.id),
            ],
          });
        } else if (active.nodeId) {
          await engine.send("selection", { target: active.nodeId, mode: active.additive ? "toggle" : "replace" });
        } else if (!active.additive) {
          await engine.send("selection", { target: null, mode: "clear" });
        }
        setFsm(editRoot ? "NestedEditing" : "Idle");
        return;
      }
      if (active.kind !== "pan") {
        if (active.kind === "create" && !active.created && active.nodeId && active.nodeKind) {
          const start = viewportToWorld(active.start);
          const root = active.nodeKind === "frame" ? documentRoot : selectionRoot;
          if (root) {
            await engine.send("update_transaction", { command: { kind: "create_shape", node_id: active.nodeId, parent_id: root, index: engine.projection.nodes.get(root)?.children.length ?? 0, shape: active.nodeKind, name: shapeName(active.nodeKind!), x: start[0], y: start[1], width: 24, height: 24 } });
          }
        }
        await engine.send("commit_transaction");
        if (active.kind === "create" && active.nodeId) {
          if (active.nodeKind === "frame") {
            await engine.send("set_active_root", { node_id: active.nodeId });
          } else {
            await engine.send("selection", { target: active.nodeId, mode: "replace" });
          }
        }
      }
    } catch (reason) {
      fail(reason);
    }
    setFsm(editRoot ? "NestedEditing" : "Idle");
    if (active.kind === "create") setTool("select");
  };


  const onWheel = (event: React.WheelEvent<HTMLDivElement>) => {
    if (!response) return;
    const point = pointerPosition(event as unknown as ReactPointerEvent);
    const zoom = Math.max(0.05, Math.min(64, response.camera.zoom * Math.exp(-event.deltaY * 0.0015)));
    scheduleDrag(() => engine.send("camera", { camera: { kind: "zoom", x: point[0], y: point[1], zoom } }).then(() => undefined));
  };

  /** Outline polygons for every selected node, drawn under the primary transform handles. */
  const selectionOutlines = useMemo(() => selectedNodes
    .filter((node) => node.geometry && node.world_transform)
    .map((node) => ({
      id: node.id,
      points: ([[0, 0], [node.geometry!.width, 0], [node.geometry!.width, node.geometry!.height], [0, node.geometry!.height]] as Array<[number, number]>)
        .map((point) => worldToViewport(transformAffinePoint(node.world_transform!, point))),
    })), [selectedNodes, worldToViewport]);

  /** Viewport rectangle around a multiple selection. */
  const selectionUnion = useMemo(() => {
    if (selectedNodes.length < 2) return null;
    const bounds = unionWorldBounds(selectedNodes.map((node) => node.world_bounds));
    if (!bounds) return null;
    const [x1, y1] = worldToViewport(bounds.min);
    const [x2, y2] = worldToViewport(bounds.max);
    return { x: Math.min(x1, x2), y: Math.min(y1, y2), width: Math.abs(x2 - x1), height: Math.abs(y2 - y1) };
  }, [selectedNodes, worldToViewport]);

  const guideSegments = useMemo(
    () => guides.map((guide) => ({ guide, segment: guideSegment(guide, worldToViewport) })),
    [guides, worldToViewport],
  );

  const availability = arrangeAvailability(engine.projection.selection.length);
  const arrangeSelection = async (operation: string) => {
    try {
      await engine.send("arrange", { operation });
    } catch (reason) {
      fail(reason);
    }
  };

  const groupSelection = async () => {
    if (engine.projection.selection.length < 2) return;
    try {
      const groupId = crypto.randomUUID();
      await engine.send("command", { command: { kind: "group", group_id: groupId, name: "Group", targets: engine.projection.selection } });
      await engine.send("selection", { target: groupId, mode: "replace" });
    } catch (reason) { fail(reason); }
  };
  const ungroupSelection = async () => {
    const node = currentNode;
    if (!node || node.kind !== "group") return;
    const children = node.children.toArray();
    try {
      await engine.send("command", { command: { kind: "ungroup", node_id: node.id } });
      if (children.length) await engine.send("selection", { mode: "set", targets: children });
    } catch (reason) { fail(reason); }
  };

  const selectAllInRoot = async () => {
    const root = selectionRoot;
    if (!root) return;
    const container = engine.projection.nodes.get(root);
    if (!container) return;
    const targets = container.children.toArray().filter((id) => {
      const node = engine.projection.nodes.get(id);
      return Boolean(node) && node!.visible && !node!.locked;
    });
    try {
      if (targets.length === 0) await engine.send("selection", { target: null, mode: "clear" });
      else await engine.send("selection", { mode: "set", targets });
    } catch (reason) {
      fail(reason);
    }
  };

  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.matches("input, textarea, select")) return;
      const modifier = event.ctrlKey || event.metaKey;
      if (event.key === "Escape" && interaction.current) {
        event.preventDefault();
        void cancelInteraction().catch(fail);
      } else if (modifier && event.key.toLowerCase() === "z") {
        event.preventDefault();
        void engine.send(event.shiftKey ? "redo" : "undo").catch(fail);
      } else if (modifier && event.key.toLowerCase() === "y") {
        event.preventDefault();
        void engine.send("redo").catch(fail);
      } else if (modifier && event.key.toLowerCase() === "a") {
        event.preventDefault();
        void selectAllInRoot();
      } else if (modifier && event.key.toLowerCase() === "g") {
        event.preventDefault();
        if (event.shiftKey) void ungroupSelection();
        else void groupSelection();
      } else if (!modifier) {
        const shortcut = tools.find((entry) => entry.shortcut.toLowerCase() === event.key.toLowerCase());
        if (shortcut) setTool(shortcut.id);
      }
    };
    window.addEventListener("keydown", keydown);
    return () => window.removeEventListener("keydown", keydown);
  }, [engine, response, editRoot, fail, cancelInteraction, selectionRoot]);

  const loadFixture = (fixture: string) => {
    setEditRoot(null);
    void engine.send("load_fixture", { fixture }).catch(fail);
  };
  const createFramePreset = async (width: number, height: number) => {
    if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
      fail(new EngineFailure("invalid_frame_size", "Frame width and height must be finite and positive"));
      return;
    }
    const root = documentRoot;
    if (!root) return;
    const nodeId = crypto.randomUUID();
    const center = response?.camera.center ?? [0, 0];
    const baseName = "Frame " + Math.round(width) + "×" + Math.round(height);
    const existingNames = new Set([...engine.projection.nodes.values()].map((candidate) => candidate.name));
    let name = baseName;
    let suffix = 2;
    while (existingNames.has(name)) {
      name = baseName + " " + suffix;
      suffix += 1;
    }
    try {
      await engine.send("command", {
        command: {
          kind: "create_shape",
          node_id: nodeId,
          parent_id: root,
          index: engine.projection.nodes.get(root)?.children.length ?? 0,
          shape: "frame",
          name,
          x: center[0] - width / 2,
          y: center[1] - height / 2,
          width,
          height,
        },
      });
      await engine.send("set_active_root", { node_id: nodeId });
      setTool("select");
    } catch (reason) {
      fail(reason);
    }
  };
  return (
    <main className="editor-app" aria-label="Vector Forge editor">
      <header className="app-bar">
        <div className="brand"><span className="brand-mark">V</span><strong>Vector Forge</strong><span className="phase-badge">Phase 1C</span></div>
        <div className="app-actions">
          <IconButton icon={Undo2} label="Undo (Ctrl+Z)" disabled={!response?.history.undo_depth} onClick={() => void engine.send("undo").catch(fail)} testId="undo" />
          <IconButton icon={Redo2} label="Redo (Ctrl+Y)" disabled={!response?.history.redo_depth} onClick={() => void engine.send("redo").catch(fail)} testId="redo" />
          <span className="app-divider" />
          <button className="button secondary compact" data-testid="group" onClick={() => void groupSelection()} disabled={engine.projection.selection.length < 2}><Group size={15} /> Group</button>
          <button className="button secondary compact" data-testid="ungroup" onClick={() => void ungroupSelection()} disabled={currentNode?.kind !== "group"}><Ungroup size={15} /> Ungroup</button>
        </div>
        <div className="app-actions right">
          <button className="button secondary compact" data-testid="show-components" onClick={() => setShowcase(true)}>Components</button>
          <button className="button secondary compact" data-testid="restart-worker" onClick={() => void cancelInteraction().catch(() => undefined).then(() => engine.restartWorker(true)).catch(fail)}>Restart Worker</button>
          <span className={`status-dot${ready ? " is-ready" : ""}`} aria-hidden="true" />
          <span className="status-label">{ready ? "Worker + WebGPU" : "Starting…"}</span>
        </div>
      </header>

      <div className="workspace">
        <aside className="left-column">
          <nav className="tool-rail" aria-label="Editor tools">
            {tools.map((entry) => (
              <IconButton key={entry.id} icon={entry.icon} label={`${entry.label} (${entry.shortcut})`} active={tool === entry.id} onClick={() => setTool(entry.id)} testId={`tool-${entry.id}`} />
            ))}
          </nav>
          <LayersPanel engine={engine} response={response} version={version} onError={fail} editRoot={editRoot} activeRoot={activeSlideId ?? documentRoot} setEditRoot={(id) => { setEditRoot(id); setFsm(id ? "NestedEditing" : "Idle"); }} />
        </aside>

        <section className="canvas-column" aria-label="Canvas workspace">
          <div className="canvas-toolbar">
            <span className="tool-state"><MousePointer2 size={14} /> {tool} · {fsm}</span>
            {editRoot ? <span className="nested-breadcrumb">Root / {engine.projection.nodes.get(editRoot)?.name}</span> : null}
            <div className="arrange-bar" role="group" aria-label="Align and distribute">
              {arrangeOperations.map((operation) => (
                <button
                  key={operation.id}
                  type="button"
                  className="arrange-button"
                  data-testid={operation.id.replace(/_/g, "-")}
                  aria-label={operation.label}
                  title={operation.label}
                  disabled={operation.kind === "align" ? !availability.canAlign : !availability.canDistribute}
                  onClick={() => void arrangeSelection(operation.id)}
                >
                  <span aria-hidden="true">{arrangeGlyphs[operation.id]}</span>
                </button>
              ))}
              <button
                type="button"
                className={`arrange-button snap-toggle${snapEnabled ? " is-active" : ""}`}
                data-testid="snap-toggle"
                aria-pressed={snapEnabled}
                aria-label="Snap to objects"
                title="Snap to objects (hold Alt to suspend)"
                onClick={() => setSnapEnabled((current) => !current)}
              >
                <span aria-hidden="true">⊹</span>
              </button>
            </div>
            <div className="canvas-toolbar-actions">
              <button onClick={() => void engine.send("camera", { camera: { kind: "fit" } }).catch(fail)}><Scan size={15} /> Fit document</button>
              <button disabled={!currentNode?.world_bounds} onClick={() => currentNode && void engine.send("camera", { camera: { kind: "fit_selection", node_id: currentNode.id } }).catch(fail)}><Focus size={15} /> Fit selection</button>
              <span className="zoom-readout">{Math.round((response?.camera.zoom ?? 1) * 100)}%</span>
            </div>
          </div>
          <div
            ref={shellRef}
            className={`canvas-shell tool-${tool}`}
            onPointerDown={(event) => void onCanvasPointerDown(event)}
            onPointerMove={onCanvasPointerMove}
            onPointerUp={(event) => void finishInteraction(event)}
            onPointerCancel={(event) => {
              void cancelInteraction().catch(fail);
              if (event.currentTarget.hasPointerCapture(event.pointerId)) {
                event.currentTarget.releasePointerCapture(event.pointerId);
              }
            }}
            onWheel={onWheel}
            data-testid="canvas-shell"
          >
            {tool === "frame" ? (
              <div
                className="frame-preset-popover"
                role="dialog"
                aria-label="Frame presets"
                onPointerDown={(event) => event.stopPropagation()}
              >
                <strong>Frame presets</strong>
                <button type="button" onClick={() => void createFramePreset(1920, 1080)}>1920 × 1080 · Full HD</button>
                <button type="button" onClick={() => void createFramePreset(3840, 2160)}>3840 × 2160 · 4K UHD</button>
                <button type="button" onClick={() => void createFramePreset(4096, 2160)}>4096 × 2160 · DCI 4K</button>
                <div className="frame-custom-row">
                  <label><span>W</span><input aria-label="Custom frame width" type="number" min="1" value={customFrameWidth} onChange={(event) => setCustomFrameWidth(Number(event.currentTarget.value))} /></label>
                  <label><span>H</span><input aria-label="Custom frame height" type="number" min="1" value={customFrameHeight} onChange={(event) => setCustomFrameHeight(Number(event.currentTarget.value))} /></label>
                </div>
                <button type="button" className="button primary" onClick={() => void createFramePreset(customFrameWidth, customFrameHeight)}>Create custom frame</button>
                <small>Or drag on canvas to create a custom frame.</small>
              </div>
            ) : null}            <canvas ref={canvasRef} className="webgpu-canvas" aria-label="Actual WebGPU document canvas" tabIndex={0} />
            <svg className="selection-overlay" aria-hidden="true" data-sequence={response?.engine_sequence ?? 0} data-selection-count={engine.projection.selection.length}>
              {selectionOutlines.length > 1
                ? selectionOutlines.map((entry) => (
                    <polygon key={entry.id} points={entry.points.map((point) => point.join(",")).join(" ")} className="selection-outline secondary" />
                  ))
                : null}
              {selectionUnion ? (
                <rect
                  x={selectionUnion.x}
                  y={selectionUnion.y}
                  width={selectionUnion.width}
                  height={selectionUnion.height}
                  className="selection-union"
                  data-testid="selection-union"
                />
              ) : null}
              {guideSegments.map(({ guide, segment }, index) => (
                <line
                  key={`${guide.axis}-${guide.target}-${index}`}
                  x1={segment.x1}
                  y1={segment.y1}
                  x2={segment.x2}
                  y2={segment.y2}
                  className="snap-guide"
                  data-testid={`snap-guide-${guide.axis}`}
                />
              ))}
              {marquee ? (
                <rect
                  x={marquee.x}
                  y={marquee.y}
                  width={marquee.width}
                  height={marquee.height}
                  className="marquee-rect"
                  data-testid="marquee"
                />
              ) : null}
              {transformOverlay ? (
                <g>
                  <polygon points={transformOverlay.points.map((point) => point.join(",")).join(" ")} className="selection-outline" />
                  {RESIZE_HANDLES.map((handle) => <circle key={handle} cx={transformOverlay.handles[handle][0]} cy={transformOverlay.handles[handle][1]} r="4" className="selection-handle" />)}
                  <line x1={transformOverlay.handles.n[0]} y1={transformOverlay.handles.n[1]} x2={transformOverlay.rotate[0]} y2={transformOverlay.rotate[1]} className="rotation-stem" />
                  <circle cx={transformOverlay.rotate[0]} cy={transformOverlay.rotate[1]} r="4" className="selection-handle rotate" />
                </g>
              ) : null}
            </svg>
            {transformOverlay ? (
              <>
                {RESIZE_HANDLES.map((handle) => (
                  <button
                    key={handle}
                    type="button"
                    aria-label={`Resize selection ${handle}`}
                    data-testid="resize-handle"
                    data-resize-handle={handle}
                    className={`transform-handle-hit resize-hit resize-${handle}`}
                    style={{ left: transformOverlay.handles[handle][0], top: transformOverlay.handles[handle][1] }}
                  />
                ))}
                <button type="button" aria-label="Rotate selection" data-testid="rotate-handle" className="transform-handle-hit rotate-hit" style={{ left: transformOverlay.rotate[0], top: transformOverlay.rotate[1] }} />
              </>
            ) : null}
            {!ready ? <div className="canvas-loading"><span className="spinner" /> Initializing Worker, WASM, and WebGPU…</div> : null}
          </div>
          <DebugPanel engine={engine} response={response} onLoad={loadFixture} onError={error} />
        </section>

        <aside className="right-column">
          <Inspector engine={engine} version={version} activeSlideId={activeSlideId} onError={fail} />
        </aside>
      </div>

      <footer className="status-bar">
        <span>{response?.fixture ?? "—"}</span>
        <span>{response ? `${response.metrics.document_nodes?.toLocaleString()} nodes` : "Waiting for runtime"}</span>
        <span>{response ? `GPU ${response.culling.visible?.toLocaleString()} visible` : ""}</span>
        <span className="status-spacer" />
        <span>Seq {engine.gpuMetrics.frame_sequence ?? 0}</span>
        <span>Doc {response?.revisions.document ?? 0}</span>
        <span>{currentNode ? currentNode.name : "No selection"}</span>
      </footer>
      {showcase ? <ComponentShowcase onClose={() => setShowcase(false)} /> : null}
    </main>
  );
}

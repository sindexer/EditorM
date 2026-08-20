import {
  Box,
  ArrowDown,
  ArrowUp,
  ChevronDown,
  ChevronRight,
  Circle,
  Copy,
  Code2,
  Eye,
  EyeOff,
  Focus,
  Group,
  Hand,
  Lock,
  MousePointer2,
  Pencil,
  PanelLeftClose,
  PanelRightClose,
  Plus,
  Redo2,
  RotateCw,
  Scan,
  Square,
  Trash2,
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
import type { EngineResponse, ProjectionNode, SlideSummary } from "./types";

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
  | "NestedEditing";

type Interaction = {
  pointerId: number;
  kind: "move" | "resize" | "rotate" | "pan" | "create";
  start: [number, number];
  last: [number, number];
  nodeId?: string;
  nodeKind?: "frame" | "rectangle" | "ellipse";
  matrix?: [number, number, number, number, number, number];
  geometry?: { width: number; height: number };
  created?: boolean;
  panPending?: [number, number];
};

const tools: Array<{ id: Tool; label: string; shortcut: string; icon: LucideIcon }> = [
  { id: "select", label: "Select", shortcut: "V", icon: MousePointer2 },
  { id: "hand", label: "Hand", shortcut: "H", icon: Hand },
  { id: "frame", label: "Frame", shortcut: "F", icon: Scan },
  { id: "rectangle", label: "Rectangle", shortcut: "R", icon: Square },
  { id: "ellipse", label: "Ellipse", shortcut: "O", icon: Circle },
];

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

function Splitter({
  orientation,
  value,
  min,
  max,
  direction = 1,
  label,
  onChange,
}: {
  orientation: "vertical" | "horizontal";
  value: number;
  min: number;
  max: number;
  direction?: 1 | -1;
  label: string;
  onChange: (value: number) => void;
}) {
  const drag = useRef<{ coordinate: number; value: number } | null>(null);
  const coordinate = (event: ReactPointerEvent) =>
    orientation === "vertical" ? event.clientX : event.clientY;
  const clamp = (next: number) => Math.max(min, Math.min(max, next));
  return (
    <div
      className={`workspace-splitter is-${orientation}`}
      role="separator"
      aria-label={label}
      aria-orientation={orientation}
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={Math.round(value)}
      tabIndex={0}
      onPointerDown={(event) => {
        drag.current = { coordinate: coordinate(event), value };
        event.currentTarget.setPointerCapture(event.pointerId);
      }}
      onPointerMove={(event) => {
        if (!drag.current) return;
        onChange(clamp(drag.current.value + (coordinate(event) - drag.current.coordinate) * direction));
      }}
      onPointerUp={(event) => {
        drag.current = null;
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
          event.currentTarget.releasePointerCapture(event.pointerId);
        }
      }}
      onPointerCancel={() => { drag.current = null; }}
      onKeyDown={(event) => {
        const negative = orientation === "vertical" ? "ArrowLeft" : "ArrowUp";
        const positive = orientation === "vertical" ? "ArrowRight" : "ArrowDown";
        if (event.key !== negative && event.key !== positive) return;
        event.preventDefault();
        const delta = event.key === positive ? 8 : -8;
        onChange(clamp(value + delta * direction));
      }}
    />
  );
}

function SlidesPanel({
  engine,
  response,
  onError,
  onActivate,
}: {
  engine: EngineClient;
  response: EngineResponse | null;
  onError: (error: unknown) => void;
  onActivate: (slide: string) => void;
}) {
  const slides = response?.editor_session.slides ?? [];
  const activeId = response?.editor_session.active_slide_id ?? null;
  const [renaming, setRenaming] = useState<string | null>(null);
  const [nameDraft, setNameDraft] = useState("");
  const listRef = useRef<HTMLDivElement>(null);
  const slideElements = useRef(new Map<string, HTMLElement>());
  const slideKey = slides.map((slide) => slide.id).join(":");

  useEffect(() => {
    const root = listRef.current;
    if (!root || typeof IntersectionObserver === "undefined") return;
    const observer = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        const slideId = (entry.target as HTMLElement).dataset.slideId;
        if (slideId) engine.setThumbnailVisibility(slideId, entry.isIntersecting);
      }
    }, { root, threshold: 0.05 });
    for (const element of slideElements.current.values()) observer.observe(element);
    return () => {
      observer.disconnect();
      for (const slide of slides) engine.setThumbnailVisibility(slide.id, false);
    };
  }, [engine, slideKey]);

  const sendSlide = (slide: Record<string, unknown>) =>
    engine.send("slide", { slide }).catch(onError);
  const activate = async (slide: SlideSummary) => {
    try {
      await engine.send("slide", { slide: { kind: "activate", slide_id: slide.id } });
      onActivate(slide.id);
    } catch (error) {
      onError(error);
    }
  };
  const commitRename = async (slide: SlideSummary) => {
    if (nameDraft.trim() && nameDraft.trim() !== slide.name) {
      await sendSlide({ kind: "rename", slide_id: slide.id, name: nameDraft });
    }
    setRenaming(null);
  };
  const reorder = (slide: SlideSummary, index: number) => {
    if (index < 0 || index >= slides.length || index === slide.index) return;
    void sendSlide({ kind: "reorder", slide_id: slide.id, index });
  };
  const onListKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const current = slides.findIndex((slide) => slide.id === activeId);
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    event.preventDefault();
    const next = Math.max(0, Math.min(slides.length - 1, current + (event.key === "ArrowDown" ? 1 : -1)));
    if (event.altKey && current >= 0) reorder(slides[current], next);
    else if (slides[next]) void activate(slides[next]);
  };

  return (
    <section className="panel slides-panel" aria-label="Slides panel">
      <PanelHeader
        title="Slides"
        action={
          <button
            type="button"
            className="icon-button small"
            aria-label="Add Slide"
            data-testid="add-slide"
            onClick={() =>
              void sendSlide({
                kind: "create",
                slide_id: crypto.randomUUID(),
                index: slides.length,
                width: 1920,
                height: 1080,
              })
            }
          >
            <Plus size={16} />
          </button>
        }
      />
      <div
        ref={listRef}
        className="slides-list"
        role="listbox"
        aria-label="Document Slides"
        aria-activedescendant={activeId ? `slide-${activeId}` : undefined}
        tabIndex={0}
        onKeyDown={onListKeyDown}
      >
        {slides.map((slide) => {
          const thumbnail = engine.thumbnails.get(slide.id);
          const active = slide.id === activeId;
          return (
            <article
              ref={(element) => {
                if (element) slideElements.current.set(slide.id, element);
                else slideElements.current.delete(slide.id);
              }}
              data-slide-id={slide.id}
              id={`slide-${slide.id}`}
              key={slide.id}
              className={`slide-card${active ? " is-active" : ""}`}
              role="option"
              aria-selected={active}
              aria-label={`Slide ${slide.index + 1}: ${slide.name}`}
              onClick={() => void activate(slide)}
              data-testid={`slide-card-${slide.index}`}
            >
              <span className="slide-number">{slide.index + 1}</span>
              <div className="slide-thumbnail" data-thumbnail-status={thumbnail?.status ?? "pending"}>
                {thumbnail?.status === "ready" && thumbnail.url ? (
                  <img src={thumbnail.url} alt="" draggable={false} />
                ) : thumbnail?.status === "error" ? (
                  <span className="thumbnail-error" title={thumbnail.error}>Preview error</span>
                ) : (
                  <span className="thumbnail-pending">Preview pending</span>
                )}
              </div>
              <div className="slide-meta">
                {renaming === slide.id ? (
                  <input
                    className="slide-name-input"
                    aria-label={`Rename ${slide.name}`}
                    autoFocus
                    value={nameDraft}
                    onClick={(event) => event.stopPropagation()}
                    onChange={(event) => setNameDraft(event.currentTarget.value)}
                    onBlur={() => void commitRename(slide)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") event.currentTarget.blur();
                      if (event.key === "Escape") setRenaming(null);
                    }}
                  />
                ) : (
                  <strong>{slide.name}</strong>
                )}
                <span>{Math.round(slide.width)} × {Math.round(slide.height)}</span>
              </div>
              {active ? (
                <div className="slide-actions" aria-label={`Actions for ${slide.name}`}>
                  <button
                    type="button"
                    aria-label={`Move ${slide.name} up`}
                    disabled={slide.index === 0}
                    onClick={(event) => { event.stopPropagation(); reorder(slide, slide.index - 1); }}
                  ><ArrowUp size={13} /></button>
                  <button
                    type="button"
                    aria-label={`Move ${slide.name} down`}
                    disabled={slide.index === slides.length - 1}
                    onClick={(event) => { event.stopPropagation(); reorder(slide, slide.index + 1); }}
                  ><ArrowDown size={13} /></button>
                  <button
                    type="button"
                    aria-label={`Rename ${slide.name}`}
                    onClick={(event) => {
                      event.stopPropagation();
                      setNameDraft(slide.name);
                      setRenaming(slide.id);
                    }}
                  ><Pencil size={13} /></button>
                  <button
                    type="button"
                    aria-label={`Duplicate ${slide.name}`}
                    onClick={(event) => {
                      event.stopPropagation();
                      void sendSlide({
                        kind: "duplicate",
                        source_id: slide.id,
                        slide_id: crypto.randomUUID(),
                        index: slide.index + 1,
                      });
                    }}
                  ><Copy size={13} /></button>
                  <button
                    type="button"
                    aria-label={`Delete ${slide.name}`}
                    disabled={slides.length === 1}
                    onClick={(event) => {
                      event.stopPropagation();
                      void sendSlide({ kind: "delete", slide_id: slide.id });
                    }}
                  ><Trash2 size={13} /></button>
                </div>
              ) : null}
            </article>
          );
        })}
      </div>
      <div className="panel-footnote">Alt + ↑/↓ to reorder · {slides.length} Slides</div>
    </section>
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
  setEditRoot,
}: {
  engine: EngineClient;
  response: EngineResponse | null;
  version: number;
  onError: (error: unknown) => void;
  editRoot: string | null;
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
    const root = editRoot ?? response?.editor_session.active_slide_id ?? engine.projection.rootId;
    if (!root) return result;
    const stack: Array<{ id: string; depth: number }> = [{ id: root, depth: 0 }];
    while (stack.length) {
      const current = stack.pop();
      if (!current) break;
      result.push(current);
      const node = nodes.get(current.id);
      if (!node) continue;
      const shouldExpand = current.depth === 0 || expanded.has(current.id);
      if (shouldExpand) {
        for (let index = node.children.length - 1; index >= 0; index -= 1) {
          const child = node.children.at(index);
          if (child) stack.push({ id: child, depth: current.depth + 1 });
        }
      }
    }
    engine.projection.counters.layersFlattenedNodesVisited += result.length;
    return result;
  }, [engine, engine.projection.hierarchyVersion, editRoot, expanded, largeFlatProjection]);
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
      <div className="timeline-header" aria-label="Timeline shell header">
        <strong>Timeline</strong>
        <span>Structure only · motion controls are not available in Phase 1B</span>
      </div>
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
                <span className="tree-name">{node.name}</span>
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
      <div className="timeline-viewport" aria-label="Timeline tracks" data-testid="timeline-viewport">
        <div
          className="timeline-track-surface"
          style={{ height: visibleOrder.length * rowHeight, transform: `translateY(${-scrollTop}px)` }}
        >
          {mounted.map(({ id }, mountedIndex) => {
            const node = engine.projection.nodes.get(id);
            if (!node) return null;
            const selected = engine.projection.selection.includes(id);
            const top = (start + mountedIndex) * rowHeight;
            return (
              <button
                type="button"
                key={id}
                className={`timeline-row${selected ? " is-selected" : ""}`}
                style={{ top, height: rowHeight }}
                data-node-id={id}
                aria-label={`Timeline track for ${node.name}`}
                onClick={(event) => void select(id, event.shiftKey)}
              ><span className="timeline-track-line" /><span className="timeline-track-label">Track</span></button>
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
  unit,
  disabled,
  onCommit,
}: {
  label: string;
  value: number;
  unit?: string;
  disabled?: boolean;
  onCommit: (value: number) => void;
}) {
  const [draft, setDraft] = useState(String(Number(value.toFixed(3))));
  const [validationCode, setValidationCode] = useState<string | null>(null);
  const errorId = "numeric-error-" + label.toLowerCase().replace(/[^a-z0-9]+/g, "-");
  useEffect(() => {
    setDraft(String(Number(value.toFixed(3))));
    setValidationCode(null);
  }, [value]);
  const commit = () => {
    const parsed = Number(draft);
    if (!Number.isFinite(parsed)) {
      setValidationCode("numeric_non_finite");
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
              setDraft(String(Number(value.toFixed(3))));
              setValidationCode(null);
              event.currentTarget.blur();
            }
          }}
        />
        {unit ? <span className="field-suffix">{unit}</span> : null}
      </div>
      {validationCode ? <small id={errorId} role="alert">numeric_non_finite: Enter a finite number.</small> : null}
    </label>
  );
}

function Inspector({
  engine,
  response,
  tool,
  version,
  onError,
}: {
  engine: EngineClient;
  response: EngineResponse | null;
  tool: Tool;
  version: number;
  onError: (error: unknown) => void;
}) {
  const node = engine.projection.primary ? engine.projection.nodes.get(engine.projection.primary) : undefined;
  const activeSlide = response?.editor_session.slides.find((slide) => slide.active);
  const matrix = node?.local_transform ?? [1, 0, 0, 1, 0, 0];
  const rotation = Math.atan2(matrix[2], matrix[0]) * (180 / Math.PI);
  const appearance = node?.appearance ?? DEFAULT_APPEARANCE;
  const setTransform = (next: [number, number, number, number, number, number]) => {
    if (!node) return;
    void engine.send("command", { command: { kind: "set_transform", node_id: node.id, matrix: next } }).catch(onError);
  };
  const updateRotation = (degrees: number) => {
    const radians = degrees * (Math.PI / 180);
    const sx = Math.hypot(matrix[0], matrix[2]);
    const sy = Math.hypot(matrix[1], matrix[3]);
    setTransform([Math.cos(radians) * sx, -Math.sin(radians) * sy, Math.sin(radians) * sx, Math.cos(radians) * sy, matrix[4], matrix[5]]);
  };
  const setGeometry = (width: number, height: number) => {
    if (!node || (node.kind !== "frame" && node.kind !== "rectangle" && node.kind !== "ellipse")) return;
    void engine
      .send("command", {
        command: { kind: "set_geometry", node_id: node.id, shape: node.kind, width, height },
      })
      .catch(onError);
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
        tool === "frame" || tool === "rectangle" || tool === "ellipse" ? (
          <div className="inspector-content" data-inspector-context="tool">
            <div className="context-heading">
              {tool === "ellipse" ? <Circle size={18} /> : tool === "frame" ? <Scan size={18} /> : <Square size={18} />}
              <div><strong>{shapeName(tool)} Tool</strong><span>Creation defaults</span></div>
            </div>
            <div className="field-grid">
              <NumericField label="Default W" value={tool === "frame" ? 1920 : 240} disabled onCommit={() => undefined} />
              <NumericField label="Default H" value={tool === "frame" ? 1080 : 160} disabled onCommit={() => undefined} />
            </div>
            <small className="inspector-note">Drag on the active Slide to set the final size. F creates a nested Frame.</small>
          </div>
        ) : activeSlide ? (
          <div className="inspector-content" data-inspector-context="slide">
            <div className="context-heading">
              <Box size={18} />
              <div><strong>Slide properties</strong><span>Active root Frame</span></div>
            </div>
            <label className="field field-wide">
              <span>Name</span>
              <input
                key={`${activeSlide.id}-${activeSlide.name}`}
                defaultValue={activeSlide.name}
                onBlur={(event) => {
                  if (event.currentTarget.value !== activeSlide.name) {
                    void engine.send("slide", {
                      slide: { kind: "rename", slide_id: activeSlide.id, name: event.currentTarget.value },
                    }).catch(onError);
                  }
                }}
              />
            </label>
            <div className="field-grid">
              <NumericField
                label="Width"
                value={activeSlide.width}
                unit="px"
                onCommit={(width) => void engine.send("command", {
                  command: { kind: "set_geometry", node_id: activeSlide.id, shape: "frame", width, height: activeSlide.height },
                }).catch(onError)}
              />
              <NumericField
                label="Height"
                value={activeSlide.height}
                unit="px"
                onCommit={(height) => void engine.send("command", {
                  command: { kind: "set_geometry", node_id: activeSlide.id, shape: "frame", width: activeSlide.width, height },
                }).catch(onError)}
              />
            </div>
            <div className="slide-ratio-row"><span>Aspect ratio</span><strong>{activeSlide.aspect_ratio.toFixed(3)} · 16:9</strong></div>
            <button className="button secondary" onClick={() => void engine.send("camera", { camera: { kind: "fit" } }).catch(onError)}>
              <Scan size={15} /> Fit Slide
            </button>
          </div>
        ) : (
          <div className="empty-state"><MousePointer2 size={22} /><strong>No Slide available</strong></div>
        )
      ) : (
        <div className="inspector-content">
          <label className="field field-wide">
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
          </label>
          <div className="node-summary">
            <span className="badge">{node.kind}</span>
            <span className="mono">{node.id.slice(0, 8)}</span>
          </div>
          <div className="field-grid">
            <NumericField label="X" value={matrix[4]} onCommit={(value) => setTransform([matrix[0], matrix[1], matrix[2], matrix[3], value, matrix[5]])} />
            <NumericField label="Y" value={matrix[5]} onCommit={(value) => setTransform([matrix[0], matrix[1], matrix[2], matrix[3], matrix[4], value])} />
            <NumericField label="W" value={node.geometry?.width ?? 0} disabled={!node.geometry} onCommit={(value) => setGeometry(value, node.geometry?.height ?? 1)} />
            <NumericField label="H" value={node.geometry?.height ?? 0} disabled={!node.geometry} onCommit={(value) => setGeometry(node.geometry?.width ?? 1, value)} />
            <NumericField label="Rotation" value={rotation} unit="°" onCommit={updateRotation} />
            <NumericField
              label="Opacity"
              value={node.opacity * 100}
              unit="%"
              onCommit={(value) => void engine.send("command", { command: { kind: "set_opacity", node_id: node.id, opacity: Math.max(0, Math.min(100, value)) / 100 } }).catch(onError)}
            />
          </div>
          <div className="appearance-section" aria-label="Appearance">
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
          </div>          <div className="inline-controls">
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

type WorkspacePreferences = {
  slidesWidth: number;
  inspectorWidth: number;
  bottomHeight: number;
  slidesCollapsed: boolean;
  inspectorCollapsed: boolean;
  bottomCollapsed: boolean;
};

const DEFAULT_WORKSPACE_PREFERENCES: WorkspacePreferences = {
  slidesWidth: 248,
  inspectorWidth: 288,
  bottomHeight: 248,
  slidesCollapsed: false,
  inspectorCollapsed: false,
  bottomCollapsed: false,
};

function readWorkspacePreferences(): WorkspacePreferences {
  try {
    const stored = JSON.parse(localStorage.getItem("editorm.phase1b.workspace") ?? "null") as Partial<WorkspacePreferences> | null;
    return stored ? { ...DEFAULT_WORKSPACE_PREFERENCES, ...stored } : DEFAULT_WORKSPACE_PREFERENCES;
  } catch {
    return DEFAULT_WORKSPACE_PREFERENCES;
  }
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
  const [workspacePreferences, setWorkspacePreferences] = useState(readWorkspacePreferences);
  const updateWorkspacePreferences = (change: Partial<WorkspacePreferences>) => {
    setWorkspacePreferences((current) => ({ ...current, ...change }));
  };

  useEffect(() => {
    localStorage.setItem("editorm.phase1b.workspace", JSON.stringify(workspacePreferences));
  }, [workspacePreferences]);

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
      editor_session: response?.editor_session ?? null,
      camera: response?.camera ?? null,
      projection_nodes: engine.projection.nodes.size,
      projection_schema_version: response?.projection.schema_version ?? null,
      primary_node: proofProjectionNode(engine.projection.primary ? engine.projection.nodes.get(engine.projection.primary) : undefined),
      render_delta: response?.render_delta ?? null,
      render_binary_schema_version: response?.render_binary_schema_version ?? null,
      resources: response?.resources ?? null,
      binary: response?.binary ?? null,
      workspace: workspacePreferences,
      interaction_active: interaction.current ? { pointer_id: interaction.current.pointerId, kind: interaction.current.kind } : null,
      interaction_queue: {
        generation: dragQueue.current.generation,
        in_flight: dragQueue.current.inFlight,
        scheduled: Boolean(dragQueue.current.scheduled),
        latest: Boolean(dragQueue.current.latest),
      },
    });
  }, [engine, response, version, heartbeatTick, fsm, tool, editRoot, error, workspacePreferences]);

  const currentNode = engine.projection.primary ? engine.projection.nodes.get(engine.projection.primary) : undefined;
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
    const queue = dragQueue.current;
    queue.generation += 1;
    queue.scheduled = null;
    queue.latest = null;
    while (queue.inFlight) await new Promise((resolve) => setTimeout(resolve, 4));
    if (active && active.kind !== "pan") await engine.send("rollback_transaction");
    setFsm(editRoot ? "NestedEditing" : "Idle");
  }, [engine, editRoot]);

  const pointerPosition = (event: ReactPointerEvent): [number, number] => {
    const rect = event.currentTarget.getBoundingClientRect();
    return [event.clientX - rect.left, event.clientY - rect.top];
  };

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
      matrix: [...node.local_transform],
    };
    captureTarget.setPointerCapture(pointerId);
    setFsm("Moving");
  };

  const beginHandleAt = async (
    pointerId: number,
    clientX: number,
    clientY: number,
    kind: "resize" | "rotate",
  ) => {
    if (!currentNode || currentNode.locked || !currentNode.geometry) return;
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) return;
    const point: [number, number] = [clientX - rect.left, clientY - rect.top];
    await engine.send("begin_transaction");
    interaction.current = {
      pointerId,
      kind,
      start: point,
      last: point,
      nodeId: currentNode.id,
      nodeKind: currentNode.kind === "frame" ? "frame" : currentNode.kind === "ellipse" ? "ellipse" : "rectangle",
      matrix: [...currentNode.local_transform],
      geometry: { ...currentNode.geometry },
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
      if (handleKind === "resize-handle" || handleKind === "rotate-handle") {
        await beginHandleAt(pointerId, event.clientX, event.clientY, handleKind === "resize-handle" ? "resize" : "rotate");
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
      const hit = await engine.send("hit_test", { x: point[0], y: point[1] });
      const target = (hit.result?.topmost as string | null | undefined) ?? null;
      if (!target) {
        await engine.send("selection", { target: null, mode: "clear" });
        setFsm("Idle");
        return;
      }
      await engine.send("selection", { target, mode: event.shiftKey ? "toggle" : "replace" });
      const node = engine.projection.nodes.get(target);
      if (node && !event.shiftKey) await beginMove(pointerId, node, point, captureTarget);
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
    if (active.kind === "move" && active.matrix && active.nodeId) {
      const zoom = response?.camera.zoom ?? 1;
      const dx = (point[0] - active.start[0]) / zoom;
      const dy = (point[1] - active.start[1]) / zoom;
      const next: [number, number, number, number, number, number] = [
        active.matrix[0], active.matrix[1], active.matrix[2], active.matrix[3], active.matrix[4] + dx, active.matrix[5] + dy,
      ];
      scheduleDrag(() => engine.send("update_transaction", { command: { kind: "set_transform", node_id: active.nodeId, matrix: next } }).then(() => undefined));
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
      const root = editRoot ?? response?.editor_session.active_slide_id ?? engine.projection.rootId;
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
    if (active.kind === "resize" && active.nodeId && active.geometry) {
      const zoom = response?.camera.zoom ?? 1;
      const width = Math.max(1, active.geometry.width + (point[0] - active.start[0]) / zoom);
      const height = Math.max(1, active.geometry.height + (point[1] - active.start[1]) / zoom);
      scheduleDrag(() => engine.send("update_transaction", { command: { kind: "set_geometry", node_id: active.nodeId, shape: active.nodeKind, width, height } }).then(() => undefined));
      return;
    }
    if (active.kind === "rotate" && active.nodeId && active.matrix && currentNode?.world_bounds) {
      const centerWorld: [number, number] = [
        (currentNode.world_bounds.min[0] + currentNode.world_bounds.max[0]) / 2,
        (currentNode.world_bounds.min[1] + currentNode.world_bounds.max[1]) / 2,
      ];
      const center = worldToViewport(centerWorld);
      const angle = Math.atan2(point[1] - center[1], point[0] - center[0]) + Math.PI / 2;
      const sx = Math.hypot(active.matrix[0], active.matrix[2]);
      const sy = Math.hypot(active.matrix[1], active.matrix[3]);
      const next: [number, number, number, number, number, number] = [Math.cos(angle) * sx, -Math.sin(angle) * sy, Math.sin(angle) * sx, Math.cos(angle) * sy, active.matrix[4], active.matrix[5]];
      scheduleDrag(() => engine.send("update_transaction", { command: { kind: "set_transform", node_id: active.nodeId, matrix: next } }).then(() => undefined));
    }
  };

  const finishInteraction = async (event: ReactPointerEvent<HTMLDivElement>) => {
    const active = interaction.current;
    if (!active || active.pointerId !== event.pointerId) return;
    interaction.current = null;
    try {
      while (dragQueue.current.inFlight) await new Promise((resolve) => setTimeout(resolve, 4));
      if (active.kind !== "pan") {
        if (active.kind === "create" && !active.created && active.nodeId && active.nodeKind) {
          const start = viewportToWorld(active.start);
          const root = editRoot ?? response?.editor_session.active_slide_id ?? engine.projection.rootId;
          if (root) {
            await engine.send("update_transaction", { command: { kind: "create_shape", node_id: active.nodeId, parent_id: root, index: engine.projection.nodes.get(root)?.children.length ?? 0, shape: active.nodeKind, name: shapeName(active.nodeKind!), x: start[0], y: start[1], width: 24, height: 24 } });
          }
        }
        await engine.send("commit_transaction");
        if (active.kind === "create" && active.nodeId) await engine.send("selection", { target: active.nodeId, mode: "replace" });
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
      } else if (!modifier) {
        const shortcut = tools.find((entry) => entry.shortcut.toLowerCase() === event.key.toLowerCase());
        if (shortcut) setTool(shortcut.id);
      }
    };
    window.addEventListener("keydown", keydown);
    return () => window.removeEventListener("keydown", keydown);
  }, [engine, response, editRoot, fail, cancelInteraction]);

  const overlay = useMemo(() => {
    if (!currentNode?.geometry || !currentNode.world_transform || !response) return null;
    const corners: Array<[number, number]> = [[0, 0], [currentNode.geometry.width, 0], [currentNode.geometry.width, currentNode.geometry.height], [0, currentNode.geometry.height]];
    const points = corners.map((point) => worldToViewport(transformAffinePoint(currentNode.world_transform!, point)));
    const handle = points[2];
    const topMid: [number, number] = [(points[0][0] + points[1][0]) / 2, (points[0][1] + points[1][1]) / 2];
    return { points, handle, rotate: [topMid[0], topMid[1] - 24] as [number, number] };
  }, [currentNode, response, worldToViewport]);

  const loadFixture = (fixture: string) => {
    setEditRoot(null);
    void engine.send("load_fixture", { fixture }).catch(fail);
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
    const first = node.children.at(0);
    try {
      await engine.send("command", { command: { kind: "ungroup", node_id: node.id } });
      if (first) await engine.send("selection", { target: first, mode: "replace" });
    } catch (reason) { fail(reason); }
  };

  const createFramePreset = async (width: number, height: number) => {
    if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
      fail(new EngineFailure("invalid_frame_size", "Frame width and height must be finite and positive"));
      return;
    }
    const root = editRoot ?? response?.editor_session.active_slide_id ?? engine.projection.rootId;
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
      await engine.send("selection", { target: nodeId, mode: "replace" });
      setTool("select");
    } catch (reason) {
      fail(reason);
    }
  };
  return (
    <main
      className="editor-app"
      aria-label="EditorM motion graphics and presentation editor"
      style={{ "--bottom-panel-height": `${workspacePreferences.bottomHeight}px` } as CSSProperties}
    >
      <header className="app-bar">
        <div className="brand"><span className="brand-mark">M</span><strong>EditorM</strong><span className="phase-badge">Phase 1B</span></div>
        <nav className="main-menu" aria-label="Application menu">
          {['File', 'Edit', 'View', 'Insert', 'Arrange'].map((label) => <button key={label} type="button">{label}</button>)}
        </nav>
        <div className="app-actions right">
          <button className="button secondary compact" data-testid="show-components" onClick={() => setShowcase(true)}>Components</button>
          <button className="button secondary compact" data-testid="restart-worker" onClick={() => void cancelInteraction().catch(() => undefined).then(() => engine.restartWorker(true)).catch(fail)}>Restart Worker</button>
          <span className={`status-dot${ready ? " is-ready" : ""}`} aria-hidden="true" />
          <span className="status-label">{ready ? "Worker + WebGPU" : "Starting…"}</span>
        </div>
      </header>

      <nav className="tools-bar" aria-label="Editor tools">
        <div className="tool-group">
          {tools.map((entry) => (
            <IconButton key={entry.id} icon={entry.icon} label={`${entry.label} (${entry.shortcut})`} active={tool === entry.id} onClick={() => setTool(entry.id)} testId={`tool-${entry.id}`} />
          ))}
        </div>
        <span className="app-divider" />
        <div className="tool-group">
          <IconButton icon={Undo2} label="Undo (Ctrl+Z)" disabled={!response?.history.undo_depth} onClick={() => void engine.send("undo").catch(fail)} testId="undo" />
          <IconButton icon={Redo2} label="Redo (Ctrl+Y)" disabled={!response?.history.redo_depth} onClick={() => void engine.send("redo").catch(fail)} testId="redo" />
          <button className="button secondary compact" data-testid="group" onClick={() => void groupSelection()} disabled={engine.projection.selection.length < 2}><Group size={15} /> Group</button>
          <button className="button secondary compact" data-testid="ungroup" onClick={() => void ungroupSelection()} disabled={currentNode?.kind !== "group"}><Ungroup size={15} /> Ungroup</button>
        </div>
        <span className="tools-spacer" />
        <div className="tool-group workspace-controls">
          <IconButton icon={PanelLeftClose} label={`${workspacePreferences.slidesCollapsed ? "Show" : "Hide"} Slides panel`} active={!workspacePreferences.slidesCollapsed} onClick={() => updateWorkspacePreferences({ slidesCollapsed: !workspacePreferences.slidesCollapsed })} testId="toggle-slides" />
          <button className={`button secondary compact${workspacePreferences.bottomCollapsed ? "" : " is-active"}`} onClick={() => updateWorkspacePreferences({ bottomCollapsed: !workspacePreferences.bottomCollapsed })} data-testid="toggle-timeline">Layers + Timeline</button>
          <IconButton icon={PanelRightClose} label={`${workspacePreferences.inspectorCollapsed ? "Show" : "Hide"} Inspector`} active={!workspacePreferences.inspectorCollapsed} onClick={() => updateWorkspacePreferences({ inspectorCollapsed: !workspacePreferences.inspectorCollapsed })} testId="toggle-inspector" />
        </div>
      </nav>

      <div
        className="workspace phase1b-workspace"
        style={{ gridTemplateColumns: `${workspacePreferences.slidesCollapsed ? 0 : workspacePreferences.slidesWidth}px ${workspacePreferences.slidesCollapsed ? 0 : 5}px minmax(420px, 1fr) ${workspacePreferences.inspectorCollapsed ? 0 : 5}px ${workspacePreferences.inspectorCollapsed ? 0 : workspacePreferences.inspectorWidth}px` }}
      >
        {!workspacePreferences.slidesCollapsed ? (
          <aside className="left-column">
            <SlidesPanel engine={engine} response={response} onError={fail} onActivate={() => { setEditRoot(null); setFsm("Idle"); }} />
          </aside>
        ) : null}
        {!workspacePreferences.slidesCollapsed ? <Splitter orientation="vertical" value={workspacePreferences.slidesWidth} min={190} max={420} label="Resize Slides panel" onChange={(slidesWidth) => updateWorkspacePreferences({ slidesWidth })} /> : null}

        <section className="canvas-column" aria-label="Canvas workspace">
          <div className="canvas-toolbar">
            <span className="tool-state"><MousePointer2 size={14} /> Slide {Math.max(1, (response?.editor_session.slides.findIndex((slide) => slide.active) ?? 0) + 1)} · {tool} · {fsm}</span>
            {editRoot ? <span className="nested-breadcrumb">Root / {engine.projection.nodes.get(editRoot)?.name}</span> : null}
            <div className="canvas-toolbar-actions">
              <button onClick={() => void engine.send("camera", { camera: { kind: "fit" } }).catch(fail)}><Scan size={15} /> Fit Slide</button>
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
            <svg className="selection-overlay" aria-hidden="true" data-sequence={response?.engine_sequence ?? 0}>
              {overlay ? (
                <g>
                  <polygon points={overlay.points.map((point) => point.join(",")).join(" ")} className="selection-outline" />
                  {overlay.points.map((point, index) => <circle key={index} cx={point[0]} cy={point[1]} r="4" className="selection-handle" />)}
                  <line x1={(overlay.points[0][0] + overlay.points[1][0]) / 2} y1={(overlay.points[0][1] + overlay.points[1][1]) / 2} x2={overlay.rotate[0]} y2={overlay.rotate[1]} className="rotation-stem" />
                  <circle cx={overlay.rotate[0]} cy={overlay.rotate[1]} r="4" className="selection-handle rotate" />
                </g>
              ) : null}
            </svg>
            {overlay ? (
              <>
                <button type="button" aria-label="Resize selection" data-testid="resize-handle" className="transform-handle-hit resize-hit" style={{ left: overlay.handle[0], top: overlay.handle[1] }} />
                <button type="button" aria-label="Rotate selection" data-testid="rotate-handle" className="transform-handle-hit rotate-hit" style={{ left: overlay.rotate[0], top: overlay.rotate[1] }} />
              </>
            ) : null}
            {!ready ? <div className="canvas-loading"><span className="spinner" /> Initializing Worker, WASM, and WebGPU…</div> : null}
          </div>
          {!workspacePreferences.bottomCollapsed ? <Splitter orientation="horizontal" value={workspacePreferences.bottomHeight} min={160} max={440} direction={-1} label="Resize Layers and Timeline panel" onChange={(bottomHeight) => updateWorkspacePreferences({ bottomHeight })} /> : null}
          {!workspacePreferences.bottomCollapsed ? (
            <div className="bottom-panel" style={{ height: workspacePreferences.bottomHeight }}>
              <LayersPanel engine={engine} response={response} version={version} onError={fail} editRoot={editRoot} setEditRoot={(id) => { setEditRoot(id); setFsm(id ? "NestedEditing" : "Idle"); }} />
            </div>
          ) : null}
          <DebugPanel engine={engine} response={response} onLoad={loadFixture} onError={error} />
        </section>

        {!workspacePreferences.inspectorCollapsed ? <Splitter orientation="vertical" value={workspacePreferences.inspectorWidth} min={240} max={440} direction={-1} label="Resize Inspector" onChange={(inspectorWidth) => updateWorkspacePreferences({ inspectorWidth })} /> : null}
        {!workspacePreferences.inspectorCollapsed ? (
          <aside className="right-column">
            <Inspector engine={engine} response={response} tool={tool} version={version} onError={fail} />
          </aside>
        ) : null}
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

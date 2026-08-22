// Pure geometry helpers for multiple selection, rubber-band selection, and snap guides.
// They contain no engine state and no React state, so both the editor and its tests can use
// them without a Worker, WASM, or WebGPU.

export interface ViewportRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface WorldBounds {
  min: [number, number];
  max: [number, number];
}

export interface SnapGuideProjection {
  axis: "vertical" | "horizontal";
  position: number;
  start: number;
  end: number;
  target: string;
}

export interface GuideSegment {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

/** Smallest pointer travel, in viewport pixels, that counts as a rubber band instead of a click. */
export const MARQUEE_THRESHOLD_PX = 3;

/** Snap radius in viewport pixels, matching the engine's default. */
export const SNAP_THRESHOLD_PX = 8;

export function marqueeRect(
  start: [number, number],
  current: [number, number],
): ViewportRect {
  return {
    x: Math.min(start[0], current[0]),
    y: Math.min(start[1], current[1]),
    width: Math.abs(current[0] - start[0]),
    height: Math.abs(current[1] - start[1]),
  };
}

/** A rubber band only becomes a selection request once the pointer has actually travelled. */
export function isMarqueeDrag(
  start: [number, number],
  current: [number, number],
  threshold = MARQUEE_THRESHOLD_PX,
): boolean {
  return (
    Math.abs(current[0] - start[0]) >= threshold ||
    Math.abs(current[1] - start[1]) >= threshold
  );
}

/** Union of the world bounds of every selected node, or null when nothing has bounds yet. */
export function unionWorldBounds(
  boundsList: Array<WorldBounds | null | undefined>,
): WorldBounds | null {
  let union: WorldBounds | null = null;
  for (const bounds of boundsList) {
    if (!bounds) continue;
    if (!union) {
      union = { min: [bounds.min[0], bounds.min[1]], max: [bounds.max[0], bounds.max[1]] };
      continue;
    }
    union.min[0] = Math.min(union.min[0], bounds.min[0]);
    union.min[1] = Math.min(union.min[1], bounds.min[1]);
    union.max[0] = Math.max(union.max[0], bounds.max[0]);
    union.max[1] = Math.max(union.max[1], bounds.max[1]);
  }
  return union;
}

/** Projects one engine snap guide into the viewport segment the overlay draws. */
export function guideSegment(
  guide: SnapGuideProjection,
  worldToViewport: (point: [number, number]) => [number, number],
): GuideSegment {
  if (guide.axis === "vertical") {
    const [x1, y1] = worldToViewport([guide.position, guide.start]);
    const [x2, y2] = worldToViewport([guide.position, guide.end]);
    return { x1, y1, x2, y2 };
  }
  const [x1, y1] = worldToViewport([guide.start, guide.position]);
  const [x2, y2] = worldToViewport([guide.end, guide.position]);
  return { x1, y1, x2, y2 };
}

/** Which arrange operations the current selection size supports. */
export function arrangeAvailability(selectionCount: number): {
  canAlign: boolean;
  canDistribute: boolean;
} {
  return { canAlign: selectionCount >= 2, canDistribute: selectionCount >= 3 };
}

/**
 * Decides what a pointer press does to the selection before any drag starts.
 *
 * Plain press on an unselected node replaces the selection; plain press on an already selected
 * node keeps the whole selection so a person can drag several objects at once; a modified press
 * toggles one node without disturbing the rest.
 */
export function selectionIntent(
  target: string | null,
  selection: readonly string[],
  additive: boolean,
): { mode: "replace" | "toggle" | "keep" | "marquee"; target: string | null } {
  if (!target) return { mode: "marquee", target: null };
  if (additive) return { mode: "toggle", target };
  if (selection.includes(target)) return { mode: "keep", target };
  return { mode: "replace", target };
}

/**
 * Whether pressing on `kind` should begin a rubber band instead of a move.
 *
 * Empty canvas always starts a band. A press inside a Frame's own area starts one too, because
 * a Frame usually is the background a person drags across; releasing without travelling still
 * selects that Frame. A node that is already selected is always dragged, never banded.
 */
export function pressStartsMarquee(kind: string | undefined | null, isSelected: boolean): boolean {
  if (!kind) return true;
  if (isSelected) return false;
  return kind === "frame" || kind === "document";
}

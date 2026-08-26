export type Point = [number, number];

export type PenAnchor = {
  id: string;
  position: Point;
  drag?: Point;
};

export type PenDraft = {
  nodeId: string;
  parentId: string;
  origin: Point;
  anchors: PenAnchor[];
  created: boolean;
};

const subtract = (point: Point, origin: Point): Point => [point[0] - origin[0], point[1] - origin[1]];

export const appendPenAnchor = (draft: PenDraft, world: Point, id: string): PenDraft => ({
  ...draft,
  anchors: [...draft.anchors, { id, position: subtract(world, draft.origin) }],
});

export const dragPenAnchor = (draft: PenDraft, index: number, world: Point): PenDraft => {
  const anchor = draft.anchors[index];
  if (!anchor) return draft;
  const drag = subtract(world, draft.origin);
  const anchors = draft.anchors.map((entry, anchorIndex) => anchorIndex === index ? {
    ...entry,
    drag,
  } : entry);
  return { ...draft, anchors };
};

export const isCloseTarget = (
  draft: PenDraft,
  world: Point,
  zoom: number,
  thresholdPixels = 10,
): boolean => {
  if (draft.anchors.length < 3 || !Number.isFinite(zoom) || zoom <= 0) return false;
  const first = draft.anchors[0].position;
  const local = subtract(world, draft.origin);
  return Math.hypot(local[0] - first[0], local[1] - first[1]) * zoom <= thresholdPixels;
};


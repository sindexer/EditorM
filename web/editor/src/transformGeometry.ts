export type Point = [number, number];
export type Matrix = [number, number, number, number, number, number];
export type ResizeHandle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";

export interface TransformBox {
  center: Point;
  origin: Point;
  axisX: Point;
  axisY: Point;
  width: number;
  height: number;
  points: Point[];
}

const HANDLE_COORDINATES: Record<ResizeHandle, Point> = {
  nw: [0, 0], n: [0.5, 0], ne: [1, 0], e: [1, 0.5],
  se: [1, 1], s: [0.5, 1], sw: [0, 1], w: [0, 0.5],
};

export const RESIZE_HANDLES = Object.keys(HANDLE_COORDINATES) as ResizeHandle[];

export function multiply(left: Matrix, right: Matrix): Matrix {
  return [
    left[0] * right[0] + left[1] * right[2],
    left[0] * right[1] + left[1] * right[3],
    left[2] * right[0] + left[3] * right[2],
    left[2] * right[1] + left[3] * right[3],
    left[0] * right[4] + left[1] * right[5] + left[4],
    left[2] * right[4] + left[3] * right[5] + left[5],
  ];
}

export function around(center: Point, linear: Matrix): Matrix {
  return multiply(
    [1, 0, 0, 1, center[0], center[1]],
    multiply(linear, [1, 0, 0, 1, -center[0], -center[1]]),
  );
}

export function rotationAround(center: Point, radians: number): Matrix {
  const cos = Math.cos(radians);
  const sin = Math.sin(radians);
  return around(center, [cos, -sin, sin, cos, 0, 0]);
}

export function normalizedAngleDelta(radians: number): number {
  return Math.atan2(Math.sin(radians), Math.cos(radians));
}

export function snappedRotationDelta(radians: number, enabled: boolean): number {
  const normalized = normalizedAngleDelta(radians);
  const step = Math.PI / 12;
  return enabled ? Math.round(normalized / step) * step : normalized;
}

export function orientedBox(points: [Point, Point, Point, Point]): TransformBox | null {
  const [nw, ne, se, sw] = points;
  const width = Math.hypot(ne[0] - nw[0], ne[1] - nw[1]);
  const height = Math.hypot(sw[0] - nw[0], sw[1] - nw[1]);
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return null;
  const axisX: Point = [(ne[0] - nw[0]) / width, (ne[1] - nw[1]) / width];
  const axisY: Point = [(sw[0] - nw[0]) / height, (sw[1] - nw[1]) / height];
  const center: Point = [(nw[0] + se[0]) / 2, (nw[1] + se[1]) / 2];
  return { center, origin: nw, axisX, axisY, width, height, points: [nw, ne, se, sw] };
}

export function axisAlignedBox(min: Point, max: Point): TransformBox | null {
  return orientedBox([min, [max[0], min[1]], max, [min[0], max[1]]]);
}

export function handlePoint(box: TransformBox, handle: ResizeHandle): Point {
  const [x, y] = HANDLE_COORDINATES[handle];
  return [
    box.origin[0] + box.axisX[0] * box.width * x + box.axisY[0] * box.height * y,
    box.origin[1] + box.axisX[1] * box.width * x + box.axisY[1] * box.height * y,
  ];
}

function projection(point: Point, origin: Point, axis: Point): number {
  return (point[0] - origin[0]) * axis[0] + (point[1] - origin[1]) * axis[1];
}

/** Builds a non-mirroring world transform for an oriented or aggregate selection box. */
export function resizeTransform(
  box: TransformBox,
  handle: ResizeHandle,
  pointer: Point,
  preserveRatio: boolean,
  fromCenter: boolean,
  minimumSize = 1,
): Matrix | null {
  const coordinate = HANDLE_COORDINATES[handle];
  const affectsX = coordinate[0] !== 0.5;
  const affectsY = coordinate[1] !== 0.5;
  const anchorCoordinate: Point = fromCenter
    ? [0.5, 0.5]
    : [1 - coordinate[0], 1 - coordinate[1]];
  const anchor = [
    box.origin[0] + box.axisX[0] * box.width * anchorCoordinate[0] + box.axisY[0] * box.height * anchorCoordinate[1],
    box.origin[1] + box.axisX[1] * box.width * anchorCoordinate[0] + box.axisY[1] * box.height * anchorCoordinate[1],
  ] as Point;
  const start = handlePoint(box, handle);
  const baseX = projection(start, anchor, box.axisX);
  const baseY = projection(start, anchor, box.axisY);
  let scaleX = affectsX ? projection(pointer, anchor, box.axisX) / baseX : 1;
  let scaleY = affectsY ? projection(pointer, anchor, box.axisY) / baseY : 1;
  if (preserveRatio) {
    const uniform = affectsX && affectsY
      ? (Math.abs(scaleX - 1) >= Math.abs(scaleY - 1) ? scaleX : scaleY)
      : (affectsX ? scaleX : scaleY);
    scaleX = uniform;
    scaleY = uniform;
  }
  if (![scaleX, scaleY].every(Number.isFinite) || scaleX <= 0 || scaleY <= 0) return null;
  if (box.width * scaleX < minimumSize || box.height * scaleY < minimumSize) return null;
  const basis: Matrix = [box.axisX[0], box.axisY[0], box.axisX[1], box.axisY[1], 0, 0];
  const inverseBasis: Matrix = [box.axisX[0], box.axisX[1], box.axisY[0], box.axisY[1], 0, 0];
  return around(anchor, multiply(basis, multiply([scaleX, 0, 0, scaleY, 0, 0], inverseBasis)));
}

export type AffineMatrix = [number, number, number, number, number, number];
export type Point2 = [number, number];

export function transformAffinePoint(matrix: AffineMatrix, point: Point2): Point2 {
  const [m11, m12, m21, m22, tx, ty] = matrix;
  const [x, y] = point;
  return [m11 * x + m12 * y + tx, m21 * x + m22 * y + ty];
}

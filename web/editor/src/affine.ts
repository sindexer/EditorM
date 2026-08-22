export type AffineMatrix = [number, number, number, number, number, number];
export type Point2 = [number, number];

export function transformAffinePoint(matrix: AffineMatrix, point: Point2): Point2 {
  const [m11, m12, m21, m22, tx, ty] = matrix;
  const [x, y] = point;
  return [m11 * x + m12 * y + tx, m21 * x + m22 * y + ty];
}

export function inverseTransformAffinePoint(matrix: AffineMatrix, point: Point2): Point2 | null {
  const [m11, m12, m21, m22, tx, ty] = matrix;
  const determinant = m11 * m22 - m12 * m21;
  if (!Number.isFinite(determinant) || Math.abs(determinant) <= Number.EPSILON * 16) return null;
  const x = point[0] - tx;
  const y = point[1] - ty;
  const local: Point2 = [
    (m22 * x - m12 * y) / determinant,
    (-m21 * x + m11 * y) / determinant,
  ];
  return local.every(Number.isFinite) ? local : null;
}

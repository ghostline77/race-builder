import type { Point, Road } from "./road-levels";

/** Iterative Douglas–Peucker; heights and endpoints survive simplification. */
export function simplifyRoad(road: Road, tolerance: number): Road {
  const points = road.points;
  if (points.length <= 2) return road;
  const keep = new Uint8Array(points.length);
  keep[0] = 1;
  keep[points.length - 1] = 1;
  const stack: [number, number][] = [[0, points.length - 1]];
  const distance = (p: Point, a: Point, b: Point) => {
    const dx = b.x - a.x,
      dy = b.y - a.y,
      dz = b.z - a.z,
      length = dx * dx + dy * dy + dz * dz;
    const t = length
      ? Math.max(
          0,
          Math.min(
            1,
            ((p.x - a.x) * dx + (p.y - a.y) * dy + (p.z - a.z) * dz) / length,
          ),
        )
      : 0;
    return (
      (p.x - a.x - dx * t) ** 2 +
      (p.y - a.y - dy * t) ** 2 +
      (p.z - a.z - dz * t) ** 2
    );
  };
  while (stack.length) {
    const [a, b] = stack.pop()!;
    let furthest = -1,
      max = tolerance * tolerance;
    for (let i = a + 1; i < b; i++) {
      const d = distance(points[i]!, points[a]!, points[b]!);
      if (d > max) {
        max = d;
        furthest = i;
      }
    }
    if (furthest >= 0) {
      keep[furthest] = 1;
      stack.push([a, furthest], [furthest, b]);
    }
  }
  return { ...road, points: points.filter((_, i) => keep[i]) };
}

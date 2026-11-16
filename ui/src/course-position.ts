import type { Point } from "./road-levels";
/** Project a map click onto the authored course, retaining its native height. */
export function coursePosition(
  click: Point,
  points: Point[],
  looped: boolean,
  height: number | null = null,
  stationHint: number | null = null,
): { position: Point; station: number } {
  if (points.length < 2)
    throw Error("Draw a course before adding speed changes.");
  let best: { position: Point; station: number } | null = null,
    score = Infinity,
    station = 0;
  const course = looped ? [...points, points[0]!] : points;
  const candidates: { position: Point; station: number; distance: number }[] = [];
  for (let i = 1; i < course.length; i++) {
    const a = course[i - 1]!,
      b = course[i]!,
      dx = b.x - a.x,
      dy = b.y - a.y,
      dz = b.z - a.z;
    const length = Math.hypot(dx, dy, dz),
      planar = dx * dx + dy * dy;
    const t = planar
      ? Math.max(
          0,
          Math.min(1, ((click.x - a.x) * dx + (click.y - a.y) * dy) / planar),
        )
      : 0;
    const position = { x: a.x + dx * t, y: a.y + dy * t, z: a.z + dz * t };
    const distance = Math.sqrt(
      (position.x - click.x) ** 2 +
      (position.y - click.y) ** 2 +
      (height === null ? 0 : (position.z - height) ** 2));
    const candidate = station + length * t;
    if (stationHint !== null) candidates.push({position, station: candidate, distance});
    if (distance < score - 0.0001 || (Math.abs(distance - score) <= 0.0001 && stationHint !== null &&
      (!best || Math.abs(candidate - stationHint) < Math.abs(best.station - stationHint)))) {
      score = distance;
      best = { position, station: candidate };
    }
    station += length;
  }
  if (stationHint !== null) {
    const hinted = candidates.filter(c => c.distance <= score + 0.5)
      .sort((a, b) => Math.abs(a.station - stationHint) - Math.abs(b.station - stationHint))[0];
    if (hinted) return {position: hinted.position, station: hinted.station};
  }
  return best!;
}

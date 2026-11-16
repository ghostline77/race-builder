// Geometry for the map's elevation layers. Heights stay in native world metres.
export interface Point {
  x: number;
  y: number;
  z: number;
}
export interface Road {
  id: string;
  corridor?: string;
  points: Point[];
  bounds: number[];
}
export interface Segment {
  a: Point;
  b: Point;
  road: Road;
}
export interface Crossing {
  x: number;
  y: number;
  upper: Segment;
  lower: Segment;
  upperZ: number;
  lowerZ: number;
}
export interface Bridge extends Crossing {
  upperGroup: string;
  lowerGroup: string;
  hits: Crossing[];
  direction: { x: number; y: number };
  halfLength: number;
  halfWidth: number;
}
export interface Camera {
  x: number;
  y: number;
  scale: number;
}
export interface MapSize {
  w: number;
  h: number;
}
export interface RoadScene {
  segments: Segment[];
  bridges: Bridge[];
  minimum: number;
  maximum: number;
}

export function clipHeight(
  segment: Segment,
  height: number | null,
  tolerance = 2,
): Segment | null {
  if (height === null) return segment;
  const dz = segment.b.z - segment.a.z;
  if (Math.abs(dz) < 1e-8)
    return Math.abs(segment.a.z - height) <= tolerance ? segment : null;
  const limits = [
    (height - tolerance - segment.a.z) / dz,
    (height + tolerance - segment.a.z) / dz,
  ].sort((a, b) => a - b);
  const lo = Math.max(0, limits[0]!),
    hi = Math.min(1, limits[1]!);
  if (lo > hi) return null;
  const at = (t: number): Point => ({
    x: segment.a.x + t * (segment.b.x - segment.a.x),
    y: segment.a.y + t * (segment.b.y - segment.a.y),
    z: segment.a.z + t * dz,
  });
  return { ...segment, a: at(lo), b: at(hi) };
}

export function elevationColor(
  z: number,
  minimum: number,
  maximum: number,
): string {
  const t = Math.max(
    0,
    Math.min(1, (z - minimum) / Math.max(1, maximum - minimum)),
  );
  const stops = [
      [96, 155, 204],
      [157, 170, 183],
      [216, 174, 106],
    ],
    part = t < 0.5 ? 0 : 1,
    u = t < 0.5 ? t * 2 : (t - 0.5) * 2;
  return `rgb(${stops[part]!.map((v, i) => Math.round(v + (stops[part + 1]![i]! - v) * u)).join(",")})`;
}

import { test } from "node:test";
import assert from "node:assert/strict";
import { clipHeight } from "../src/road-levels";
import { simplifyRoad } from "../src/road-lod";
import { coursePosition } from "../src/course-position";
import type { Segment } from "../src/road-levels";
import { RoadIndex } from "../src/road-index";
const segment = (
  a: [number, number, number],
  b: [number, number, number],
  id: string,
): Segment => ({
  a: { x: a[0], y: a[1], z: a[2] },
  b: { x: b[0], y: b[1], z: b[2] },
  road: { id, corridor: id, points: [], bounds: [] },
});
test("speed marks project onto the existing course and retain its level", () => {
  const course = [
    { x: 0, y: 0, z: 10 },
    { x: 100, y: 0, z: 20 },
    { x: 100, y: 100, z: 20 },
  ];
  const mark = coursePosition({ x: 40, y: 6, z: 0 }, course, false);
  assert.deepEqual(mark.position, { x: 40, y: 0, z: 14 });
  assert.ok(Math.abs(mark.station - Math.hypot(100, 10) * 0.4) < 1e-8);
  const crossing = [
    { x: -10, y: 0, z: 0 },
    { x: 10, y: 0, z: 0 },
    { x: 0, y: -10, z: 10 },
    { x: 0, y: 10, z: 10 },
  ];
  assert.equal(
    coursePosition({ x: 0, y: 0, z: 0 }, crossing, false, 10).position.z,
    10,
  );
});
test("height focus clips sloping segments instead of showing the entire ramp", () => {
  const ramp = segment([0, 0, -10], [100, 0, 10], "ramp"),
    clipped = clipHeight(ramp, 0);
  assert.equal(clipped.a.x, 40);
  assert.equal(clipped.b.x, 60);
  assert.equal(clipped.a.z, -2);
  assert.equal(clipped.b.z, 2);
  assert.equal(clipHeight(segment([0, 0, 10], [10, 0, 10], "upper"), 0), null);
});

test("camera queries reuse indexed geometry and cached bridges", () => {
  const lower = segment([-1000, 0, -4], [1000, 0, -4], "lower"),
    upper = segment([0, -1000, 8], [0, 1000, 8], "upper");
  const roads = [lower, upper].map((s) => ({
    ...s.road,
    points: [s.a, s.b],
    bounds: [
      Math.min(s.a.x, s.b.x),
      Math.min(s.a.y, s.b.y),
      Math.max(s.a.x, s.b.x),
      Math.max(s.a.y, s.b.y),
    ],
  }));
  const index = new RoadIndex(roads);
  index.setBridges([
    {
      x: 0,
      y: 0,
      upper,
      lower,
      upperZ: 8,
      lowerZ: -4,
      upperGroup: "upper",
      lowerGroup: "lower",
      hits: [{ x: 0, y: 0, upper, lower, upperZ: 8, lowerZ: -4 }],
      direction: { x: 0, y: 1 },
      halfLength: 10,
      halfWidth: 3,
    },
  ]);
  const scene = index.scene({ x: 0, y: 0, scale: 2 }, { w: 200, h: 200 });
  assert.equal(scene.bridges.length, 1);
  assert.equal(scene.segments[0], index.segments[0]);
  const elsewhere = index.scene({ x: 500, y: 0, scale: 2 }, { w: 200, h: 200 });
  assert.equal(elsewhere.bridges.length, 0);
  assert.equal(elsewhere.segments.length, 1);
});
test("road detail levels preserve corners and vertical ramps without mutating routing geometry", () => {
  const road = {
    id: "bend",
    bounds: [],
    points: [
      { x: 0, y: 0, z: 0 },
      { x: 10, y: 0.01, z: 0 },
      { x: 20, y: 0, z: 0 },
      { x: 20, y: 20, z: 0 },
      { x: 20, y: 30, z: 10 },
      { x: 20, y: 40, z: 0 },
    ],
  };
  const simplified = simplifyRoad(road, 1);
  assert.deepEqual(simplified.points, [
    road.points[0],
    ...road.points.slice(2),
  ]);
  assert.equal(road.points.length, 6);
  assert.equal(simplified.points[0], road.points[0]);
});

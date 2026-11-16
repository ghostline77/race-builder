import { clipHeight, elevationColor } from "./road-levels";
import type {
  Bridge,
  Camera,
  MapSize,
  Point,
  RoadScene,
  Segment,
} from "./road-levels";
type Context = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;
export function paintDeck(
  ctx: Context,
  bridge: Bridge,
  camera: Camera,
  size: MapSize,
  minimum: number,
  maximum: number,
  background: string,
) {
  const d = bridge.direction,
    n = { x: -d.y, y: d.x };
  const at = (along: number, across: number): Point => ({
    x: bridge.x + d.x * along + n.x * across,
    y: bridge.y + d.y * along + n.y * across,
    z: bridge.upperZ,
  });
  const screen = (p: Point) => ({
    x: (p.x - camera.x) * camera.scale + size.w / 2,
    y: -(p.y - camera.y) * camera.scale + size.h / 2,
  });
  const line = (a: Point, b: Point, color: string, width: number) => {
    const p = screen(a),
      q = screen(b);
    ctx.strokeStyle = color;
    ctx.lineWidth = width;
    ctx.beginPath();
    ctx.moveTo(p.x, p.y);
    ctx.lineTo(q.x, q.y);
    ctx.stroke();
  };
  const w = Math.max(bridge.halfWidth, 4 / camera.scale),
    l = Math.max(bridge.halfLength, 9 / camera.scale);
  line(at(-l, 0), at(l, 0), background, w * 2 * camera.scale + 6);
  for (const side of [-1, 1])
    line(at(-l, side * w), at(l, side * w), "#c7cdd3", 1.4);
  ctx.save();
  ctx.beginPath();
  for (const [i, p] of [at(-l, -w), at(l, -w), at(l, w), at(-l, w)].entries()) {
    const q = screen(p);
    i ? ctx.lineTo(q.x, q.y) : ctx.moveTo(q.x, q.y);
  }
  ctx.closePath();
  ctx.clip();
  for (const hit of bridge.hits)
    line(
      hit.upper.a,
      hit.upper.b,
      elevationColor(bridge.upperZ, minimum, maximum),
      1.5,
    );
  ctx.restore();
}
export function paintRoads(
  ctx: Context,
  scene: RoadScene,
  camera: Camera,
  size: MapSize,
  height: number | null,
  minimum: number,
  maximum: number,
  background: string,
) {
  const batches = new Map<number, Segment[]>();
  for (const segment of scene.segments) {
    const visible = clipHeight(segment, height);
    if (!visible) continue;
    const steps = Math.max(
      1,
      Math.ceil(Math.abs(visible.b.z - visible.a.z) / 2),
    );
    const at = (t: number): Point => ({
      x: visible.a.x + (visible.b.x - visible.a.x) * t,
      y: visible.a.y + (visible.b.y - visible.a.y) * t,
      z: visible.a.z + (visible.b.z - visible.a.z) * t,
    });
    for (let i = 0; i < steps; i++) {
      const part =
        steps === 1
          ? visible
          : { ...visible, a: at(i / steps), b: at((i + 1) / steps) };
      const band = Math.round(part.a.z + part.b.z),
        batch = batches.get(band);
      if (batch) batch.push(part);
      else batches.set(band, [part]);
    }
  }
  const layers = [...batches.keys()].map((z) => ({
    z: z / 2,
    draw: () => {
      ctx.strokeStyle = elevationColor(z / 2, minimum, maximum);
      ctx.lineWidth = 1.4;
      ctx.beginPath();
      for (const s of batches.get(z)!) {
        ctx.moveTo(
          (s.a.x - camera.x) * camera.scale + size.w / 2,
          -(s.a.y - camera.y) * camera.scale + size.h / 2,
        );
        ctx.lineTo(
          (s.b.x - camera.x) * camera.scale + size.w / 2,
          -(s.b.y - camera.y) * camera.scale + size.h / 2,
        );
      }
      ctx.stroke();
    },
  }));
  for (const bridge of scene.bridges)
    if (height === null || Math.abs(bridge.upperZ - height) <= 2)
      layers.push({
        z: bridge.upperZ - 0.01,
        draw: () =>
          paintDeck(ctx, bridge, camera, size, minimum, maximum, background),
      });
  layers.sort((a, b) => a.z - b.z);
  for (const layer of layers) layer.draw();
}

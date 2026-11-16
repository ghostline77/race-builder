/// <reference lib="webworker" />
import { RoadIndex } from "./road-index";
import { simplifyRoad } from "./road-lod";
import { paintRoads } from "./road-painter";
import type { Bridge, Road } from "./road-levels";
declare const self: DedicatedWorkerGlobalScope;
export interface TileSpec {
  key: string;
  zoom: number;
  x: number;
  y: number;
  ratio: number;
  height: number | null;
  background: string;
}
export type TileInput =
  | {
      kind: "init";
      roads: Road[];
      bridges: Bridge[];
      minimum: number;
      maximum: number;
    }
  | { kind: "view"; tiles: TileSpec[] };
export type TileOutput =
  | { kind: "tile"; spec: TileSpec; bitmap: ImageBitmap }
  | { kind: "error"; message: string };
let levels: RoadIndex[] = [],
  minimum = 0,
  maximum = 1,
  queue: TileSpec[] = [],
  scheduled = false;
const tolerances = [0, 1, 8, 32];
function schedule() {
  if (scheduled) return;
  scheduled = true;
  setTimeout(drawNext, 0);
}
function drawNext() {
  scheduled = false;
  const spec = queue.shift();
  if (!spec) return;
  try {
    const scale = 2 ** (spec.zoom / 2),
      span = 256 / scale;
    const level =
      spec.height !== null || scale >= 1
        ? 0
        : scale >= 0.25
          ? 1
          : scale >= 0.05
            ? 2
            : 3;
    const index = levels[level]!;
    // Two pixels of gutter avoid antialiasing seams at tile boundaries.
    const size = { w: 260, h: 260 },
      camera = { x: (spec.x + 0.5) * span, y: (spec.y + 0.5) * span, scale };
    const scene = index.scene(
      camera,
      { w: size.w + 120 * scale, h: size.h + 120 * scale },
    );
    const canvas = new OffscreenCanvas(
      Math.ceil(260 * spec.ratio),
      Math.ceil(260 * spec.ratio),
    );
    const ctx = canvas.getContext("2d");
    if (!ctx) throw Error("Road tile rendering unavailable");
    ctx.scale(spec.ratio, spec.ratio);
    ctx.fillStyle = spec.background;
    ctx.fillRect(0, 0, 260, 260);
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    paintRoads(
      ctx,
      scene,
      camera,
      size,
      spec.height,
      minimum,
      maximum,
      spec.background,
    );
    const bitmap = canvas.transferToImageBitmap();
    self.postMessage({ kind: "tile", spec, bitmap } satisfies TileOutput, [
      bitmap,
    ]);
  } catch (error) {
    self.postMessage({
      kind: "error",
      message: error instanceof Error ? error.message : String(error),
    } satisfies TileOutput);
  }
  if (queue.length) schedule();
}
self.onmessage = (event: MessageEvent<TileInput>) => {
  const message = event.data;
  if (message.kind === "init") {
    minimum = message.minimum;
    maximum = message.maximum;
    levels = tolerances.map((t) => {
      const index = new RoadIndex(
        t ? message.roads.map((r) => simplifyRoad(r, t)) : message.roads,
      );
      index.setBridges(message.bridges);
      return index;
    });
  } else {
    // Replace stale requests when the camera moves; never drain an old city view.
    queue = message.tiles;
    schedule();
  }
};

import TileWorker from "./road-tile-worker?worker&inline";
import type { TileInput, TileOutput, TileSpec } from "./road-tile-worker";
import type { Bridge, Camera, MapSize, Road } from "./road-levels";
interface Tile {
  spec: TileSpec;
  bitmap: ImageBitmap;
}
/** Bounded raster cache. Geometry and rasterization never run in input handlers. */
export class RoadTiles {
  private worker: Worker | null = null;
  private cache = new Map<string, Tile>();
  private requested = "";
  private revision = 0;
  private bytes = 0;
  private visible = new Set<string>();
  error = "";
  constructor(private readonly redraw: () => void) {}
  setRoads(roads: Road[], bridges: Bridge[], minimum: number, maximum: number) {
    this.worker?.terminate();
    for (const tile of this.cache.values()) tile.bitmap.close();
    this.cache.clear();
    this.bytes = 0;
    this.requested = "";
    this.error = "";
    const worker = new TileWorker();
    this.worker = worker;
    const revision = ++this.revision;
    worker.onmessage = (event: MessageEvent<TileOutput>) => {
      const data = event.data;
      if (revision !== this.revision) {
        if (data.kind === "tile") data.bitmap.close();
        return;
      }
      if (data.kind === "error") {
        this.error = data.message;
        this.redraw();
        return;
      }
      const old = this.cache.get(data.spec.key);
      if (old) {
        this.bytes -= old.bitmap.width * old.bitmap.height * 4;
        old.bitmap.close();
        this.cache.delete(data.spec.key);
      }
      this.cache.set(data.spec.key, { spec: data.spec, bitmap: data.bitmap });
      this.bytes += data.bitmap.width * data.bitmap.height * 4;
      while (this.bytes > 64 * 1024 * 1024 && this.cache.size > 1) {
        const key = [...this.cache.keys()].find(
          (key) => !this.visible.has(key),
        );
        if (key === undefined) break; // A large high-DPI viewport must retain its visible tiles.
        const evicted = this.cache.get(key)!;
        this.bytes -= evicted.bitmap.width * evicted.bitmap.height * 4;
        evicted.bitmap.close();
        this.cache.delete(key);
      }
      this.redraw();
    };
    worker.onerror = () => {
      this.error = "Road rendering unavailable";
      this.redraw();
    };
    worker.postMessage({
      kind: "init",
      roads,
      bridges,
      minimum,
      maximum,
    } satisfies TileInput);
  }
  draw(
    ctx: CanvasRenderingContext2D,
    camera: Camera,
    size: MapSize,
    height: number | null,
    background: string,
  ) {
    const zoom = Math.ceil(Math.log2(camera.scale) * 2),
      scale = 2 ** (zoom / 2),
      span = 256 / scale,
      ratio = Math.min(2, devicePixelRatio || 1);
    const minX = camera.x - size.w / camera.scale / 2,
      maxX = camera.x + size.w / camera.scale / 2,
      minY = camera.y - size.h / camera.scale / 2,
      maxY = camera.y + size.h / camera.scale / 2;
    const specs: TileSpec[] = [];
    for (
      let x = Math.floor(minX / span) - 1;
      x <= Math.floor(maxX / span) + 1;
      x++
    )
      for (
        let y = Math.floor(minY / span) - 1;
        y <= Math.floor(maxY / span) + 1;
        y++
      )
        specs.push({
          key: [zoom, x, y, ratio, height, background].join(":"),
          zoom,
          x,
          y,
          ratio,
          height,
          background,
        });
    this.visible = new Set(
      specs
        .filter(
          (s) =>
            s.x * span <= maxX &&
            (s.x + 1) * span >= minX &&
            s.y * span <= maxY &&
            (s.y + 1) * span >= minY,
        )
        .map((s) => s.key),
    );
    const missing = specs.filter((s) => !this.cache.has(s.key));
    missing.sort(
      (a, b) =>
        Math.hypot(
          (a.x + 0.5) * span - camera.x,
          (a.y + 0.5) * span - camera.y,
        ) -
        Math.hypot(
          (b.x + 0.5) * span - camera.x,
          (b.y + 0.5) * span - camera.y,
        ),
    );
    const wanted = specs.map((s) => s.key).join("|");
    if (wanted !== this.requested) {
      this.requested = wanted;
      this.worker?.postMessage({
        kind: "view",
        tiles: missing,
      } satisfies TileInput);
    }
    const blit = (tile: Tile) => {
      const tspan = 256 / 2 ** (tile.spec.zoom / 2),
        x = tile.spec.x * tspan,
        y = (tile.spec.y + 1) * tspan;
      if (x + tspan < minX || x > maxX || y < minY || y - tspan > maxY) return;
      ctx.drawImage(
        tile.bitmap,
        2 * tile.spec.ratio,
        2 * tile.spec.ratio,
        256 * tile.spec.ratio,
        256 * tile.spec.ratio,
        (x - camera.x) * camera.scale + size.w / 2,
        -(y - camera.y) * camera.scale + size.h / 2,
        tspan * camera.scale,
        tspan * camera.scale,
      );
    };
    // Keep previous detail visible while replacement tiles arrive.
    for (const tile of this.cache.values())
      if (
        tile.spec.zoom !== zoom &&
        tile.spec.height === height &&
        tile.spec.background === background
      )
        blit(tile);
    for (const spec of specs) {
      const tile = this.cache.get(spec.key);
      if (!tile) continue;
      blit(tile);
      this.cache.delete(spec.key);
      this.cache.set(spec.key, tile);
    }
    return missing.length;
  }
}

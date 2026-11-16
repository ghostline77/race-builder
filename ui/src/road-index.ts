import type {
  Bridge,
  Camera,
  MapSize,
  Road,
  RoadScene,
  Segment,
} from "./road-levels";

/** Immutable geometry index shared by all camera views. */
export class RoadIndex {
  readonly segments: Segment[] = [];
  private readonly cells = new Map<string, number[]>();
  private bridges: Bridge[] = [];
  constructor(roads: Road[]) {
    for (const road of roads)
      for (let i = 1; i < road.points.length; i++) {
        const a = road.points[i - 1]!,
          b = road.points[i]!;
        if (Math.hypot(b.x - a.x, b.y - a.y) < 1e-5) continue;
        const index = this.segments.length;
        this.segments.push({ a, b, road });
        for (
          let x = Math.floor(Math.min(a.x, b.x) / 128);
          x <= Math.floor(Math.max(a.x, b.x) / 128);
          x++
        )
          for (
            let y = Math.floor(Math.min(a.y, b.y) / 128);
            y <= Math.floor(Math.max(a.y, b.y) / 128);
            y++
          ) {
            const key = `${x},${y}`;
            let cell = this.cells.get(key);
            if (!cell) {
              cell = [];
              this.cells.set(key, cell);
            }
            cell.push(index);
          }
      }
  }
  setBridges(bridges: Bridge[]) {
    this.bridges = bridges;
  }
  crossings(camera: Camera, size: MapSize): Bridge[] {
    if (camera.scale < 0.25) return [];
    const dx = size.w / camera.scale / 2,
      dy = size.h / camera.scale / 2;
    return this.bridges.filter(
      (b) => Math.abs(b.x - camera.x) <= dx && Math.abs(b.y - camera.y) <= dy,
    );
  }
  scene(camera: Camera, size: MapSize): RoadScene {
    const minX = camera.x - size.w / camera.scale / 2,
      maxX = camera.x + size.w / camera.scale / 2,
      minY = camera.y - size.h / camera.scale / 2,
      maxY = camera.y + size.h / camera.scale / 2;
    const candidates = new Set<number>();
    // A city overview is cheaper to scan once than to enumerate thousands of empty cells.
    const cells =
      (Math.ceil((maxX - minX) / 128) + 1) *
      (Math.ceil((maxY - minY) / 128) + 1);
    if (cells > this.cells.size) {
      for (let i = 0; i < this.segments.length; i++) candidates.add(i);
    } else
      for (let x = Math.floor(minX / 128); x <= Math.floor(maxX / 128); x++)
        for (let y = Math.floor(minY / 128); y <= Math.floor(maxY / 128); y++)
          for (const index of this.cells.get(`${x},${y}`) || [])
            candidates.add(index);
    const segments: Segment[] = [];
    let minimum = Infinity,
      maximum = -Infinity;
    for (const index of candidates) {
      const segment = this.segments[index]!,
        { a, b } = segment;
      if (
        Math.max(a.x, b.x) < minX ||
        Math.min(a.x, b.x) > maxX ||
        Math.max(a.y, b.y) < minY ||
        Math.min(a.y, b.y) > maxY
      )
        continue;
      segments.push(segment);
      minimum = Math.min(minimum, a.z, b.z);
      maximum = Math.max(maximum, a.z, b.z);
    }
    return {
      segments,
      bridges: this.crossings(camera, size),
      minimum: Number.isFinite(minimum) ? minimum : 0,
      maximum: Number.isFinite(maximum) ? maximum : 1,
    };
  }
}

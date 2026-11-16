import { clipHeight } from "./road-levels";
import { RoadIndex } from "./road-index";
import { RoadTiles } from "./road-tiles";
import { paintDeck } from "./road-painter";
import type {
  Bridge,
  Camera,
  Point,
  Road,
  RoadScene,
  Segment,
} from "./road-levels";

import type { RouteMark } from "./models";
export interface MapHooks {
  ready: () => boolean;
  camera: () => Camera;
  setCamera: (camera: Camera) => void;
  marks: () => RouteMark[];
  course: () => RouteMark[];
  guidance?: () => Point[];
  anchors: () => RouteMark[];
  selected: () => string | null;
  mode: () => string;
  looped: () => boolean;
  projectId: () => string;
  label: (mark: RouteMark) => string;
  select: (mark: RouteMark, details: boolean) => void;
  move: (
    mark: RouteMark,
    position: Point,
    height: number | null,
    snap: boolean,
  ) => void;
  place: (
    position: Point,
    role: string,
    height: number | null,
    snap: boolean,
  ) => void;
}
interface Hover {
  road: Road;
  p: Point;
  distance: number;
  stacked: boolean;
}
const element = <T extends HTMLElement>(id: string) => {
  const node = document.getElementById(id);
  if (!node) throw Error(`Missing map control ${id}`);
  return node as T;
};
const metres = (z: number) => `${z < 0 ? "−" : "+"}${Math.abs(z).toFixed(1)} m`;
const interpolate = (a: Point, b: Point, t: number): Point => ({
  x: a.x + t * (b.x - a.x),
  y: a.y + t * (b.y - a.y),
  z: a.z + t * (b.z - a.z),
});
type Paint = { z: number; draw: () => void };

export class RouteMap {
  private roads: Road[] = [];
  private index = new RoadIndex([]);
  private readonly tiles = new RoadTiles(() => this.requestDraw());
  private minimum = 0;
  private maximum = 1;
  private frame: number | null = null;
  private ghost: Point | null = null;
  private pointerPosition: Point | null = null;
  private viewTimer: ReturnType<typeof setTimeout> | undefined;
  private savedViewKey = "";
  private scene: RoadScene = {
    segments: [],
    bridges: [],
    minimum: 0,
    maximum: 1,
  };
  private sceneKey = "";
  private size = { w: 1, h: 1 };
  private hover: Hover | null = null;
  private activeBridge: Bridge | null = null;
  private labelBoxes: { x: number; y: number; w: number; h: number }[] = [];
  private drag: {
    x: number;
    y: number;
    camera: Camera;
    mark: RouteMark | undefined;
    moved: boolean;
  } | null = null;
  private readonly ctx: CanvasRenderingContext2D;
  constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly hooks: MapHooks,
  ) {
    const ctx = canvas.getContext("2d");
    if (!ctx) throw Error("Canvas rendering unavailable");
    this.ctx = ctx;
    canvas.addEventListener("pointerdown", (event) => this.pointerDown(event));
    canvas.addEventListener("pointermove", (event) => this.pointerMove(event));
    canvas.addEventListener("pointerup", (event) => this.pointerUp(event));
    canvas.addEventListener("pointercancel", () => {
      this.drag = null;
      this.ghost = null;
      this.draw();
    });
    canvas.addEventListener("pointerleave", () => {
      this.hover = null;
      element("coordinates").textContent = "World coordinates · metres";
      this.draw();
    });
    canvas.addEventListener(
      "wheel",
      (event) => {
        event.preventDefault();
        this.hover = null;
        this.pointerPosition = null;
        const rect = canvas.getBoundingClientRect(),
          x = event.clientX - rect.left,
          y = event.clientY - rect.top,
          before = this.world(x, y),
          camera = { ...hooks.camera() };
        camera.scale = Math.max(
          0.005,
          Math.min(80, camera.scale * Math.exp(-event.deltaY * 0.001)),
        );
        hooks.setCamera(camera);
        const after = this.world(x, y);
        camera.x += before.x - after.x;
        camera.y += before.y - after.y;
        this.requestDraw();
      },
      { passive: false },
    );
    element("focus-upper").addEventListener("click", () =>
      this.focusLevel(this.activeBridge?.upperZ),
    );
    element("focus-lower").addEventListener("click", () =>
      this.focusLevel(this.activeBridge?.lowerZ),
    );
    element("all-levels").addEventListener("click", () => this.focusLevel());
    new ResizeObserver(() => this.draw()).observe(canvas);
    window.addEventListener("pagehide", () => this.saveView());
  }
  setRoads(roads: Road[], bridges: Bridge[]) {
    this.roads = roads;
    this.index = new RoadIndex(roads);
    this.sceneKey = "";
    this.index.setBridges(bridges);
    this.minimum = Infinity;
    this.maximum = -Infinity;
    for (const s of this.index.segments) {
      this.minimum = Math.min(this.minimum, s.a.z, s.b.z);
      this.maximum = Math.max(this.maximum, s.a.z, s.b.z);
    }
    if (!Number.isFinite(this.minimum)) {
      this.minimum = 0;
      this.maximum = 1;
    }
    this.tiles.setRoads(roads, bridges, this.minimum, this.maximum);
    this.draw();
  }
  private height() {
    const value = element<HTMLInputElement>("height").value;
    return value === "" ? null : Number(value);
  }
  private focusLevel(height?: number) {
    element<HTMLInputElement>("height").value =
      height === undefined ? "" : height.toFixed(1);
    this.hover = null;
    this.draw();
  }
  private screen(p: { x: number; y: number }) {
    const c = this.hooks.camera();
    return {
      x: (p.x - c.x) * c.scale + this.size.w / 2,
      y: -(p.y - c.y) * c.scale + this.size.h / 2,
    };
  }
  private world(x: number, y: number): Point {
    const c = this.hooks.camera();
    return {
      x: (x - this.size.w / 2) / c.scale + c.x,
      y: -(y - this.size.h / 2) / c.scale + c.y,
      z: Number(element<HTMLInputElement>("free-z").value) || 0,
    };
  }
  private line(a: Point, b: Point, color: string, width: number) {
    const p = this.screen(a),
      q = this.screen(b),
      ctx = this.ctx;
    ctx.strokeStyle = color;
    ctx.lineWidth = width;
    ctx.beginPath();
    ctx.moveTo(p.x, p.y);
    ctx.lineTo(q.x, q.y);
    ctx.stroke();
  }
  private roadHover(p: Point): Hover | null {
    const hits: Hover[] = [],
      height = this.height();
    for (const segment of this.index.scene(
      { x: p.x, y: p.y, scale: 1 },
      { w: 40, h: 40 },
    ).segments) {
      const { a, b } = segment,
        dx = b.x - a.x,
        dy = b.y - a.y,
        d = dx * dx + dy * dy;
      if (d < 1e-8) continue;
      const t = Math.max(
          0,
          Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / d),
        ),
        q = interpolate(a, b, t),
        distance = Math.hypot(q.x - p.x, q.y - p.y);
      if (distance < 15 && (height === null || Math.abs(q.z - height) <= 2))
        hits.push({ road: segment.road, p: q, distance, stacked: false });
    }
    hits.sort((a, b) => a.distance - b.distance);
    const hit = hits[0];
    if (!hit) return null;
    hit.stacked = hits.some(
      (other) =>
        other.distance < hit.distance + 5 && Math.abs(other.p.z - hit.p.z) > 4,
    );
    return hit;
  }
  private pointerDown(event: PointerEvent) {
    if (!this.hooks.ready()) return;
    const rect = this.canvas.getBoundingClientRect(),
      x = event.clientX - rect.left,
      y = event.clientY - rect.top;
    const mark =
      this.hooks.mode() === "edit"
        ? this.hooks
            .marks()
            .filter(
              (m) =>
                !m.options?.generated_road_sample ||
                element<HTMLInputElement>("show-samples").checked,
            )
            .find((m) => {
              const p = this.screen(m.position);
              return Math.hypot(p.x - x, p.y - y) < 12;
            })
        : undefined;
    this.drag = {
      x: event.clientX,
      y: event.clientY,
      camera: { ...this.hooks.camera() },
      mark,
      moved: false,
    };
    this.canvas.setPointerCapture(event.pointerId);
  }
  private pointerMove(event: PointerEvent) {
    const rect = this.canvas.getBoundingClientRect(),
      p = this.world(event.clientX - rect.left, event.clientY - rect.top);
    this.pointerPosition = this.drag && !this.drag.mark ? null : p;
    if (this.drag && !this.drag.mark) this.hover = null;
    if (this.hooks.camera().scale < 0.25 && !this.drag) {
      this.hover = null;
      this.pointerPosition = null;
      element("coordinates").textContent =
        `X ${p.x.toFixed(1)} · Y ${p.y.toFixed(1)}`;
      return;
    }
    if (this.drag) {
      const dx = event.clientX - this.drag.x,
        dy = event.clientY - this.drag.y;
      if (Math.hypot(dx, dy) > 4) this.drag.moved = true;
      if (this.drag.moved && !this.drag.mark) {
        const c = this.hooks.camera();
        this.hooks.setCamera({
          ...c,
          x: this.drag.camera.x - dx / c.scale,
          y: this.drag.camera.y + dy / c.scale,
        });
      }
    }
    this.ghost = this.drag?.mark && this.drag.moved ? p : null;
    this.requestDraw();
  }
  private pointerUp(event: PointerEvent) {
    const drag = this.drag;
    if (!drag) return;
    this.drag = null;
    this.ghost = null;
    this.requestDraw();
    const rect = this.canvas.getBoundingClientRect(),
      p = this.world(event.clientX - rect.left, event.clientY - rect.top),
      snap = element<HTMLInputElement>("snap").checked;
    if (drag.mark) {
      this.hooks.select(drag.mark, !drag.moved);
      if (drag.moved) {
        p.z = drag.mark.position.z;
        this.hooks.move(drag.mark, p, this.height(), snap);
      }
    } else if (!drag.moved && ["draw", "place"].includes(this.hooks.mode()))
      this.hooks.place(
        p,
        element<HTMLSelectElement>("role").value,
        this.height(),
        snap,
      );
  }
  fit(city = false) {
    const rect = this.canvas.getBoundingClientRect();
    this.size = { w: rect.width, h: rect.height };
    const source =
      !city && this.hooks.course().length
        ? this.hooks.course().map((m) => m.position)
        : this.roads.flatMap((r) => r.points);
    if (!source.length) return;
    let minX = Infinity,
      maxX = -Infinity,
      minY = Infinity,
      maxY = -Infinity;
    for (const p of source) {
      minX = Math.min(minX, p.x);
      maxX = Math.max(maxX, p.x);
      minY = Math.min(minY, p.y);
      maxY = Math.max(maxY, p.y);
    }
    this.hooks.setCamera({
      x: (minX + maxX) / 2,
      y: (minY + maxY) / 2,
      scale: Math.max(
        0.01,
        Math.min(
          this.size.w / (maxX - minX + 100),
          this.size.h / (maxY - minY + 100),
        ),
      ),
    });
    this.draw();
  }
  draw() {
    this.requestDraw();
  }
  private requestDraw() {
    if (this.frame !== null) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = null;
      this.renderFrame();
    });
  }
  private renderFrame() {
    if (!this.hooks.ready()) return;
    const rect = this.canvas.getBoundingClientRect();
    if (!rect.width || !rect.height) return;
    const ratio = devicePixelRatio || 1;
    if (
      this.canvas.width !== Math.round(rect.width * ratio) ||
      this.canvas.height !== Math.round(rect.height * ratio)
    ) {
      this.canvas.width = Math.round(rect.width * ratio);
      this.canvas.height = Math.round(rect.height * ratio);
    }
    this.size = { w: rect.width, h: rect.height };
    const ctx = this.ctx,
      camera = this.hooks.camera(),
      height = this.height();
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    const background =
      getComputedStyle(this.canvas)
        .getPropertyValue("--color-background")
        .trim() || "#111518";
    ctx.fillStyle = background;
    ctx.fillRect(0, 0, rect.width, rect.height);
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    this.tiles.draw(ctx, camera, this.size, height, background);
    element("bridge-status").textContent =
      this.tiles.error || "Raised deck = no junction · green = course";
    const key = [
      camera.x,
      camera.y,
      camera.scale,
      rect.width,
      rect.height,
    ].join(":");
    if (key !== this.sceneKey) {
      this.scene = {
        segments: [],
        bridges: this.index.crossings(camera, this.size),
        minimum: this.minimum,
        maximum: this.maximum,
      };
      this.sceneKey = key;
      this.activeBridge = this.scene.bridges.reduce<Bridge | null>(
        (best, b) =>
          !best ||
          Math.hypot(b.x - camera.x, b.y - camera.y) <
            Math.hypot(best.x - camera.x, best.y - camera.y)
            ? b
            : best,
        null,
      );
    }
    if (this.pointerPosition) {
      const p = this.pointerPosition;
      this.hover = this.roadHover(p);
      element("coordinates").textContent =
        `X ${p.x.toFixed(1)} · Y ${p.y.toFixed(1)}${this.hover ? " · Road " + metres(this.hover.p.z) : ""}`;
      if (!this.drag) {
        this.activeBridge = this.scene.bridges.reduce<Bridge | null>(
          (best, b) =>
            !best ||
            Math.hypot(b.x - p.x, b.y - p.y) <
              Math.hypot(best.x - p.x, best.y - p.y)
              ? b
              : best,
          null,
        );
      }
    }
    const paints: Paint[] = [];
    // Only authored geometry and interaction overlays are painted on this thread.
    const add = (segment: Segment, route: boolean) => {
      const visible = clipHeight(segment, height);
      if (!visible) {
        if (route)
          paints.push({
            z: (segment.a.z + segment.b.z) / 2,
            draw: () => {
              ctx.globalAlpha = 0.22;
              this.line(segment.a, segment.b, "#c8f58a", 3);
              ctx.globalAlpha = 1;
            },
          });
        return;
      }
      const steps = Math.max(
        1,
        Math.ceil(Math.abs(visible.b.z - visible.a.z) / 2),
      );
      for (let i = 0; i < steps; i++) {
        const a =
            steps === 1
              ? visible.a
              : interpolate(visible.a, visible.b, i / steps),
          b =
            steps === 1
              ? visible.b
              : interpolate(visible.a, visible.b, (i + 1) / steps),
          z = (a.z + b.z) / 2;
        paints.push({
          z,
          draw: () => this.line(a, b, "#c8f58a", 4),
        });
      }
    };
    if (this.hover)
      for (let i = 1; i < this.hover.road.points.length; i++) {
        const a = this.hover.road.points[i - 1]!,
          b = this.hover.road.points[i]!;
        paints.push({
          z: (a.z + b.z) / 2,
          draw: () => this.line(a, b, "#9bd4f6", 3),
        });
      }
    const course = this.hooks.course(),
      routeRoad: Road = { id: "route", points: [], bounds: [] };
    const preview = this.hooks.guidance?.() ?? [];
    const line = preview.length ? preview : course.map(m => m.position);
    for (let i = 1; i < line.length; i++)
      add(
        { a: line[i - 1]!, b: line[i]!, road: routeRoad },
        true,
      );
    if (this.hooks.looped() && course.length > 2)
      add(
        { a: line.at(-1)!, b: line[0]!, road: routeRoad },
        true,
      );
    for (const bridge of this.scene.bridges)
      if (height === null || Math.abs(bridge.upperZ - height) <= 2)
        paints.push({
          z: bridge.upperZ - 0.01,
          draw: () =>
            paintDeck(
              ctx,
              bridge,
              camera,
              this.size,
              this.minimum,
              this.maximum,
              background,
            ),
        });
    paints.sort((a, b) => a.z - b.z);
    for (const paint of paints) paint.draw();
    if (this.ghost) {
      const p = this.screen(this.ghost);
      ctx.strokeStyle = "#fff";
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(p.x, p.y, 9, 0, Math.PI * 2);
      ctx.stroke();
    }
    const anchors = this.hooks.anchors(),
      selected = this.hooks.selected();
    this.labelBoxes = [];
    for (const mark of this.hooks
      .marks()
      .filter(
        (m) =>
          !m.options?.generated_road_sample ||
          element<HTMLInputElement>("show-samples").checked,
      )) {
      const p = this.screen(mark.position);
      if (p.x < 0 || p.y < 0 || p.x > rect.width || p.y > rect.height) continue;
      const index = anchors.findIndex((a) => a.id === mark.id);
      ctx.globalAlpha =
        height !== null && Math.abs(mark.position.z - height) > 2 ? 0.35 : 1;
      ctx.fillStyle =
        mark.id === selected
          ? "#fff"
          : index === 0
            ? "#89c9ed"
            : index === anchors.length - 1
              ? "#f6b08c"
              : mark.role === "spline_point"
                ? "#c8f58a"
                : "#89c9ed";
      ctx.beginPath();
      ctx.arc(
        p.x,
        p.y,
        mark.options?.generated_road_sample
          ? 2
          : mark.role === "spline_point"
            ? 7
            : 4,
        0,
        Math.PI * 2,
      );
      ctx.fill();
      ctx.strokeStyle = background;
      ctx.lineWidth = 2;
      ctx.stroke();
      if (
        !mark.options?.generated_road_sample &&
        (mark.role === "spline_point" ||
          mark.role === "speed_change" ||
          mark.id === selected)
      )
        this.markLabel(
          p,
          (mark.role === "speed_change"
            ? `${Math.round((mark.options.speed_mps ?? 0) * 3.6)} km/h`
            : mark.name || this.hooks.label(mark)) +
            " · " +
            metres(mark.position.z),
        );
      ctx.globalAlpha = 1;
    }
    this.bridgeLabels();
    this.levelPanel();
    element("map-empty").hidden = this.roads.length > 0;
    element("elevation-low").textContent = metres(this.minimum);
    element("elevation-high").textContent = metres(this.maximum);
    element("map-elevation").hidden = !this.roads.length;
    const viewKey = this.hooks.projectId() + ":" + key;
    if (viewKey !== this.savedViewKey) {
      this.savedViewKey = viewKey;
      clearTimeout(this.viewTimer);
      this.viewTimer = setTimeout(() => this.saveView(), 150);
    }
  }
  private saveView() {
    if (this.hooks.ready())
      sessionStorage.setItem(
        "race-view",
        JSON.stringify({
          project: this.hooks.projectId(),
          camera: this.hooks.camera(),
        }),
      );
  }
  private markLabel(point: { x: number; y: number }, text: string) {
    const ctx = this.ctx;
    ctx.font = "12px system-ui";
    const w = ctx.measureText(text).width + 10,
      h = 20;
    const choices = [
      { x: point.x + 12, y: point.y - 25 },
      { x: point.x + 12, y: point.y + 8 },
      { x: point.x - w - 12, y: point.y - 25 },
      { x: point.x - w - 12, y: point.y + 8 },
    ];
    const box =
      choices.find(
        (p) =>
          p.x >= 2 &&
          p.y >= 2 &&
          p.x + w < this.size.w - 2 &&
          p.y + h < this.size.h - 2 &&
          !this.labelBoxes.some(
            (q) =>
              p.x < q.x + q.w &&
              p.x + w > q.x &&
              p.y < q.y + q.h &&
              p.y + h > q.y,
          ),
      ) || choices[0]!;
    this.labelBoxes.push({ ...box, w, h });
    ctx.fillStyle = "#121a21dd";
    ctx.fillRect(box.x, box.y, w, h);
    ctx.fillStyle = "#edf0f2";
    ctx.fillText(text, box.x + 5, box.y + 14);
  }
  private bridgeLabels() {
    if (this.hooks.camera().scale < 0.6) return;
    const ctx = this.ctx,
      occupied: { x: number; y: number }[] = [];
    const bridges = [...this.scene.bridges].sort((a, b) =>
      a === this.activeBridge ? -1 : b === this.activeBridge ? 1 : 0,
    );
    for (const bridge of bridges) {
      const p = this.screen(bridge);
      if (
        p.x < 15 ||
        p.y < 55 ||
        p.x > this.size.w - 15 ||
        p.y > this.size.h - 35 ||
        occupied.some(
          (q) => Math.abs(q.x - p.x) < 170 && Math.abs(q.y - p.y) < 45,
        )
      )
        continue;
      occupied.push(p);
      const label = `${metres(bridge.upperZ)} over ${metres(bridge.lowerZ)}`,
        x = Math.max(8, Math.min(this.size.w - 178, p.x + 18)),
        y = p.y - 44;
      ctx.fillStyle = "#121a21ee";
      ctx.fillRect(x, y, 170, 37);
      ctx.strokeStyle = "#8797a5";
      ctx.lineWidth = 1;
      ctx.strokeRect(x, y, 170, 37);
      ctx.font = "12px system-ui";
      ctx.fillStyle = "#edf0f2";
      ctx.fillText(label, x + 8, y + 14);
      ctx.fillStyle = "#b8c2ca";
      ctx.font = "11px system-ui";
      ctx.fillText("Bridge / underpass · no junction", x + 8, y + 29);
    }
  }
  private levelPanel() {
    const bridge = this.activeBridge;
    element("crossing-levels").hidden = !bridge;
    if (!bridge) return;
    element("focus-upper").textContent = "Upper road " + metres(bridge.upperZ);
    element("focus-lower").textContent = "Lower road " + metres(bridge.lowerZ);
    element("crossing-gap").textContent =
      `${(bridge.upperZ - bridge.lowerZ).toFixed(1)} m apart · no turn here`;
  }
}

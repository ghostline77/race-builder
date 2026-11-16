import { RouteMap } from "./route-map";
import { SeriesEditor } from "./series-editor";
import { coursePosition } from "./course-position";
import {
  $,
  el,
  errorMessage,
  safely,
  on,
  notice,
  createInput,
  button,
  dialog,
  modalField,
} from "./dom";
import type {
  ApiReplies,
  Bootstrap,
  BuildJob,
  Project,
  RoadNetwork,
  RouteMark,
  Route,
  SavedView,
} from "./models";
import type { Point, Road } from "./road-levels";
const words = (s: string) =>
  s.replaceAll("_", " ").replace(/\b\w/g, (c) => c.toUpperCase());
const id = () => `m_${crypto.randomUUID().replaceAll("-", "")}`;
const copy = <T>(v: T): T => structuredClone(v);
let bootstrap: Bootstrap,
  network: RoadNetwork,
  project: Project,
  selected: string | null = null,
  undo: Project[] = [],
  redo: Project[] = [],
  saveTimer: ReturnType<typeof setTimeout> | undefined,
  jobTimer: ReturnType<typeof setInterval> | undefined;
let camera = { x: 0, y: 0, scale: 0.1 },
  roads: Road[] = [];
let mapMode = "draw",
  routeQueue = Promise.resolve(),
  pendingEdits = 0;
let pendingSpeedMps: number | null = null;
let guidancePoints: Point[] = [];
let guidanceKey = "";
let checkpointRanks = new Map<string, number>();
const canvas = $("#map");
const input = createInput(checkpoint, () => update(false));
const routeMap = new RouteMap(canvas, {
  ready: () => Boolean(project),
  camera: () => camera,
  setCamera: (value) => {
    camera = value;
  },
  marks: () => project?.route.marks || [],
  course: () => (project ? points() : []),
  guidance: () => guidancePoints,
  anchors: () => (project ? controls() : []),
  selected: () => selected,
  mode: () => mapMode,
  looped: () => project.route.looped,
  projectId: () => project.route.id,
  label: pointLabel,
  select: (mark, details) => {
    selected = mark.id;
    if (details) $("#selected-details").open = true;
    renderMarks();
    draw();
  },
  move: (mark, position, height, snap) =>
    queueEdit(async () => {
      if (mark.role === "speed_change") {
        checkpoint();
        mark.position = coursePosition(
          position,
          points().map((m) => m.position),
          project.route.looped,
          height ?? mark.position.z,
        ).position;
        update();
        return;
      }
      const p = snap
        ? (await api("snap", { position, height: height ?? position.z }))
            .position
        : position;
      if (mark.role === "spline_point") await reroute(mark, p);
      else {
        checkpoint();
        mark.position = p;
        update();
      }
    }),
  place: (position, role, height, snap) =>
    queueEdit(() => place(position, { role, height, snap })),
});
function api<K extends keyof ApiReplies>(
  path: K,
  value?: unknown,
): Promise<ApiReplies[K]>;
function api(path: `project/${string}`): Promise<Project>;
function api(path: `job/${string}`): Promise<BuildJob>;
function api(path: string, value?: unknown): Promise<unknown>;
async function api(path: string, value?: unknown): Promise<unknown> {
  for (let attempt = 0; attempt < 2; attempt++) {
    const r = await fetch(
      "/api/" + path,
      value === undefined
        ? {}
        : {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "X-Race-Token": bootstrap.token,
            },
            body: JSON.stringify(value),
          },
    );
    const data: unknown = await r.json();
    if (!r.ok) {
      const message =
        typeof data === "object" && data !== null && "error" in data
          ? String(data.error)
          : "Request failed";
      if (
        attempt === 0 &&
        message.includes("Reload the editor to renew its session")
      ) {
        bootstrap = await api("bootstrap");
        continue;
      }
      throw Error(message);
    }
    return data;
  }
  throw Error("Request failed after renewing the editor session");
}
function last<T>(values: readonly T[]): T {
  const value = values.at(-1);
  if (value === undefined) throw Error("Expected at least one route point");
  return value;
}
const seriesEditor = new SeriesEditor({
  request: api,
  projects: () => bootstrap.projects,
  openProject: async (id) => {
    await routeQueue;
    await flushSave();
    project = await api(`project/${id}`);
    selected = null;
    undo = [];
    redo = [];
    renderAll();
    fit();
    document.querySelector<HTMLButtonElement>('[data-tab="route"]')!.click();
  },
});
const showingSeries = () => !$("#series-view").hidden;
function checkpoint() {
  undo.push(copy(project));
  if (undo.length > 100) undo.shift();
  redo = [];
}
let saveQueue = Promise.resolve();
async function flushSave() {
  clearTimeout(saveTimer);
  const snapshot = copy(project);
  saveQueue = saveQueue
    .catch(() => {})
    .then(async () => {
      try {
        const r = await api("save", snapshot);
        bootstrap.projects = r.projects;
        if (project.route.id === snapshot.route.id) {
          $("#save-status").textContent = "Saved";
          renderProjects();
        }
      } catch (error) {
        $("#save-status").textContent = "Save failed";
        throw error;
      }
    });
  return saveQueue;
}
function update(render = true) {
  $("#save-status").textContent = "Saving…";
  clearTimeout(saveTimer);
  saveTimer = setTimeout(safely(flushSave), 600);
  if (render) renderAll();
  else draw();
}
function points() {
  return project.route.marks
    .filter((m) => m.role === "spline_point")
    .sort((a, b) => a.order - b.order);
}
function controls() {
  return points().filter((m) => !m.options?.generated_road_sample);
}
function setMode(mode: string) {
  mapMode = mode;
  $("#add-speed").setAttribute(
    "aria-pressed",
    String(mode === "place" && $("#role").value === "speed_change"),
  );
  if (mode === "draw") $("#role").value = "spline_point";
  for (const key of ["draw", "edit", "pan"])
    $("#mode-" + key).setAttribute("aria-pressed", String(mode === key));
  canvas.style.cursor =
    mode === "draw" || mode === "place"
      ? "crosshair"
      : mode === "pan"
        ? "grab"
        : "default";
  $("#map-hint").textContent =
    mode === "draw"
      ? "Click before and after turns to add route points. Drag empty space to pan."
      : mode === "edit"
        ? "Drag a route point to reshape the roads around it. Click a point for details."
        : mode === "pan"
          ? "Drag to explore the map. Scroll to zoom."
          : "Click the map to place " +
            words($("#role").value).toLowerCase() +
            ".";
  draw();
}
function queueEdit(action: () => void | Promise<void>) {
  pendingEdits++;
  $("#draw-state").textContent =
    "Following roads…" +
    (pendingEdits > 1 ? " " + pendingEdits + " points queued" : "");
  routeQueue = routeQueue
    .then(action)
    .catch((error) => notice(errorMessage(error), true))
    .finally(() => {
      pendingEdits--;
      $("#draw-state").textContent = pendingEdits
        ? "Following roads… " + pendingEdits + " points queued"
        : "";
    });
  return routeQueue;
}
function updateHeadings() {
  const course = points();
  for (let i = 0; i < course.length; i++) {
    const a = course[i]!.position,
      b = course[Math.min(i + 1, course.length - 1)]!.position;
    if (i < course.length - 1)
      course[i]!.yaw =
        ((Math.atan2(-(b.x - a.x), b.y - a.y) * 180) / Math.PI + 360) % 360;
    else if (i) course[i]!.yaw = course[i - 1]!.yaw;
  }
}
function roadSample(p: Point): RouteMark {
  return {
    id: id(),
    role: "spline_point",
    order: 0,
    name: "",
    notes: "",
    position: p,
    yaw: 0,
    options: { generated_road_sample: true },
    evidence: { source: "road-path" },
  };
}
async function reshape(
  anchors: RouteMark[],
  closed: boolean,
  selection: string | null = null,
  force = false,
) {
  const original = points(),
    rebuilt: RouteMark[] = [],
    snap = $("#snap").checked,
    height = $("#height").value === "" ? null : Number($("#height").value);
  for (let i = 0; i < anchors.length; i++) {
    const anchor = copy(anchors[i]!);
    if (i) {
      const previous = last(rebuilt),
        a = original.findIndex((m) => m.id === previous.id),
        b = original.findIndex((m) => m.id === anchor.id);
      const unchanged =
        a >= 0 &&
        b > a &&
        distance(original[a]!.position, previous.position) < 0.001 &&
        distance(original[b]!.position, anchor.position) < 0.001;
      if (unchanged && !force)
        rebuilt.push(...original.slice(a + 1, b).map(copy));
      else if (snap) {
        const route = await api("path", {
          start: previous.position,
          end: anchor.position,
          height,
        });
        rebuilt.push(...route.points.slice(1, -1).map(roadSample));
        anchor.position = last(route.points);
      }
    }
    anchor.options = {
      ...anchor.options,
      route_anchor: true,
      generated_road_sample: false,
    };
    rebuilt.push(anchor);
  }
  if (closed && snap && anchors.length >= 3) {
    const route = await api("path", {
      start: last(rebuilt).position,
      end: rebuilt[0]!.position,
      height,
    });
    rebuilt.push(...route.points.slice(1, -1).map(roadSample));
  }
  checkpoint();
  rebuilt.forEach((m, i) => (m.order = i));
  project.route.marks = [
    ...project.route.marks.filter((m) => m.role !== "spline_point"),
    ...rebuilt,
  ];
  project.route.looped = closed && anchors.length >= 3;
  updateHeadings();
  selected = selection;
  update();
}
async function reroute(moved: RouteMark, position: Point) {
  if (moved.options?.generated_road_sample) {
    checkpoint();
    moved.position = position;
    update();
    return;
  }
  await reshape(
    controls().map((a) => (a.id === moved.id ? { ...copy(a), position } : a)),
    project.route.looped,
    moved.id,
  );
}
function addMark(role: string, position: Point, yaw = 0): RouteMark {
  const marks = project.route.marks;
  const order =
    1 +
    Math.max(-1, ...marks.filter((m) => m.role === role).map((m) => m.order));
  const mark: RouteMark = {
    id: id(),
    role,
    order,
    name: "",
    notes: "",
    position: { ...position },
    yaw,
    options: {},
    evidence: { source: "map" },
  };
  marks.push(mark);
  selected = mark.id;
  return mark;
}
function newProject(): Project {
  const key = "race_" + crypto.randomUUID().replaceAll("-", "").slice(0, 12);
  return {
    route: {
      format: "ghostline-race-route",
      schema_version: 1,
      id: key,
      title: "Untitled race",
      description: "",
      prefix: "races",
      looped: false,
      width_default: 5,
      speed_mps: 100,
      marks: [],
    },
    config: {
      schema_version: 1,
      id: key,
      title: "Untitled race",
      description: "",
      route: "route.json",
      entrants: [],
      checkpoint_spacing_m: 40,
      countdown_seconds: 3,
      timeout_seconds: 300,
      checkpoint_mode: "vanilla",
    },
  };
}
function renderProjects() {
  const picker = $("#projects");
  picker.replaceChildren(new Option("Choose a saved race", ""));
  for (const p of bootstrap.projects) picker.append(new Option(p.title, p.id));
  picker.value = bootstrap.projects.some((p) => p.id === project.route.id)
    ? project.route.id
    : "";
}
function renderAll() {
  renderProjects();
  $("#title").textContent = project.config.title;
  $("#description").textContent =
    project.config.description ||
    "Draw a course along the roads, then place the start and finish.";
  $("#looped").checked = project.route.looped;
  renderMarks();
  renderEntrants();
  renderRules();
  draw();
}
function pointLabel(mark: RouteMark) {
  if (mark.role === "checkpoint") return `Checkpoint ${checkpointRanks.get(mark.id) ?? mark.order + 1}`;
  if (mark.role !== "spline_point")
    return words(mark.role) + " " + (mark.order + 1);
  const anchors = controls(),
    index = anchors.findIndex((m) => m.id === mark.id);
  return index === 0
    ? "Start"
    : index === anchors.length - 1 && !project.route.looped
      ? "End"
      : index > 0
        ? "Via point " + index
        : "Road sample " + (mark.order + 1);
}
function renderSpeedMarks() {
  const speedList = $("#speed-marks");
  speedList.replaceChildren();
  const course = points().map((m) => m.position);
  for (const m of project.route.marks
    .filter((m) => m.role === "speed_change")
    .map((mark) => ({
      mark,
      station:
        course.length < 2
          ? 0
          : coursePosition(
              mark.position,
              course,
              project.route.looped,
              mark.position.z,
            ).station,
    }))
    .sort((a, b) => a.station - b.station)) {
    button(
      speedList,
      `${Math.round((m.mark.options.speed_mps ?? project.route.speed_mps) * 3.6)} km/h · ${(m.station / 1000).toFixed(2)} km`,
      () => {
        selected = m.mark.id;
        setMode("edit");
        $("#selected-details").open = true;
        renderMarks();
        camera.x = m.mark.position.x;
        camera.y = m.mark.position.y;
        draw();
      },
      "mark" + (selected === m.mark.id ? " active" : ""),
    );
  }
}
function renderMarks() {
  let length = 0;
  const all = points();
  const checkpointMarks = project.route.marks.filter(m => m.role === "checkpoint");
  if (all.length > 1) {
    const course = all.map(m => m.position);
    const stations = new Map(checkpointMarks.map(m => [m.id, coursePosition(m.position, course,
      project.route.looped, m.position.z, typeof m.options.station_m === "number" ? m.options.station_m : null).station]));
    checkpointMarks.sort((a,b) => stations.get(a.id)! - stations.get(b.id)!);
  }
  checkpointRanks = new Map(checkpointMarks.map((m,i) => [m.id, i + 1]));
  for (let i = 1; i < all.length; i++)
    length += distance(all[i - 1]!.position, all[i]!.position);
  if (project.route.looped && all.length > 2)
    length += distance(last(all).position, all[0]!.position);
  $("#route-summary").textContent =
    `${(length / 1000).toFixed(2)} km · ${controls().length} route point${controls().length === 1 ? "" : "s"}`;
  $("#finish-drawing").disabled = controls().length < 2;
  $("#looped").disabled = controls().length < 3;
  $("#undo").disabled = !undo.length;
  $("#redo").disabled = !redo.length;
  $("#add-speed").setAttribute(
    "aria-pressed",
    String(mapMode === "place" && $("#role").value === "speed_change"),
  );
  const speedDefault = $("#default-speed");
  speedDefault.replaceChildren();
  input(
    speedDefault,
    "Starting target (km/h)",
    Math.round(project.route.speed_mps * 360) / 100,
    (v) => {
      if (v < 0) {
        notice("Speed cannot be negative.", true);
        return;
      }
      project.route.speed_mps = v / 3.6;
      renderSpeedMarks();
      renderRules();
    },
    { type: "number" },
  );
  renderSpeedMarks();
  const list = $("#marks");
  list.replaceChildren();
  const query = $("#mark-filter").value.toLowerCase();
  const marks = [...project.route.marks]
    .filter(
      (m) =>
        m.role !== "speed_change" &&
        ($("#show-samples").checked || !m.options?.generated_road_sample),
    )
    .sort((a, b) =>
      a.role === "spline_point" && b.role !== "spline_point"
        ? -1
        : b.role === "spline_point" && a.role !== "spline_point"
          ? 1
          : a.role.localeCompare(b.role) || (a.role === "checkpoint"
            ? checkpointRanks.get(a.id)! - checkpointRanks.get(b.id)! : a.order - b.order),
    )
    .filter((m) => `${m.name} ${pointLabel(m)}`.toLowerCase().includes(query));
  for (const m of marks) {
    button(
      list,
      `${pointLabel(m)}${m.name ? " · " + m.name : ""}`,
      () => {
        selected = m.id;
        setMode("edit");
        $("#selected-details").open = true;
        renderMarks();
        draw();
      },
      "mark" + (selected === m.id ? " active" : ""),
    );
  }
  $("#selected").replaceChildren();
  $("#speed-selected").replaceChildren();
  const mark = project.route.marks.find((m) => m.id === selected);
  if (!mark) return;
  const detail =
    mark.role === "speed_change" ? $("#speed-selected") : $("#selected");
  if (mark.role === "speed_change") {
    input(
      detail,
      "Target speed (km/h)",
      Math.round((mark.options.speed_mps ?? project.route.speed_mps) * 360) /
        100,
      (v) => {
        if (v < 0) {
          notice("Speed cannot be negative.", true);
          return;
        }
        mark.options.speed_mps = v / 3.6;
        renderSpeedMarks();
      },
      { type: "number" },
    );
    button(detail, "Move on map", () => {
      setMode("edit");
      camera.x = mark.position.x;
      camera.y = mark.position.y;
      camera.scale = Math.max(camera.scale, 2);
      draw();
    });
    button(
      detail,
      "Remove speed change",
      () => {
        checkpoint();
        project.route.marks = project.route.marks.filter(
          (m) => m.id !== mark.id,
        );
        selected = null;
        update();
      },
      "quiet danger",
    );
    detail.append(
      el(
        "p",
        "Drag the marker in Move points mode to change where this speed begins.",
        "hint",
      ),
    );
    return;
  }
  detail.append(el("h2", words(mark.role)));
  input(detail, "Name", mark.name, (v) => {
    mark.name = v;
  });
  const coords = el("div", undefined, "fields");
  for (const k of ["x", "y", "z"] as const)
    input(
      coords,
      k.toUpperCase(),
      mark.position[k],
      (v) => (mark.position[k] = v),
      { type: "number" },
    );
  if (mark.role === "checkpoint") input(coords, "Checkpoint arrow", String(mark.options.direction ?? "automatic"),
    (v) => { if (v === "automatic") delete mark.options.direction; else mark.options.direction = v; },
    {choices: [["From route", "automatic"], ["Straight ahead", "Forward"], ["Turn left", "Left"], ["Turn right", "Right"]]});
  if (mark.role === "checkpoint") input(coords, "Heading alignment", mark.options.align_to_route === false ? "custom" : "course",
    (v) => { mark.options.align_to_route = v !== "custom"; renderMarks(); },
    {choices: [["Across the course", "course"], ["Custom heading", "custom"]]});
  if (mark.role !== "checkpoint" || mark.options.align_to_route === false) input(coords, "Heading (°)", mark.yaw, (v) => (mark.yaw = v), {
    type: "number",
  });
  detail.append(coords);
  if (
    ["checkpoint", "staging", "finish", "hazard", "boundary"].includes(
      mark.role,
    )
  )
    for (const [k, label, defaultValue] of [
      ["width_m", "Width (m)", 10],
      ["depth_m", "Depth (m)", 2],
      ["height_m", "Height (m)", 4],
    ] as const)
      input(
        detail,
        label,
        mark.options[k] ?? (mark.role === "checkpoint" ?
          (k === "width_m" ? project.config.checkpoint_width_m ?? 12 : k === "depth_m" ? 4 : 6) : defaultValue),
        (v) => (mark.options[k] = v),
        { type: "number" },
      );
  if (mark.role === "width_change")
    for (const k of ["left_m", "right_m"] as const)
      input(
        detail,
        words(k),
        mark.options[k] ?? 5,
        (v) => (mark.options[k] = v),
        { type: "number" },
      );
  if (mark.role === "prop")
    input(
      detail,
      "Entity template",
      mark.options.entity_template || "",
      (v) => (mark.options.entity_template = v),
    );
  input(detail, "Notes", mark.notes || "", (v) => (mark.notes = v), {
    type: "textarea",
  });
  if (mark.role === "checkpoint") detail.append(el("p", "Checkpoint order follows the course.", "hint"));
  else {
    button(detail, "Move earlier", () => moveMark(mark, -1));
    button(detail, "Move later", () => moveMark(mark, 1));
  }
  button(detail, "Centre map here", () => {
    camera.x = mark.position.x;
    camera.y = mark.position.y;
    camera.scale = Math.max(camera.scale, 2);
    draw();
  });
  button(
    detail,
    "Delete point",
    () =>
      queueEdit(async () => {
        if (
          mark.role === "spline_point" &&
          !mark.options?.generated_road_sample
        )
          await reshape(
            controls().filter((m) => m.id !== mark.id),
            project.route.looped,
          );
        else {
          checkpoint();
          project.route.marks = project.route.marks.filter(
            (m) => m.id !== mark.id,
          );
          selected = null;
          update();
        }
      }),
    "quiet danger",
  );
}
function moveMark(mark: RouteMark, direction: number) {
  if (mark.role === "spline_point" && !mark.options?.generated_road_sample) {
    queueEdit(async () => {
      const anchors = controls(),
        index = anchors.indexOf(mark),
        other = index + direction;
      if (other < 0 || other >= anchors.length) return;
      [anchors[index], anchors[other]] = [anchors[other]!, anchors[index]!];
      await reshape(anchors, project.route.looped, mark.id);
    });
    return;
  }
  const same = project.route.marks
      .filter((m) => m.role === mark.role)
      .sort((a, b) => a.order - b.order),
    index = same.indexOf(mark),
    other = same[index + direction];
  if (!other) return;
  checkpoint();
  [mark.order, other.order] = [other.order, mark.order];
  update();
}
function renderEntrants() {
  const parent = $("#entrants");
  parent.replaceChildren();
  for (const [i, entry] of project.config.entrants.entries()) {
    const card = el("section", undefined, "entrant");
    card.append(el("h2", entry.name || `Opponent ${i + 1}`));
    const fields = el("div", undefined, "fields");
    input(fields, "Name", entry.name || "", (v) => (entry.name = v));
    const grids = project.route.marks
      .filter((m) => m.role === "start_grid")
      .sort((a, b) => a.order - b.order);
    input(
      fields,
      "Grid position",
      entry.grid,
      (v) => (entry.grid = Number(v)),
      {
        choices: grids.length
          ? grids.map((g) => [
              `${g.order + 1}${g.name ? " · " + g.name : ""}`,
              g.order,
            ])
          : [["Place a start grid mark", ""]],
      },
    );
    input(fields, "Vehicle record", entry.vehicle, (v) => (entry.vehicle = v), {
      list: "vehicle-records",
    });
    input(
      fields,
      "Vehicle appearance",
      entry.vehicle_appearance || "default",
      (v) => (entry.vehicle_appearance = v),
    );
    input(fields, "Driver record", entry.driver, (v) => (entry.driver = v));
    input(
      fields,
      "Driver appearance",
      entry.driver_appearance || "default",
      (v) => (entry.driver_appearance = v),
    );
    input(
      fields,
      "Passenger record",
      entry.passenger || entry.driver,
      (v) => (entry.passenger = v),
    );
    input(
      fields,
      "Passenger appearance",
      entry.passenger_appearance || "default",
      (v) => (entry.passenger_appearance = v),
    );
    card.append(fields);
    button(
      card,
      "Remove opponent",
      () => {
        checkpoint();
        project.config.entrants.splice(i, 1);
        update();
      },
      "quiet danger",
    );
    parent.append(card);
  }
  if (!project.config.entrants.length)
    parent.append(
      el(
        "p",
        "Add at least one opponent. Each opponent needs a distinct starting-grid mark.",
        "hint",
      ),
    );
  $("#force-player").checked = Boolean(project.config.player_vehicle);
  const player = $("#player-vehicle");
  player.replaceChildren();
  const playerVehicle = project.config.player_vehicle;
  if (playerVehicle) {
    input(
      player,
      "Player vehicle record",
      playerVehicle.record,
      (v) => (playerVehicle.record = v),
      { list: "vehicle-records" },
    );
    input(
      player,
      "Player vehicle appearance",
      playerVehicle.appearance || "default",
      (v) => (playerVehicle.appearance = v),
    );
  }
}
function renderRules() {
  const parent = $("#rules");
  parent.replaceChildren();
  parent.append(el("h2", "Course & race settings"));
  input(
    parent,
    "Race namespace",
    project.config.id,
    (v) => (project.config.id = v),
  );
  input(
    parent,
    "Route half-width (m)",
    project.route.width_default,
    (v) => (project.route.width_default = v),
    { type: "number" },
  );
  input(
    parent,
    "Starting target (m/s)",
    project.route.speed_mps,
    (v) => {
      project.route.speed_mps = v;
      renderMarks();
    },
    { type: "number" },
  );
  input(
    parent,
    "Checkpoint placement",
    project.config.checkpoint_placement ?? "automatic",
    (v) => { project.config.checkpoint_placement = v as "automatic" | "manual"; renderRules(); },
    { choices: [["Placed checkpoints only", "manual"], ["Spaced checkpoints + placed points", "automatic"]] },
  );
  if (project.config.checkpoint_placement !== "manual") input(
    parent,
    "Checkpoint spacing (m)",
    project.config.checkpoint_spacing_m ?? 40,
    (v) => (project.config.checkpoint_spacing_m = v),
    { type: "number" },
  );
  input(parent, "Default checkpoint width (m)", project.config.checkpoint_width_m ?? 12,
    (v) => (project.config.checkpoint_width_m = v), {type: "number"});
  input(parent, "Guidance line", project.config.guidance_alignment ?? "route",
    (v) => (project.config.guidance_alignment = v as "route" | "road_center"),
    {choices: [["Road centre", "road_center"], ["Route lane", "route"]]});
  input(
    parent,
    "Checkpoint style",
    project.config.checkpoint_mode || "vanilla",
    (v) => (project.config.checkpoint_mode = v),
    {
      choices: [
        ["Generated native checkpoints", "custom"],
        ["Shipped MQ056 checkpoints", "vanilla"],
      ],
    },
  );
  parent.append(
    el(
      "p",
      "Native race rules: first place wins; 10,000-eddy payout; off-course recovery at 50 m with a 2-second grace and 40 m return threshold. A closed spline is one course, not a multi-lap race.",
      "hint",
    ),
  );
  parent.append(el("h2", "Advanced course authoring"));
  parent.append(
    el(
      "p",
      "Water areas, dressing, finish parking, courier paths, stopping areas and individual staging transforms remain available in the complete JSON editor.",
      "hint",
    ),
  );
}
function distance(a: Point, b: Point) {
  return Math.hypot(a.x - b.x, a.y - b.y, a.z - b.z);
}
function draw() {
  if (project) {
    const key = JSON.stringify([project.config.guidance_alignment, points().map(m => m.position)]);
    if (key !== guidanceKey) {
      guidanceKey = key;
      guidancePoints = [];
      if (project.config.guidance_alignment === "road_center" && points().length > 1) {
        void api("guidance", {route: project.route}).then(result => {
          if (key === guidanceKey) { guidancePoints = result.points; routeMap.draw(); }
        }).catch(error => notice(errorMessage(error), true));
      }
    }
  }
  routeMap.draw();
}
function fit(city = false) {
  routeMap.fit(city);
}
function prepareRoads() {
  prepareRoadData();
  routeMap.setRoads(roads, network.map_display.bridges);
}
async function place(
  position: Point,
  options: { role?: string; height?: number | null; snap?: boolean } = {},
) {
  const role = options.role || $("#role").value,
    height =
      options.height ??
      ($("#height").value === "" ? null : Number($("#height").value)),
    snap = options.snap ?? $("#snap").checked;
  if (role === "speed_change") {
    const projected = coursePosition(
      position,
      points().map((m) => m.position),
      project.route.looped,
      height,
    );
    checkpoint();
    const mark = addMark(role, projected.position);
    mark.options.speed_mps = pendingSpeedMps ?? project.route.speed_mps;
    pendingSpeedMps = null;
    setMode("edit");
    $("#selected-details").open = true;
    $("#notice").hidden = true;
    update();
    return;
  }
  if (role === "spline_point" && project.route.looped)
    throw Error("Uncheck Join back to start before extending the route.");
  if (snap && role === "spline_point") {
    const route = points();
    if (route.length) {
      const result = await api("path", {
        start: last(route).position,
        end: position,
        height,
      });
      const additions = result.points
        .slice(1)
        .filter((p) => distance(last(route).position, p) >= 0.05);
      if (!additions.length) return;
      checkpoint();
      for (const p of additions) {
        const m = addMark(role, p);
        m.options.generated_road_sample = true;
        m.evidence.source = "road-path";
      }
      const endpoint = last(points());
      endpoint.options = {
        ...endpoint.options,
        route_anchor: true,
        generated_road_sample: false,
      };
      endpoint.evidence.source = "map";
    } else {
      const result = await api("snap", { position, height });
      checkpoint();
      addMark(role, result.position).options.route_anchor = true;
    }
  } else {
    if (snap) position = (await api("snap", { position, height })).position;
    if (
      ["finish", "player_start", "staging", "activation"].includes(role) &&
      project.route.marks.some((m) => m.role === role)
    )
      throw Error(
        `Use Move points to adjust the existing ${words(role).toLowerCase()}.`,
      );
    checkpoint();
    const m = addMark(role, position);
    if (role === "spline_point") m.options.route_anchor = true;
  }
  updateHeadings();
  $("#notice").hidden = true;
  update();
}
on("#undo", "click", async () => {
  await routeQueue;
  if (!undo.length) return;
  redo.push(copy(project));
  project = undo.pop()!;
  selected = null;
  update();
});
on("#redo", "click", async () => {
  await routeQueue;
  if (!redo.length) return;
  undo.push(copy(project));
  project = redo.pop()!;
  selected = null;
  update();
});
on("#fit", "click", () => fit());
on("#fit-network", "click", () => fit(true));
on("#height", "input", draw);
on("#mark-filter", "input", renderMarks);
on("#show-samples", "change", () => {
  renderMarks();
  draw();
});
on("#looped", "change", () => {
  const closed = $("#looped").checked;
  queueEdit(async () => {
    try {
      await reshape(controls(), closed);
      if (closed) setMode("edit");
    } catch (error) {
      $("#looped").checked = project.route.looped;
      throw error;
    }
  });
});
for (const mode of ["draw", "edit", "pan"])
  on("#mode-" + mode, "click", () => setMode(mode));
on("#add-speed", "click", async () => {
  if (points().length < 2)
    throw Error("Draw a course before adding speed changes.");
  const target = await dialog("Add speed change", (parent) => {
    parent.append(
      el(
        "p",
        "Choose the opponents’ target speed, then click where it should begin on the existing course.",
      ),
    );
    const speed = modalField(
      parent,
      "Target speed (km/h)",
      String(Math.round(project.route.speed_mps * 3.6)),
      "number",
    );
    speed.setAttribute("min", "0");
    speed.setAttribute("step", "any");
    return () => {
      const value = Number(speed.value);
      if (!speed.value || !Number.isFinite(value) || value < 0)
        throw Error("Enter a non-negative target speed.");
      return value / 3.6;
    };
  });
  if (target === null) return;
  pendingSpeedMps = target;
  $("#role").value = "speed_change";
  setMode("place");
  $("#map-hint").textContent =
    `Click the course to begin ${Math.round(target * 3.6)} km/h. Drag empty space to pan.`;
});
on("#role", "change", () =>
  setMode($("#role").value === "spline_point" ? "draw" : "place"),
);
on("#rebuild-roads", "click", () =>
  queueEdit(() => reshape(controls(), project.route.looped, selected, true)),
);
on("#finish-drawing", "click", async () => {
  await routeQueue;
  setMode("edit");
  notice("Course ready. Set up the starting grid, then choose your vehicles.");
});
on("#next-vehicles", "click", () =>
  document.querySelector<HTMLButtonElement>('[data-tab="grid"]')!.click(),
);
on("#new", "click", async () => {
  await routeQueue;
  await flushSave();
  project = newProject();
  undo = [];
  redo = [];
  selected = null;
  $("#selected-details").open = false;
  setMode("draw");
  update();
});
on("#projects", "change", async () => {
  const key = $("#projects").value;
  if (!key) return;
  await routeQueue;
  await flushSave();
  project = await api(`project/${key}`);
  undo = [];
  redo = [];
  selected = null;
  setMode("edit");
  renderAll();
  fit();
});
on("#edit-title", "click", async () => {
  const result = await dialog("Race title and description", (p) => {
    const title = modalField(p, "Title", project.config.title),
      desc = modalField(
        p,
        "Description",
        project.config.description,
        "textarea",
      );
    return () => ({ title: title.value.trim(), description: desc.value });
  });
  if (!result) return;
  if (!result.title) throw Error("Enter a title");
  checkpoint();
  Object.assign(project.config, result);
  Object.assign(project.route, result);
  update();
});
on("#add-entrant", "click", () => {
  if (project.config.entrants.length >= 12)
    throw Error("A race supports at most 12 opponents");
  checkpoint();
  const n = project.config.entrants.length + 1;
  project.config.entrants.push({
    id: "opponent_" + id().slice(2, 10),
    name: "Opponent " + n,
    grid: n - 1,
    vehicle: "Vehicle.v_standard2_archer_quartz",
    driver: "Character.sq024_city_race_corpo_racer",
  });
  update();
});
on("#force-player", "change", () => {
  checkpoint();
  if ($("#force-player").checked)
    project.config.player_vehicle = {
      record: "Vehicle.v_standard2_archer_quartz",
      appearance: "default",
    };
  else delete project.config.player_vehicle;
  update();
});
on("#layout", "click", async () => {
  await routeQueue;
  const route = points();
  if (route.length < 2) throw Error("Draw at least two course points first");
  if (
    project.route.marks.some(
      (m) => m.role === "start_grid" || m.role === "activation",
    )
  )
    throw Error(
      "The route already has starting marks. Edit those marks or remove them before laying out a new grid.",
    );
  checkpoint();
  const a = route[0]!.position,
    b = route[1]!.position,
    yaw = ((Math.atan2(-(b.x - a.x), b.y - a.y) * 180) / Math.PI + 360) % 360;
  const angle = (yaw * Math.PI) / 180;
  const offset = (right: number, back: number) => ({
    x: a.x + Math.cos(angle) * right + Math.sin(angle) * back,
    y: a.y + Math.sin(angle) * right - Math.cos(angle) * back,
    z: a.z,
  });
  addMark("player_start", offset(0, 0), yaw);
  for (let i = 0; i < Math.max(1, project.config.entrants.length); i++)
    addMark(
      "start_grid",
      offset((i % 2 ? 1 : -1) * 3, 5 + Math.floor(i / 2) * 6),
      yaw,
    );
  addMark("activation", offset(7, 8), yaw);
  addMark("staging", offset(7, 8), yaw);
  if (!project.route.marks.some((m) => m.role === "finish"))
    addMark(
      "finish",
      (project.route.looped ? route[0]! : last(route)).position,
      (project.route.looped ? route[0]! : last(route)).yaw,
    );
  if (!project.route.marks.some((m) => m.role === "checkpoint"))
    addMark("checkpoint", route[Math.floor(route.length / 2)]!.position, yaw);
  update();
});
for (const b of document.querySelectorAll<HTMLButtonElement>("[data-tab]"))
  b.onclick = () => {
    document
      .querySelectorAll<HTMLButtonElement>("[data-tab]")
      .forEach((x) => x.classList.toggle("active", x === b));
    for (const key of ["route", "grid", "rules", "series"])
      $("#" + key + "-view").hidden = key !== b.dataset.tab;
    document.querySelector<HTMLElement>(".heading")!.hidden =
      b.dataset.tab === "series";
    if (b.dataset.tab === "series") seriesEditor.render();
    $("#projects").hidden = b.dataset.tab === "series";
    $("#new").hidden = b.dataset.tab === "series";
    $("#build").textContent =
      b.dataset.tab === "series" ? "Build championship" : "Build";
    $("#check").textContent =
      b.dataset.tab === "series" ? "Check championship" : "Check race";
    draw();
  };
function download(value: unknown, name: string) {
  const url = URL.createObjectURL(
      new Blob([JSON.stringify(value, null, 2)], { type: "application/json" }),
    ),
    a = el("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 5000);
}
on("#export", "click", async () => {
  await routeQueue;
  await flushSave();
  download(project.route, project.route.id + ".route.json");
});
on("#export-spec", "click", async () => {
  await routeQueue;
  await flushSave();
  download(
    { ...project.config, route: project.route.id + ".route.json" },
    project.config.id + ".race.json",
  );
});
async function upload(): Promise<File | undefined> {
  const input = $("#file");
  input.value = "";
  return await new Promise<File | undefined>((resolve) => {
    const finish = (file: File | undefined) => {
      input.removeEventListener("change", change);
      input.removeEventListener("cancel", cancel);
      resolve(file);
    };
    const change = () => finish(input.files?.[0]),
      cancel = () => finish(undefined);
    input.addEventListener("change", change, { once: true });
    input.addEventListener("cancel", cancel, { once: true });
    input.click();
  });
}
on("#import", "click", async () => {
  await routeQueue;
  const file = await upload();
  if (!file) return;
  const data: unknown = JSON.parse(await file.text());
  const route = await api("import", data);
  await flushSave();
  checkpoint();
  project = {
    route,
    config: {
      ...newProject().config,
      id: route.id,
      title: route.title,
      description: route.description,
    },
  };
  selected = null;
  update();
  fit();
  notice(
    "Route imported. Choose opponents and check the starting grid before building.",
  );
});
function prepareRoadData() {
  const lanes = network.roads.filter((r) => r.drivable !== false),
    ids = new Set(lanes.map((r) => r.id));
  const connections = (network.connections || []).flatMap((c, i) =>
    ids.has(c.from) && ids.has(c.to) && c.from_position && c.to_position
      ? [{ id: "connection_" + i, points: [c.from_position, c.to_position] }]
      : [],
  );
  roads = [...lanes, ...connections]
    .filter((r) => r.points.length > 1)
    .map((r) => {
      const points = r.points.map((p) =>
        Array.isArray(p) ? { x: p[0], y: p[1], z: p[2] } : p,
      );
      return {
        ...r,
        points,
        bounds: [
          Math.min(...points.map((p) => p.x)),
          Math.min(...points.map((p) => p.y)),
          Math.max(...points.map((p) => p.x)),
          Math.max(...points.map((p) => p.y)),
        ],
      };
    });
  $("#network-summary").textContent =
    network.topology === "native-traffic"
      ? `Native traffic network · ${lanes.length.toLocaleString()} drivable lanes · ${connections.length.toLocaleString()} junction connections${network.failures?.length ? " · " + network.failures.length + " reported failures" : ""}`
      : `Mesh fragments · ${network.roads.length.toLocaleString()} pieces · partial road coverage`;
  $("#snap").checked = roads.length > 0;
}
on("#import-roads", "click", async () => {
  const file = await upload();
  if (!file) return;
  const data: unknown = JSON.parse(await file.text());
  await api("roads", data);
  network = await api("roads");
  prepareRoads();
  fit();
});
on("#raw", "click", async () => {
  await routeQueue;
  const result = await dialog("Complete race authoring", (p) => {
    p.append(
      el(
        "p",
        "This edits the same route and settings, including all native layout overrides.",
      ),
    );
    const text = modalField(
      p,
      "Route and settings JSON",
      JSON.stringify(project, null, 2),
      "textarea",
    );
    text.style.minHeight = "45vh";
    return () => JSON.parse(text.value) as unknown;
  });
  if (!result) return;
  await api("save", result);
  checkpoint();
  // The server validates the complete authoring document before it becomes state.
  project = result as Project;
  selected = null;
  update();
});
on("#tool-settings", "click", async () => {
  const result = await dialog("Tool settings", (p) => {
    const schema = modalField(p, "Reflection override (optional)", bootstrap.settings.schema),
      game = modalField(p, "Game directory", bootstrap.settings.game);
    return () => ({ schema: schema.value.trim(), game: game.value.trim() });
  });
  if (result) {
    await api("settings", result);
    bootstrap.settings = result;
  }
});
on("#check", "click", async () => {
  if (showingSeries()) return seriesEditor.check();
  await routeQueue;
  const result = await api("check", project);
  notice(
    `Race authoring is valid · ${result.marks} marks. Road accessibility and native runtime behavior still need in-game verification.`,
  );
});
async function build(native: boolean) {
  if (showingSeries()) return seriesEditor.build(native);
  await routeQueue;
  await flushSave();
  const result = await api("build", { ...project, native });
  $("#build").disabled = true;
  notice("Generating race resources…");
  clearInterval(jobTimer);
  jobTimer = setInterval(async () => {
    try {
      const job = await api(`job/${result.job}`);
      if (job.status === "running") return;
      clearInterval(jobTimer);
      $("#build").disabled = false;
      notice(
        job.status === "complete"
          ? `${job.native ? "Native build" : "Resource documents"} complete · ${job.resources} resources`
          : job.error,
        job.status === "failed",
      );
      if (job.status === "complete" && job.native) {
        const link = el("a", "Download mod ZIP", "download");
        link.href = "/api/job/" + result.job + "/download";
        link.style.color = "inherit";
        $("#notice").append(document.createTextNode(" · "), link);
      }
    } catch (error) {
      clearInterval(jobTimer);
      $("#build").disabled = false;
      notice(errorMessage(error), true);
    }
  }, 1000);
}
on("#build", "click", () => build(true));
on("#docs-build", "click", () => build(false));
async function boot() {
  const controls = [
    ...document.querySelectorAll<
      HTMLButtonElement | HTMLInputElement | HTMLSelectElement
    >("button,input,select"),
  ];
  controls.forEach((c) => (c.disabled = true));
  canvas.style.pointerEvents = "none";
  $("#save-status").textContent = "Loading roads…";
  bootstrap = await api("bootstrap");
  for (const role of bootstrap.roles)
    if (role !== "spline_point")
      $("#role").append(new Option(words(role), role));
  network = await api("roads");
  prepareRoads();
  const vehicles = await api("vehicles");
  for (const v of vehicles) {
    const record = v.record || v.record_id || v.id;
    if (typeof record === "string" && record.startsWith("Vehicle.")) {
      const option = new Option(v.name || record, record);
      $("#vehicle-records").append(option);
    }
  }
  let saved: SavedView | null = null;
  try {
    saved = JSON.parse(
      sessionStorage.getItem("race-view") || "null",
    ) as SavedView | null;
  } catch {}
  const key = bootstrap.projects.some((p) => p.id === saved?.project)
    ? saved!.project
    : bootstrap.projects[0]?.id;
  project = key ? await api(`project/${key}`) : newProject();
  controls.forEach((c) => (c.disabled = false));
  canvas.style.pointerEvents = "";
  if (
    saved?.project === project.route.id &&
    ["x", "y", "scale"].every((k) =>
      Number.isFinite(saved.camera?.[k as keyof typeof saved.camera]),
    ) &&
    saved.camera.scale > 0
  )
    camera = saved.camera;
  renderAll();
  if (saved?.project !== project.route.id) fit();
  setMode(points().length ? "edit" : "draw");
  await seriesEditor.init();
  $("#save-status").textContent = "Ready";
}
boot().catch((e) => notice(errorMessage(e), true));

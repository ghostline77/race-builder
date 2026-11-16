import { useLayoutEffect, useRef } from "react";
import { createRoot } from "react-dom/client";
import {
  initialize,
  editor,
  selectTab,
  seriesEditor,
  Entrants,
  Rules,
  SpeedMarks,
  MarkPanel,
} from "./editor";
import { view, actions, changed, useEditorState } from "./state";
import { Modal } from "./Modal";
import { AppShell, Button, Input, Select } from "@ghostline/ui-core";
import "./style.css";

function RaceBuilder() {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEditorState();
  useLayoutEffect(() => {
    initialize(canvas.current!);
  }, []);
  const { project, bootstrap, undo, redo, vehicles, anchorCount } = editor();
  const invoke = (name: string) => () => void actions[name]?.();
  return (
    <AppShell
      product="Race builder"
      project={
        <>
          <Select
            id="projects"
            aria-label="Race project"
            disabled={!view.ready}
            hidden={view.tab === "series"}
            value={
              bootstrap?.projects.some((p) => p.id === project?.route.id)
                ? project.route.id
                : ""
            }
            onChange={invoke("projects")}
          >
            <option value="">Choose a saved race</option>
            {bootstrap?.projects.map((p) => (
              <option key={p.id} value={p.id}>
                {p.title}
              </option>
            ))}
          </Select>
          <span id="save-status" role="status">
            {view.status}
          </span>
        </>
      }
      actions={
        <>
          <Button
            id="new"
            onClick={invoke("new")}
            hidden={view.tab === "series"}
            disabled={!view.ready}
          >
            New race
          </Button>
          <Button id="check" onClick={invoke("check")} disabled={!view.ready}>
            {view.tab === "series" ? "Check championship" : "Check race"}
          </Button>
          <Button
            id="build"
            onClick={invoke("build")}
            disabled={!view.ready || view.busy}
            variant="primary"
          >
            {view.tab === "series" ? "Build championship" : "Build"}
          </Button>
          <details className="menu">
            <summary>Advanced</summary>
            <div>
              {[
                ["import", "Import route / Game Scout export"],
                ["export", "Export shared route"],
                ["export-spec", "Export race settings"],
                ["import-roads", "Load road network"],
                ["raw", "Edit complete route & settings"],
                ["tool-settings", "Tool settings"],
                ["docs-build", "Generate resource documents"],
              ].map(([id, label]) => (
                <Button
                  key={id}
                  id={id}
                  disabled={!view.ready}
                  onClick={invoke(id!)}
                >
                  {label}
                </Button>
              ))}
            </div>
          </details>
        </>
      }
    >
      <nav aria-label="Race sections">
        <Button
          data-tab="route"
          disabled={!view.ready}
          onClick={() => selectTab("route")}
          className={view.tab === "route" ? "active" : ""}
        >
          Route
        </Button>
        <Button
          data-tab="grid"
          disabled={!view.ready}
          onClick={() => selectTab("grid")}
          className={view.tab === "grid" ? "active" : ""}
        >
          Vehicles & grid
        </Button>
        <Button
          data-tab="rules"
          disabled={!view.ready}
          onClick={() => selectTab("rules")}
          className={view.tab === "rules" ? "active" : ""}
        >
          Race settings
        </Button>
        <Button
          data-tab="series"
          disabled={!view.ready}
          onClick={() => selectTab("series")}
          className={view.tab === "series" ? "active" : ""}
        >
          Championship
        </Button>
      </nav>
      <main>
        <div className="heading" hidden={view.tab === "series"}>
          <div>
            <h1 id="title">{project?.config.title || "Untitled race"}</h1>
            <p id="description">
              {project?.config.description ||
                "Draw a course along the roads, then place the start and finish."}
            </p>
          </div>
          <Button
            id="edit-title"
            onClick={invoke("edit-title")}
            disabled={!view.ready}
          >
            Edit title & description
          </Button>
        </div>
        <section id="route-view" hidden={view.tab !== "route"}>
          <div className="map-tools">
            <div className="tool-group" aria-label="Map tools">
              <Button
                id="mode-draw"
                onClick={invoke("mode-draw")}
                disabled={!view.ready}
                aria-pressed={view.mode === "draw"}
              >
                Draw route
              </Button>
              <Button
                id="mode-edit"
                onClick={invoke("mode-edit")}
                disabled={!view.ready}
                aria-pressed={view.mode === "edit"}
              >
                Move points
              </Button>
              <Button
                id="mode-pan"
                onClick={invoke("mode-pan")}
                disabled={!view.ready}
                aria-pressed={view.mode === "pan"}
              >
                Pan map
              </Button>
            </div>
            <label className="check">
              <Input
                id="snap"
                type="checkbox"
                checked={view.snap}
                onChange={(e) => {
                  view.snap = e.target.checked;
                  changed();
                }}
              />{" "}
              Follow roads
            </label>
            <label className="road-height-control">
              Road height (m, ±2)
              <Input
                id="height"
                onInput={invoke("height")}
                type="number"
                placeholder="All levels"
                step="any"
              />
            </label>
            <Button id="all-levels">All levels</Button>
            <div className="map-actions">
              <Button
                id="undo"
                onClick={invoke("undo")}
                disabled={!view.ready || !undo.length}
              >
                Undo
              </Button>
              <Button
                id="redo"
                onClick={invoke("redo")}
                disabled={!view.ready || !redo.length}
              >
                Redo
              </Button>
              <Button id="fit" onClick={invoke("fit")} disabled={!view.ready}>
                Fit route
              </Button>
              <Button
                id="fit-network"
                onClick={invoke("fit-network")}
                disabled={!view.ready}
              >
                Show city
              </Button>
            </div>
          </div>
          <div className="map-layout">
            <div className="map-wrap">
              <canvas
                id="map"
                ref={canvas}
                tabIndex={0}
                aria-label="Road network and race route"
                aria-describedby="map-hint"
              />
              <p id="map-hint">{view.hint}</p>
              <div id="draw-state" role="status">
                {view.progress}
              </div>
              <div id="coordinates">World coordinates · metres</div>
              <section
                id="crossing-levels"
                className="crossing-levels"
                aria-label="Road levels at crossing"
                hidden
              >
                <h3>Bridge / underpass</h3>
                <p id="crossing-gap" />
                <Button id="focus-upper">Upper road</Button>
                <Button id="focus-lower">Lower road</Button>
              </section>
              <div id="map-elevation" className="map-elevation" hidden>
                <span>Road elevation</span>
                <div className="elevation-ramp" />
                <div className="elevation-values">
                  <span id="elevation-low" />
                  <span id="elevation-high" />
                </div>
                <small id="bridge-status">Preparing bridge details…</small>
              </div>
              <div id="map-empty" hidden>
                Load a road network under Advanced to draw along roads.
              </div>
            </div>
            <aside>
              <h2>1. Draw your course</h2>
              <p id="route-summary">
                {project && <MarkPanel part="summary" />}
              </p>
              <p className="hint">
                Click before and after a turn. The route follows the roads
                between your points.
              </p>
              <Button
                id="finish-drawing"
                onClick={invoke("finish-drawing")}
                disabled={!view.ready || anchorCount < 2}
              >
                Finish drawing
              </Button>
              <label className="check">
                <Input
                  id="looped"
                  checked={project?.route.looped ?? false}
                  disabled={anchorCount < 3}
                  onChange={invoke("looped")}
                  type="checkbox"
                />{" "}
                Join back to start
              </label>
              <h3>Opponent speed</h3>
              <div id="default-speed">
                {project && <MarkPanel part="speed" />}
              </div>
              <Button
                id="add-speed"
                onClick={invoke("add-speed")}
                aria-pressed={
                  view.mode === "place" && view.role === "speed_change"
                }
                disabled={!view.ready}
              >
                Add speed change
              </Button>
              <p className="hint">
                Choose a target speed, then click the course. It applies from
                that point until the next speed change.
              </p>
              <div id="speed-marks">{project && <SpeedMarks />}</div>
              <div id="speed-selected">
                {project && <MarkPanel part="speed-selected" />}
              </div>
              <h3>Your route points</h3>
              <Input
                id="mark-filter"
                value={view.filter}
                onChange={(e) => {
                  view.filter = e.target.value;
                  changed();
                }}
                type="search"
                placeholder="Find route points…"
                aria-label="Find route points"
              />
              <div id="marks">{project && <MarkPanel part="list" />}</div>
              <details
                id="selected-details"
                open={view.details}
                onToggle={(e) => {
                  view.details = e.currentTarget.open;
                }}
              >
                <summary>Selected point details</summary>
                <div id="selected">
                  {project && <MarkPanel part="selected" />}
                </div>
              </details>
              <h2>2. Start & finish</h2>
              <Button
                id="layout"
                onClick={invoke("layout")}
                disabled={!view.ready}
              >
                Set up starting grid
              </Button>
              <p className="hint">
                Creates the start, finish and meeting point. Use Move points to
                adjust their positions.
              </p>
              <Button
                id="next-vehicles"
                onClick={invoke("next-vehicles")}
                disabled={!view.ready}
                variant="primary"
              >
                Choose vehicles
              </Button>
              <details className="map-advanced">
                <summary>Advanced map controls</summary>
                <label>
                  Place a technical mark
                  <Select
                    id="role"
                    value={view.role}
                    onChange={(e) => {
                      view.role = e.target.value;
                      void actions.role?.();
                    }}
                  >
                    {(bootstrap?.roles ?? ["spline_point"]).map((role) => (
                      <option key={role} value={role}>
                        {role.replaceAll("_", " ")}
                      </option>
                    ))}
                  </Select>
                </label>
                <label>
                  Free-point height (m)
                  <Input
                    id="free-z"
                    type="number"
                    defaultValue="0"
                    step="any"
                  />
                </label>
                <label className="check">
                  <Input
                    id="show-samples"
                    checked={view.samples}
                    onChange={(e) => {
                      view.samples = e.target.checked;
                      void actions["show-samples"]?.();
                    }}
                    type="checkbox"
                  />{" "}
                  Show generated road samples
                </label>
                <button
                  id="rebuild-roads"
                  onClick={invoke("rebuild-roads")}
                  disabled={!view.ready}
                >
                  Rebuild road sections
                </button>
                <p className="hint" id="network-summary">
                  {view.network}
                </p>
              </details>
            </aside>
          </div>
        </section>
        <section id="grid-view" hidden={view.tab !== "grid"}>
          <div className="section-title">
            <div>
              <h2>Opponent grid</h2>
              <p>
                Choose the vehicle and crew for each surveyed starting position.
              </p>
            </div>
            <Button
              id="add-entrant"
              onClick={invoke("add-entrant")}
              disabled={!view.ready}
              variant="primary"
            >
              Add opponent
            </Button>
          </div>
          <datalist id="vehicle-records">
            {vehicles.map((v) => {
              const id = v.record ?? v.record_id ?? v.id;
              return id?.startsWith("Vehicle.") ? (
                <option key={id} value={id}>
                  {v.name ?? id}
                </option>
              ) : null;
            })}
          </datalist>
          <div id="entrants">{project && <Entrants />}</div>
          <section className="panel">
            <h2>Player vehicle</h2>
            <label className="check">
              <Input
                id="force-player"
                checked={Boolean(project?.config.player_vehicle)}
                onChange={invoke("force-player")}
                type="checkbox"
              />{" "}
              Supply a specific vehicle for this race
            </label>
            <div id="player-vehicle">{project && <Entrants playerOnly />}</div>
            <p className="hint">
              When disabled, the race uses the player's currently selected owned
              vehicle. Supplying a vehicle does not grant ownership.
            </p>
          </section>
        </section>
        <section id="rules-view" hidden={view.tab !== "rules"}>
          <div id="rules">{project && <Rules />}</div>
        </section>
        <section id="series-view" hidden={view.tab !== "series"}>
          <div id="series-editor">
            {view.tab === "series" && seriesEditor.render()}
          </div>
        </section>
        <div
          id="notice"
          role="status"
          className={view.error ? "error" : ""}
          hidden={!view.message}
        >
          {view.message}{" "}
          {view.download && <a href={view.download}>Download mod ZIP</a>}
        </div>
      </main>
      <Modal />
    </AppShell>
  );
}
document.body.classList.add("gl-root");
createRoot(document.getElementById("root")!).render(<RaceBuilder />);

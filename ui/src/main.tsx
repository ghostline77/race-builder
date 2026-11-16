import { AppShell, Button, Input, Select, mountApp } from "@ghostline/ui-core";
import "./style.css";

function RaceBuilder() {
  return (
    <AppShell
      product="Race builder"
      project={
        <>
          <Select id="projects" aria-label="Race project" />
          <span id="save-status" role="status">
            Ready
          </span>
        </>
      }
      actions={
        <>
          <Button id="new">New race</Button>
          <Button id="check">Check race</Button>
          <Button id="build" variant="primary">
            Build
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
                <Button key={id} id={id}>
                  {label}
                </Button>
              ))}
            </div>
          </details>
        </>
      }
    >
      <nav aria-label="Race sections">
        <Button data-tab="route" className="active">
          Route
        </Button>
        <Button data-tab="grid">Vehicles & grid</Button>
        <Button data-tab="rules">Race settings</Button>
        <Button data-tab="series">Championship</Button>
      </nav>
      <main>
        <div className="heading">
          <div>
            <h1 id="title">Untitled race</h1>
            <p id="description">
              Draw a course along the roads, then place the start and finish.
            </p>
          </div>
          <Button id="edit-title">Edit title & description</Button>
        </div>
        <section id="route-view">
          <div className="map-tools">
            <div className="tool-group" aria-label="Map tools">
              <Button id="mode-draw" aria-pressed="true">
                Draw route
              </Button>
              <Button id="mode-edit" aria-pressed="false">
                Move points
              </Button>
              <Button id="mode-pan" aria-pressed="false">
                Pan map
              </Button>
            </div>
            <label className="check">
              <Input id="snap" type="checkbox" defaultChecked /> Follow roads
            </label>
            <label className="road-height-control">
              Road height (m, ±2)
              <Input
                id="height"
                type="number"
                placeholder="All levels"
                step="any"
              />
            </label>
            <Button id="all-levels">All levels</Button>
            <div className="map-actions">
              <Button id="undo">Undo</Button>
              <Button id="redo">Redo</Button>
              <Button id="fit">Fit route</Button>
              <Button id="fit-network">Show city</Button>
            </div>
          </div>
          <div className="map-layout">
            <div className="map-wrap">
              <canvas
                id="map"
                tabIndex={0}
                aria-label="Road network and race route"
                aria-describedby="map-hint"
              />
              <p id="map-hint">
                Click roads to add route points. Drag empty space to pan. Scroll
                to zoom.
              </p>
              <div id="draw-state" role="status" />
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
              <p id="route-summary" />
              <p className="hint">
                Click before and after a turn. The route follows the roads
                between your points.
              </p>
              <Button id="finish-drawing">Finish drawing</Button>
              <label className="check">
                <Input id="looped" type="checkbox" /> Join back to start
              </label>
              <h3>Opponent speed</h3>
              <div id="default-speed" />
              <Button id="add-speed">Add speed change</Button>
              <p className="hint">
                Choose a target speed, then click the course. It applies from
                that point until the next speed change.
              </p>
              <div id="speed-marks" />
              <div id="speed-selected" />
              <h3>Your route points</h3>
              <Input
                id="mark-filter"
                type="search"
                placeholder="Find route points…"
                aria-label="Find route points"
              />
              <div id="marks" />
              <details id="selected-details">
                <summary>Selected point details</summary>
                <div id="selected" />
              </details>
              <h2>2. Start & finish</h2>
              <Button id="layout">Set up starting grid</Button>
              <p className="hint">
                Creates the start, finish and meeting point. Use Move points to
                adjust their positions.
              </p>
              <Button id="next-vehicles" variant="primary">
                Choose vehicles
              </Button>
              <details className="map-advanced">
                <summary>Advanced map controls</summary>
                <label>
                  Place a technical mark
                  <Select id="role">
                    <option value="spline_point">Route point</option>
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
                  <Input id="show-samples" type="checkbox" /> Show generated
                  road samples
                </label>
                <button id="rebuild-roads">Rebuild road sections</button>
                <p className="hint" id="network-summary" />
              </details>
            </aside>
          </div>
        </section>
        <section id="grid-view" hidden>
          <div className="section-title">
            <div>
              <h2>Opponent grid</h2>
              <p>
                Choose the vehicle and crew for each surveyed starting position.
              </p>
            </div>
            <Button id="add-entrant" variant="primary">
              Add opponent
            </Button>
          </div>
          <datalist id="vehicle-records" />
          <div id="entrants" />
          <section className="panel">
            <h2>Player vehicle</h2>
            <label className="check">
              <Input id="force-player" type="checkbox" /> Supply a specific
              vehicle for this race
            </label>
            <div id="player-vehicle" />
            <p className="hint">
              When disabled, the race uses the player's currently selected owned
              vehicle. Supplying a vehicle does not grant ownership.
            </p>
          </section>
        </section>
        <section id="rules-view" hidden>
          <div id="rules" />
        </section>
        <section id="series-view" hidden>
          <div id="series-editor" />
        </section>
        <div id="notice" role="status" hidden />
      </main>
      <dialog id="dialog">
        <form method="dialog">
          <div id="dialog-content" />
          <div className="dialog-actions">
            <Button type="submit" value="cancel">
              Cancel
            </Button>
            <Button type="submit" value="ok" variant="primary">
              Save
            </Button>
          </div>
        </form>
      </dialog>
      <Input id="file" type="file" accept=".json" hidden />
    </AppShell>
  );
}
mountApp(<RaceBuilder />, () => import("./editor"));

# Race Builder

Rust race compiler, road-route editor and CET capture mod for Cyberpunk 2077.
With the Windows distribution, use `ghostline-race-builder.exe` and pass the separate `vehicles-catalogue` directory to `--vehicles`. Generate `roads.json` from your game using the authoring guide below.

For a source build, use Rust 1.92+ and Node.js 20+. Cargo and npm fetch the locked dependencies. Run from this repository.

See [create and test a race](docs/create-a-race.md) for the complete authoring flow.

```sh
cargo run --release --locked -- serve --roads roads.json --vehicles path/to/vehicle-catalogue
```

Open <http://127.0.0.1:8795/>. Use **Draw route** to click before and after turns;
the course follows connected roads between those points. Use **Move points** to
drag a point and rebuild its adjoining road sections. Undo and redo apply to
whole route edits. **Join back to start** follows roads to close a circuit.
Drag empty map space to pan and scroll to zoom. Bridges have raised-deck rails
and a gap in the lower road, with both elevations labelled. Use the upper/lower
road buttons or **Road height** to focus within two metres of a level;
**All levels** restores the full map. Heights are game-world metres.
Disconnected roads produce an error.
Turn off **Follow roads** to place surveyed points directly.

Lay out the starting grid, adjust its marks, then choose opponent vehicles and
crew under **Vehicles & grid**. The player can use their current owned car or a
specified race vehicle. Set the game directory under
**Advanced → Tool settings**, check the race and build it. Projects save locally
under `work/projects`; advanced JSON editing exposes the complete course layout.

## Routes

The editor, CET mod and CLI use [`ghostline-race-route` version 1](schemas/route-v1.schema.json).
It contains world-space XYZ positions, headings, ordered marks, route tuning and
optional native world-layout overrides. Display text is independent of IDs.

Native builds use `--game` to sample installed collision meshes and
terrain automatically. Each five-metre road marker is fitted to 27 points across
its actual mesh footprint, including pitch and cross-slope, with at least 8 cm clearance.
Bridges and underpasses are matched to the intended road level. Small lateral
adjustments can avoid obstructions without changing the race or driving spline.
Sharp surface changes use shorter segments of the stock arrow mesh, with the same
material and continuous coverage. Mesh dimensions and its placement frame are read
from the installed game.

No running game or hand-authored height profile is needed. Samples are cached by
the generated footprints, game archives and schema, so route edits and game updates
invalidate stale measurements. A marker that cannot be covered or fitted safely
stops the build with its reference and coordinates. Generated bindings include a
`road_marker_surface_audit` for each race.

Following roads blends short sideways lane joins into gradual transitions. Saved
generated routes receive the same treatment during compilation, preserving authored
anchors and genuine turns. Native spline spans are subdivided to at most ten metres
without moving the route, so racing progression does not depend on sparse lane vertices.

**Advanced → Import route / Game Scout export** accepts shared routes, existing
Game Scout `race-plan.json` exports and saved race records. Imported native
references and explicit layout overrides are preserved.

```sh
cargo run --release -- import-route race-plan.json --out route.json
cargo run --release -- export-survey route.json --out survey.json
cargo run --release -- validate examples/race.json
cargo run --release -- build examples/race.json --out build/example --game "path/to/Cyberpunk 2077"
```

A race settings file names its route file relative to itself. Shared route and
race settings downloads from the editor use matching filenames. Reflection is
bundled and loaded automatically. `--schema` and the reflection field under Tool
settings are optional advanced overrides. Native world sampling and bootstrap
generation require the game installation.

## Championships

Open **Championship** to arrange saved races, write the organiser's phone
introduction and invitations, and set the final bonus. New championships have
nine slots and a €$100,000 completion reward. Empty slots can be saved while
courses are being authored; checking or building requires a distinct, complete
race project in every slot.

The introduction unlocks the first race. First-place finishes are recorded as
permanent, per-race facts. After finish cleanup, the next event's journal and
invitation become available together. Losses allow an immediate retry. Won races
stay completed until every course has been won; they then become replayable as
race icons, opening a journal entry when activated. Replay wins do not substitute
for other courses. The final bonus has its own durable receipt and is awarded once,
in addition to ordinary race winnings. The payout and receipt run inside a native
save lock. The organiser's messages and race display titles are editable without
changing progression identities.

Build from **Championship** to produce one archive/ZIP containing every course,
phone journal, localization and the progression controller. Building from a race
tab produces that race alone for standalone playtesting. Runtime progression is
native quest flow and does not require CET. Series manifests use
[`ghostline-race-series` version 1](schemas/series-v1.schema.json); `project`
references a saved race ID in the selected projects directory.
Set **Advanced: quest startup → Quest Core slot** to use an allocated slot in
the separate Ghostline Quest Core mod. The race package then owns its slot wrapper
and gameplay content, while the core owns the shared native bootstrap and global
overrides. Leave the slot blank to package a standalone conversation bootstrap.
Both paths are independent of H10. Native builds require `--game`.
The organiser's introduction starts the progression controller without CET.

The default `actor_ownership: "population"` generates quest-owned population
spawners for drivers, passengers, crowd and vehicles. Stationary crowd activities
use world workspots owned by the race. Drivers and passengers remain population-owned
while mounted; their transforms are not held by an owning scene. Placement edits
require rebuilding the race package, with no core workspot-registration update.

The race setting `actor_ownership: "scene"` generates scene-owned NPCs and local
animation workspots, with native population spawners for vehicles. Acquisition waits for the actual entities;
retirement waits for despawning before replacements. The `"community"` mode uses
world communities and persistent workspot registration instead. In a Quest Core
championship, community placements receive distinct IDs from the allocated slot's
workspot bank. Build the core with the generated `quest-core-registration.json`
to register their authored positions. Route arrows are
partitioned by checkpoint, displaying the current section and the next section
while hiding later overlapping passes.
Build output includes the series manifest and its source projects in `projects/`
so the complete championship can be rebuilt from that directory.

```sh
cargo run --release -- validate-series championship.series.json --projects work/projects
cargo run --release -- build-series championship.series.json --projects work/projects --out build/championship --game "path/to/Cyberpunk 2077"
```

Native generation/readback verifies resource structure. Test message delivery,
save/load, losses, replays, sequential unlocks and the final payout in game before
distributing a championship.

## Manual capture in CET

Copy `cet/init.lua` to
`bin/x64/plugins/cyber_engine_tweaks/mods/race-builder/init.lua` in the game.
Open CET to choose mark roles and recording spacing. Bind **add selected mark**,
**toggle course recording** and **mark finish** in CET's hotkey settings.

Record while driving or walking. Marks use the mounted vehicle's transform when
mounted, otherwise the player's transform. Capture the standing activation mark
on foot on clear ground. Save or archive the route, then import the resulting
JSON from the mod directory into the editor. CET is needed only for capture.

## Native road geometry

```sh
cargo run --release -- roads --game "path/to/Cyberpunk 2077" --out roads.json
```

The default extracts Night City's native persistent traffic-lane geometry and
explicit lane connections, including transition distances within each lane.
Junctions and stacked roads use native topology. The editor displays and routes
on AI-drivable road lanes, including GPS-only junction curves needed for turns;
pavement and no-AI-driving lanes remain in the extracted data but cannot carry
race opponents. Road colours and the elevation legend show changes in height.
Crossing labels distinguish bridges and underpasses from junctions; route points
also show their elevation. Hovering highlights a road and shows its height.
This covers the game's road traffic network, rather than every physical surface
on which a vehicle might fit.
Use `--traffic-root` for another world's generated traffic resource directory.

For mesh inspection, `--sectors sectors.json` extracts bended-road fragments from
an array of exact `.streamingsector` depot paths. That is partial visual geometry,
not the routing network. Both outputs report failures and provenance. The editor
uses a file and an in-memory routing graph; no database is needed.

## Generated races

Resources are generated directly: standing activation, preparation, countdown,
crew spawning and mounting, ordered checkpoints and HUD, opponent driving and
rubberbanding, recovery, finish/failure handling, rewards, courier, cleanup and
replay. One ordered course is played per attempt; closing its geometry does not
add laps. Native rules use first place and a 10,000-eddy payout.

Native builds include `source/raw`, `source/archive`, loose resources, authoring
inputs and a build report. `package/` contains an install-folder layout and ZIP
for ArchiveXL and TweakXL. Graphs, references and native readback are audited;
archive and ZIP contents are checked against generated bytes before publication.
Existing build directories are never overwritten. Resource verification does
not establish road accessibility or in-game race behavior; verify those in game.

Compiler code is split between world, resources, presentation, validation and
individual phase modules under `src/compile`. `src/route.rs` owns the common
contract; `src/roads` handles native geometry; `src/server.rs` serves the local UI.

## UI development

The TypeScript application lives in `ui/src`. `editor.ts` owns authoring state,
`models.ts` describes the API data, and `dom.ts` supplies typed form controls.
`route-map.ts` handles map input and drawing. Road segments are spatially indexed;
Rust prepares height-separated crossings when generating or loading a network.
`road-tile-worker.ts` builds road detail levels and renders tiles with
`OffscreenCanvas`. The map reuses a bounded bitmap cache while route edits and
hover highlights draw independently on animation frames.

```sh
npm run typecheck --prefix ui
npm test --prefix ui
npm run build --prefix ui
```

The shared build hook prepares missing or stale frontend output before the Rust
server embeds `ui/dist`. JavaScript workspace dependencies must already be installed.

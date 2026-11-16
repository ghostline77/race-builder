# Create and test a race

Use the local editor to draw a course, place checkpoints and select vehicles.
The installed game supplies native road geometry and build-time surface sampling.
CET is required only for manual in-game route capture.

## Prepare the editor

Install the parent JavaScript workspace dependencies and keep the native sibling
crates available. Run from the race-builder repository:

```powershell
cargo build --release --locked
cargo run --release --locked -- roads --game 'path/to/Cyberpunk 2077' --out roads.json
cargo run --release --locked -- serve --roads roads.json --vehicles ../vehicles/dist/catalogue
```

Open <http://127.0.0.1:8795/>. The vehicle picker uses a published Vehicles
catalogue. The road network represents native traffic lanes, including their
elevations and connections.

## Draw a course

1. Create a race and enter its display title.
2. In **Draw route**, click before and after turns with **Follow roads** enabled.
3. Use **Move points** to adjust the course. Drag empty space to pan and scroll to zoom.
4. Inspect stacked-road labels. Crossings with different heights are not junctions.
5. Place the start and finish, then inspect checkpoint order around loops and overlapping roads.
6. Save the project.

Use **Join back to start** for a closed shape. Closing the geometry does not add
laps. Turn **Follow roads** off to place directly surveyed points.

For a manual route, use the [CET capture workflow](../README.md#manual-capture-in-cet).
For an existing Game Scout survey, use **Advanced → Import route / Game Scout
export**. Both are converted to the [same route schema](../schemas/route-v1.schema.json)
used by the editor.

## Place the grid and actors

Choose opponents and their crew under **Vehicles & grid**. Select either the
player's owned car or a provided vehicle. Adjust individual grid marks.

Place activation, player arrival and crowd on clear ground. Allow for each
vehicle's full footprint, doors, road barriers and nearby world objects. Surface
sampling fits road markers; it does not certify that starting vehicles or NPCs
clear every obstruction.

The default population ownership keeps crew and cars independent of scene actor
transforms. Standing workspots belong to the race package. See
[ownership modes](../README.md#championships) before selecting scene or community
ownership under Advanced.

## Build and inspect

Set the game directory under **Advanced → Tool settings**, check the race and
build. The builder samples collision meshes and terrain to place the road markers.
Unfitted markers stop the build with coordinates and a reference. Inspect the
generated surface audit and build report.

For a checked-in race settings file:

```powershell
cargo run --release --locked -- validate examples/race.json
cargo run --release --locked -- build examples/race.json --out build/example --game 'path/to/Cyberpunk 2077'
```

Use a new output directory. Native reflection is automatic.

## Test in game

Follow the generated package's installation instructions for its selected startup
mode. A Quest Core slot build needs the separate core and an allocated slot.
A standalone conversation bootstrap owns its shared startup overrides.

Check payment, player arrival, crowd height, boarding and countdown. Drive every
checkpoint in order, including overlapping sections and different road levels.
Check recovery, finish dialogue, cleanup and replay after a loss. For a
championship, also check sequential unlocks and the final reward.

See [championship authoring](../README.md#championships) for the series contract.
Successful native serialization does not prove road accessibility or race AI behaviour.

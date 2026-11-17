use crate::route::{distance, point, position, text};
use anyhow::{Context, Result, ensure};
use ghostline_quest_builder::nodes as n;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

fn push(v: &mut Value, key: &str, item: Value) {
    if !v[key].is_array() {
        v[key] = json!([]);
    }
    if let Some(rows) = v[key].as_array_mut() {
        rows.push(item);
    }
}
fn absolute(origin: [f64; 3], yaw: f64, ahead: f64, side: f64) -> [f64; 3] {
    let r = yaw.to_radians();
    [
        origin[0] - r.sin() * ahead + r.cos() * side,
        origin[1] + r.cos() * ahead + r.sin() * side,
        origin[2],
    ]
}
fn area(
    world: &mut Value,
    reference: &str,
    p: [f64; 3],
    yaw: f64,
    width: f64,
    depth: f64,
    height: f64,
) {
    push(
        world,
        "triggers",
        json!({"ref":reference,"position":point(p),"yaw":yaw,"outline":{"type":"box","width":width,"depth":depth,"height":height}}),
    );
}
fn spline(world: &mut Value, reference: &str, points: &[[f64; 3]], speed: Option<Value>) {
    let origin = points[0];
    let mut row = json!({"ref":reference,"position":point(origin),"yaw":0,"looped":false,"points":points.iter().map(|p|std::array::from_fn::<_,3,_>(|i|p[i]-origin[i])).collect::<Vec<_>>()});
    if let Some(speed) = speed {
        row["speed_sections"] = speed;
    }
    push(world, "splines", row);
}
fn full(prefab: &str, reference: &str) -> String {
    if reference.starts_with("$/") {
        reference.to_owned()
    } else {
        format!("{}/{reference}", prefab.trim_end_matches('/'))
    }
}
pub fn node_hash(value: &str) -> u64 {
    ghostline_quest_builder::world::node_ref_hash(value)
}

pub fn author(config: &Value, survey: &Value) -> Result<(Value, Value)> {
    let ns = text(config, "id")?;
    let mut world = survey["world_spec"].clone();
    let role = |name: &str| -> Result<Value> {
        let mut rows = survey["bindings"]
            .as_array()
            .context("survey bindings")?
            .iter()
            .filter(|m| m["role"] == name)
            .cloned()
            .collect::<Vec<_>>();
        rows.sort_by_key(|m| m["order"].as_u64().unwrap_or(0));
        if ["checkpoint", "start_grid"].contains(&name) {
            Ok(json!(rows))
        } else {
            ensure!(rows.len() == 1, "Survey needs one {name}");
            Ok(rows.remove(0))
        }
    };
    let find = |reference: &str| -> Result<Value> {
        survey["world_spec"]["markers"]
            .as_array()
            .context("markers")?
            .iter()
            .find(|m| m["ref"] == reference)
            .cloned()
            .with_context(|| format!("Missing surveyed marker {reference}"))
    };
    let finish_binding = role("finish")?;
    let finish = find(text(&finish_binding, "ref")?)?;
    let staging = role("staging")?;
    let activation = role("activation")?;
    let player = role("player_start")?;
    let start = find(text(&player, "ref")?)?;
    let waiting = find(text(&activation, "ref")?)?;
    let grid = role("start_grid")?;
    let old_gates = role("checkpoint")?;
    for key in ["markers", "triggers"] {
        if let Some(rows) = world[key].as_array_mut() {
            rows.retain(|m| {
                !old_gates
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|b| m["ref"] == b["ref"] || m["ref"] == b["trigger_ref"])
            });
        }
    }
    let checkpoints = station_gates(
        &mut world,
        ns,
        config["checkpoint_spacing_m"].as_f64().unwrap_or(40.),
        &survey["route"],
        config,
    )?;
    let root = text(&world, "block_path")?
        .rsplit_once("\\world\\")
        .context("World block must live in a world depot folder")?
        .0
        .to_owned();
    let prefab = text(&world, "prefab_root")?.to_owned();
    for marker in world["markers"].as_array_mut().context("markers")? {
        if marker.get("marker_type").is_none() {
            marker["marker_type"] = json!("null");
        }
        if marker["ref"] == activation["ref"] {
            marker
                .as_object_mut()
                .context("marker")?
                .remove("marker_type");
            marker["scene_marker"] = json!(true);
            marker["sector"] = json!("always_loaded");
        }
    }
    world["markers"]
        .as_array_mut()
        .context("markers")?
        .retain(|m| m["ref"] != player["ref"]);
    push(
        &mut world,
        "ai_spots",
        json!({"ref":player["ref"],"position":start["position"],"yaw":start["yaw"],"vehicle_spawn":true,"is_workspot_infinite":false,"node_data":{"max_streaming_distance":15131.8017578125}}),
    );
    let driving = format!("#{ns}_driving_route");
    let mut regular = world["race_splines"][0].clone();
    regular["ref"] = json!(driving);
    push(&mut world, "splines", regular);
    cover_starting_grid(
        &mut world,
        position(&start["position"])?,
        start["yaw"].as_f64().context("grid heading")?,
        &grid
            .as_array()
            .context("grid")?
            .iter()
            .map(|binding| position(&find(text(binding, "ref")?)?["position"]))
            .collect::<Result<Vec<_>>>()?,
    )?;
    let layout = finish_layout(
        &mut world,
        config,
        &finish,
        text(&finish_binding, "trigger_ref")?,
    )?;
    let community = format!("#{ns}_entrants");
    let crowd = format!("#{ns}_spectators");
    let mut entries = Vec::new();
    let mut crews = Vec::new();
    for entry in config["entrants"].as_array().context("entrants")? {
        let id = text(entry, "id")?;
        let grid_binding = grid
            .as_array()
            .context("grid")?
            .iter()
            .find(|g| g["order"] == entry["grid"])
            .context("Unsurveyed opponent grid")?;
        let placement = find(text(grid_binding, "ref")?)?;
        let p = position(&placement["position"])?;
        let yaw = placement["yaw"].as_f64().unwrap_or(0.);
        let mut crew = json!({"id":id});
        for kind in ["vehicle", "driver", "passenger"] {
            let reference = format!("#{ns}_{id}_{kind}_spot");
            let member = format!("{id}_{kind}");
            crew[format!("{kind}_entry")] = json!(member);
            let side = if kind == "driver" {
                -2.
            } else if kind == "passenger" {
                2.
            } else {
                0.
            };
            let mut spot = json!({"ref":reference,"position":point(absolute(p,yaw,0.,side)),"yaw":yaw,"vehicle_spawn":kind=="vehicle","node_data":{"max_streaming_distance":15131.8017578125,"streaming_distance":15131.8017578125}});
            if kind != "vehicle" {
                spot["snap_to_ground"] = json!(true);
                spot["workspot"] = json!(
                    "base\\workspots\\common\\ground\\generic__stand_ground__stand_around__07.workspot"
                );
                if let Some(custom) = entry.get(format!("{kind}_staging")) {
                    for key in ["position", "yaw", "workspot"] {
                        if let Some(v) = custom.get(key) {
                            spot[key] = v.clone();
                        }
                    }
                }
            }
            let authored = &world["finish_spots"][id];
            let end = authored
                .get(kind)
                .or_else(|| authored.get("vehicle"))
                .unwrap_or(authored);
            let mut finish_p = position(&end["position"])?;
            let finish_yaw = end["yaw"].as_f64().unwrap_or(yaw);
            if kind != "vehicle" && authored.get(kind).is_none() {
                finish_p = absolute(
                    finish_p,
                    finish_yaw,
                    0.,
                    if kind == "driver" { -1.5 } else { 1.5 },
                );
            }
            let stop = json!({"ref":format!("#{ns}_{id}_{kind}_finish_spot"),"position":point(finish_p),"yaw":finish_yaw,"vehicle_spawn":kind=="vehicle","phase":"finish","node_data":{"max_streaming_distance":15131.8017578125,"streaming_distance":15131.8017578125}});
            let record = entry
                .get(kind)
                .or_else(|| {
                    if kind == "passenger" {
                        entry.get("driver")
                    } else {
                        None
                    }
                })
                .context("Crew record")?;
            entries.push(json!({"entry":member,"character":record,"active_on_start":0,"appearance":entry.get(format!("{kind}_appearance")).and_then(Value::as_str).unwrap_or("default"),"spots":[spot,stop],"additional_phases":["finish"]}));
        }
        crews.push(crew);
    }
    let origin = world["origin"].clone();
    push(
        &mut world,
        "communities",
        json!({"ref":community,"active_on_start":0,"position":origin,"streaming_distance":15131.8017578125,"node_data":{"max_streaming_distance":15131.8017578125,"streaming_distance":15131.8017578125},"entries":entries,"always_spawned":"true_","spawn_in_view":"default__true_"}),
    );
    let map_ref = format!("#{ns}_race_start_map");
    push(
        &mut world,
        "entities",
        json!({"ref":map_ref,"position":waiting["position"],"yaw":waiting["yaw"],"sector":"always_loaded","node_data":{"max_streaming_distance":15131.8017578125},"entity_template":"base\\gameplay\\devices\\empty_visible_object.ent"}),
    );
    let mut audience = Vec::new();
    let records = [
        "Character.sq024_race_announcer",
        "Character.mq056_santo_domingo_crowd_male",
        "Character.mq056_santo_domingo_crowd_female",
        "Character.mq056_santo_domingo_crowd_male",
        "Character.mq056_santo_domingo_crowd_female",
        "Character.mq056_santo_domingo_crowd_male",
        "Character.mq056_santo_domingo_crowd_female",
        "Character.Silverhand",
    ];
    let waiting_p = position(&waiting["position"])?;
    let waiting_yaw = waiting["yaw"].as_f64().unwrap_or(0.);
    for (i, record) in records.iter().enumerate() {
        let entry = if i == 0 {
            "announcer".to_owned()
        } else if i == 7 {
            "johnny".to_owned()
        } else {
            format!("spectator_{i}")
        };
        let mut p = [
            waiting_p[0] + (i % 2) as f64 * 1.2,
            waiting_p[1] + (i / 2) as f64 * 1.5,
            waiting_p[2],
        ];
        if i == 0 {
            let start = position(&start["position"])?;
            let d = (start[0] - waiting_p[0]).hypot(start[1] - waiting_p[1]);
            if d > 0. {
                let step = 1.5f64.min(d / 2.);
                p[0] += (start[0] - waiting_p[0]) / d * step;
                p[1] += (start[1] - waiting_p[1]) / d * step;
            }
        }
        if i == 7 {
            p = absolute(waiting_p, waiting_yaw, 0., 2.);
        }
        let p = world["race_actor_positions"]
            .get(&entry)
            .cloned()
            .unwrap_or(point(p));
        let yaw = world["race_actor_yaws"]
            .get(&entry)
            .cloned()
            .unwrap_or(json!(waiting_yaw));
        audience.push(json!({"entry":entry,"character":record,"active_on_start":0,"appearance":"default","spots":[{"ref":format!("#{ns}_{entry}_spot"),"position":p,"yaw":yaw,"snap_to_ground":true,"workspot":"base\\workspots\\common\\ground\\generic__stand_ground__stand_around__07.workspot"}]}));
    }
    push(
        &mut world,
        "communities",
        json!({"ref":crowd,"active_on_start":0,"position":waiting["position"],"always_spawned":"true_","spawn_in_view":"default__true_","entries":audience}),
    );
    let mut nulls = Vec::new();
    let route_points = world["race_splines"][0]["points"]
        .as_array()
        .context("spline points")?;
    let route_origin = position(&world["race_splines"][0]["position"])?;
    for (i, pair) in route_points.windows(2).enumerate() {
        let a = array_point(&pair[0])?;
        let b = array_point(&pair[1])?;
        let centre = std::array::from_fn(|j| route_origin[j] + (a[j] + b[j]) / 2.);
        nulls.push(json!({"ref":format!("#{ns}_race_crowd_null_{i:03}"),"position":point(centre),"yaw":(-(b[0]-a[0])).atan2(b[1]-a[1]).to_degrees(),"outline":{"type":"box","width":30,"depth":distance(a,b)+10.,"height":12}}));
    }
    world["crowd_null_areas"] = json!(nulls);
    let journal = format!("quests/minor_quest/{ns}");
    let mut bindings = json!({"id":ns,"community":community,"crowd":crowd,"crowd_entries":audience.iter().map(|e|e["entry"].clone()).collect::<Vec<_>>(),"prefab":prefab,"depot_root":root,"activation":activation["ref"],"spline":world["race_splines"][0]["ref"],"driving_spline":driving,"staging":staging["trigger_ref"],"player_start":player["ref"],"player_vehicle_spawn":player["ref"],"finish":finish_binding["trigger_ref"],"entrants":crews,"checkpoints":checkpoints.iter().map(|m|m["trigger_ref"].clone()).collect::<Vec<_>>(),"journal":journal,"objective":format!("{journal}/race/finish"),"activation_map_ref":full(&prefab,&map_ref),"activation_map_position":waiting["position"],"activation_objective":format!("{journal}/race/01a_wait_for_race"),"activation_mappin":format!("{journal}/race/01a_wait_for_race/waiting_spot"),"mount_objective":format!("{journal}/race/01_get_in"),"countdown_objective":format!("{journal}/race/01a_wait_for_race1"),"activation_poi":format!("points_of_interest/street_race/{ns}_start"),"player_vehicle":"#owned_vehicle","checkpoint_mode":config["checkpoint_mode"].as_str().unwrap_or("custom")});
    bindings["checkpoint_stations_m"] = json!(
        checkpoints
            .iter()
            .map(|gate| gate["station_m"].as_f64().unwrap_or(0.)
                + world["race_grid_approach_m"].as_f64().unwrap_or(0.))
            .collect::<Vec<_>>()
    );
    world["always_loaded_sector_path"] =
        json!(format!("{root}\\world\\always_loaded.streamingsector"));
    let view_ref = format!("#{ns}_activation_view");
    let mut view = absolute(
        position(&waiting["position"])?,
        waiting["yaw"].as_f64().context("activation heading")?,
        10.,
        0.,
    );
    view[2] += 1.6;
    push(
        &mut world,
        "markers",
        json!({"ref":view_ref,"position":point(view),"yaw":waiting["yaw"],"scene_marker":true,"sector":"always_loaded"}),
    );
    bindings["activation_view"] = json!(view_ref);
    for name in ["root", "lifecycle", "preparation", "attempt"] {
        bindings[format!("{name}_phase")] =
            json!(format!("{root}\\phases\\{ns}_{name}.questphase"));
    }
    for name in ["activation", "countdown", "result"] {
        bindings[format!("{name}_scene")] = json!(format!("{root}\\scenes\\{ns}_{name}.scene"));
    }
    if let Some(supplied) = config.get("player_vehicle") {
        let reference = format!("#{ns}_supplied_player_vehicle");
        bindings["player_vehicle"] = json!(reference);
        bindings["supplied_player_vehicle"] = json!(reference);
        bindings["supplied_player_vehicle_full"] = json!(full(&prefab, &reference));
        let record = text(supplied, "record")?;
        let mut entity = json!({"ref":reference,"position":start["position"],"yaw":start["yaw"],"population_record":(u64::from(crc32fast::hash(record.as_bytes()))|((record.len()as u64)<<32)).to_string(),"is_vehicle":true,"node_data":{"max_streaming_distance":15131.8017578125,"streaming_distance":15131.8017578125}});
        if let Some(appearance) = supplied.get("appearance") {
            entity["appearance"] = appearance.clone();
        }
        push(&mut world, "entities", entity);
    }
    let mut bounds = vec![waiting_p, position(&start["position"])?];
    for g in grid.as_array().context("grid")? {
        bounds.push(position(&find(text(g, "ref")?)?["position"])?);
    }
    let low = std::array::from_fn::<_, 3, _>(|i| {
        bounds.iter().map(|p| p[i]).fold(f64::INFINITY, f64::min)
    });
    let high = std::array::from_fn::<_, 3, _>(|i| {
        bounds
            .iter()
            .map(|p| p[i])
            .fold(f64::NEG_INFINITY, f64::max)
    });
    let staging_exit = world["staging_exit_ref"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or(format!("#{ns}_left_start_area"));
    if world.get("staging_exit_ref").is_none() {
        area(
            &mut world,
            &staging_exit,
            [
                (low[0] + high[0]) / 2.,
                (low[1] + high[1]) / 2.,
                low[2] - 2.,
            ],
            0.,
            high[0] - low[0] + 20.,
            high[1] - low[1] + 20.,
            high[2] - low[2] + 12.,
        );
    }
    bindings["staging_exit"] = json!(staging_exit);
    for key in [
        "finish_splines",
        "end_area",
        "cleanup_area",
        "courier_spline",
        "courier_trigger",
        "courier_workspot",
    ] {
        bindings[key] = layout[key].clone();
    }
    bindings["reward_objective"] = json!(format!("{journal}/race/04_get_reward"));
    bindings["reward_mappin"] = json!(format!("{journal}/race/04_get_reward/reward_crate"));
    for (key, record, vehicle, appearance) in [
        ("courier", "139946753417", false, None),
        (
            "courier_bike",
            "146084740299",
            true,
            Some("brennan_apollo_basic_adventure_01_1"),
        ),
    ] {
        let reference = format!("#{ns}_{key}");
        bindings[key] = json!(reference);
        let placement = &layout[if vehicle {
            "courier_bike_start"
        } else {
            "courier_start"
        }];
        let mut entity = json!({"ref":reference,"position":placement["position"],"yaw":placement["yaw"],"population_record":record,"is_vehicle":vehicle});
        if let Some(a) = appearance {
            entity["appearance"] = json!(a);
        }
        push(&mut world, "entities", entity);
    }
    bindings["reward_crate"] = json!(format!("#{ns}_reward_crate"));
    push(
        &mut world,
        "entities",
        json!({"ref":bindings["reward_crate"],"position":layout["reward_drop"]["position"],"yaw":layout["reward_drop"]["yaw"],"entity_template":"base\\gameplay\\loot\\containers\\crates\\crate_small.ent","appearance":"crate_small_default"}),
    );
    let mut visuals = Vec::new();
    let mut checkpoint_entities = json!({});
    for direction in ["Forward", "Left", "Right", "Finish"] {
        checkpoint_entities[direction] = json!(if config["checkpoint_mode"] == "vanilla" {
            "base\\quest\\minor_quests\\mq056\\entities\\mq056_checkpoint_marker.ent".to_owned()
        } else {
            format!(
                "{root}\\entities\\checkpoint_{}.ent",
                direction.to_lowercase()
            )
        });
    }
    bindings["checkpoint_entities"] = checkpoint_entities.clone();
    for gate in &checkpoints {
        let marker = world["markers"]
            .as_array()
            .context("markers")?
            .iter()
            .find(|m| m["ref"] == gate["ref"])
            .context("gate marker")?
            .clone();
        let reference = format!("{}_visual", text(gate, "ref")?);
        visuals.push(reference.clone());
        push(
            &mut world,
            "entities",
            json!({"ref":reference,"position":marker["position"],"yaw":marker["yaw"],"entity_template":checkpoint_entities[text(gate,"direction")?],"checkpoint_direction":gate["direction"],"checkpoint_trigger":gate["trigger_ref"]}),
        );
    }
    bindings["checkpoint_visuals"] = json!(visuals);
    bindings["finish_visual"] = json!(format!("{}_visual", text(&finish_binding, "ref")?));
    push(
        &mut world,
        "entities",
        json!({"ref":bindings["finish_visual"],"position":finish["position"],"yaw":finish["yaw"],"entity_template":checkpoint_entities["Finish"],"checkpoint_direction":"Finish","checkpoint_trigger":finish_binding["trigger_ref"]}),
    );
    bindings["crowd_null_areas"] =
        json!(nulls.iter().map(|n| n["ref"].clone()).collect::<Vec<_>>());
    for key in ["splines", "race_splines"] {
        if let Some(rows) = world[key].as_array_mut() {
            for row in rows {
                row["node_data"] = json!({"max_streaming_distance":15131.8017578125,"streaming_distance":15131.8017578125});
            }
        }
    }
    dressing(&mut world, &mut bindings, &waiting, &finish)?;
    // The grid approach is generated after route preparation. Subdivide here
    // as well so its spans have the same native tracking resolution as the course.
    for key in ["race_splines", "splines"] {
        for row in world[key].as_array_mut().into_iter().flatten() {
            // Explicit world-spec control points can carry custom Bezier handles.
            // Only the generated linear arrays are safe to subdivide this way.
            if row["points"]
                .as_array()
                .is_some_and(|points| points.iter().any(|p| !p.is_array()))
            {
                continue;
            }
            let mut points = row["points"]
                .as_array()
                .context("native spline points")?
                .iter()
                .map(array_point)
                .collect::<Result<Vec<_>>>()?;
            let looped = row["looped"] == true;
            if looped {
                points.push(*points.first().context("looped spline start")?);
            }
            let mut dense = crate::roads::joins::subdivide(&points);
            if looped {
                dense.pop();
            }
            row["points"] = json!(dense);
        }
    }
    bindings["course_areas"] = json!(course_areas(&mut world, ns)?);
    Ok((world, bindings))
}
/// Bound short stretches of road separately so adjacent decks can stay distinct.
fn course_areas(world: &mut Value, ns: &str) -> Result<Vec<String>> {
    let course = &world["race_splines"][0];
    let origin = position(&course["position"])?;
    let mut points = course["points"]
        .as_array()
        .context("race spline points")?
        .iter()
        .map(array_point)
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        points.len() >= 2,
        "Course coverage needs at least two points"
    );
    if course["looped"] == true {
        points.push(points[0]);
    }
    let mut refs = Vec::new();
    let mut start = 0;
    while start + 1 < points.len() {
        let mut end = start + 1;
        let mut length = distance(points[start], points[end]);
        while end + 1 < points.len() {
            let next = points[end + 1];
            let next_length = length + distance(points[end], next);
            if next_length > 32.0 || (next[2] - points[start][2]).abs() > 3.0 {
                break;
            }
            end += 1;
            length = next_length;
        }
        let a = points[start];
        let b = points[end];
        let angle = (b[0] - a[0]).atan2(b[1] - a[1]);
        let (sin, cos) = angle.sin_cos();
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        for p in &points[start..=end] {
            let coordinates = [p[0] * cos - p[1] * sin, p[0] * sin + p[1] * cos, p[2]];
            for axis in 0..3 {
                low[axis] = low[axis].min(coordinates[axis]);
                high[axis] = high[axis].max(coordinates[axis]);
            }
        }
        let side = (low[0] + high[0]) / 2.0;
        let ahead = (low[1] + high[1]) / 2.0;
        let base = [
            origin[0] + side * cos + ahead * sin,
            origin[1] - side * sin + ahead * cos,
            origin[2] + low[2] - 3.0,
        ];
        let reference = format!("#{ns}_course_area_{:04}", refs.len());
        area(
            world,
            &reference,
            base,
            -angle.to_degrees(),
            high[0] - low[0] + 50.0,
            high[1] - low[1] + 24.0,
            high[2] - low[2] + 8.0,
        );
        refs.push(reference);
        start = end;
    }
    Ok(refs)
}

fn array_point(v: &Value) -> Result<[f64; 3]> {
    Ok([
        v[0].as_f64().context("spline x")?,
        v[1].as_f64().context("spline y")?,
        v[2].as_f64().context("spline z")?,
    ])
}

/// Include the surveyed grid approach in recovery and guidance geometry.
/// Checkpoints and the AI driving spline retain their authored course distances.
fn cover_starting_grid(
    world: &mut Value,
    start: [f64; 3],
    yaw: f64,
    grid: &[[f64; 3]],
) -> Result<()> {
    let course = &world["race_splines"][0];
    if course["looped"] == true {
        return Ok(());
    }
    let origin = position(&course["position"])?;
    let points = course["points"].as_array().context("course points")?;
    ensure!(points.len() >= 2, "Course needs two points");
    let a = array_point(&points[0])?;
    // A tiny connector at the start need not face along the starting grid.
    let heading = yaw.to_radians();
    let forward = [-heading.sin(), heading.cos(), 0.];
    // Actual grid samples preserve curves and road elevation. A tangent extended
    // backwards can cut through a ramp or leave the rear grid outside recovery.
    let first: [f64; 3] = std::array::from_fn(|i| a[i] + origin[i]);
    let station = |p: &[f64; 3]| (p[0] - first[0]) * forward[0] + (p[1] - first[1]) * forward[1];
    let mut approach = grid
        .iter()
        .copied()
        .chain([start])
        .filter(|p| station(p) < -0.5)
        .collect::<Vec<_>>();
    approach.sort_by(|a, b| station(a).total_cmp(&station(b)));
    approach.dedup_by(|a, b| distance(*a, *b) < 0.5);
    // Cars beside each other describe one grid row, not successive turns.
    // Average each row so recovery follows its centre instead of zigzagging
    // through every vehicle's individual starting position.
    let mut rows = Vec::new();
    let mut remaining = approach.as_slice();
    while let Some(first) = remaining.first() {
        let count = remaining
            .iter()
            .take_while(|p| station(p) - station(first) < 1.)
            .count();
        rows.push(std::array::from_fn::<_, 3, _>(|axis| {
            remaining[..count].iter().map(|p| p[axis]).sum::<f64>() / count as f64
        }));
        remaining = &remaining[count..];
    }
    let approach = rows;
    if approach.is_empty() {
        return Ok(());
    }
    let extra_length = approach
        .iter()
        .copied()
        .chain([first])
        .collect::<Vec<_>>()
        .windows(2)
        .map(|p| distance(p[0], p[1]))
        .sum::<f64>();
    let mut extended = approach
        .iter()
        .map(|p| json!(std::array::from_fn::<_, 3, _>(|i| p[i] - origin[i])))
        .collect::<Vec<_>>();
    extended.extend(points.iter().cloned());
    world["race_grid_approach_m"] = json!(extra_length);
    world["race_splines"][0]["points"] = json!(extended);
    for key in ["speed_sections", "width_sections"] {
        if let Some(sections) = world["race_splines"][0]
            .get_mut(key)
            .and_then(Value::as_array_mut)
        {
            for section in sections {
                for field in if key == "width_sections" {
                    ["from", "to"]
                } else {
                    ["start", "end"]
                } {
                    if let Some(value) = section[field].as_f64() {
                        if value > 0. {
                            section[field] = json!(value + extra_length);
                        }
                    }
                }
            }
        }
    }
    if let Some(guidance) = world.get_mut("race_guidance").and_then(Value::as_array_mut) {
        let mut extended = approach
            .iter()
            .map(|p| json!({"position":point(*p),"yaw":yaw}))
            .collect::<Vec<_>>();
        extended.append(guidance);
        *guidance = extended;
    }
    Ok(())
}

fn station_gates(
    world: &mut Value,
    ns: &str,
    spacing: f64,
    route: &Value,
    config: &Value,
) -> Result<Vec<Value>> {
    let spline = world["race_splines"][0].clone();
    let origin = position(&spline["position"])?;
    let mut points = spline["points"]
        .as_array()
        .context("spline points")?
        .iter()
        .map(array_point)
        .collect::<Result<Vec<_>>>()?;
    if spline["looped"] == true {
        points.push(points[0]);
    }
    let mut segments = Vec::new();
    let mut length = 0.;
    for pair in points.windows(2) {
        let d = distance(pair[0], pair[1]);
        if d > 0. {
            segments.push((length, length + d, pair[0], pair[1]));
            length += d;
        }
    }
    ensure!(length > 0., "Course must have nonzero length");
    let sample = |s: f64| -> Result<([f64; 3], [f64; 3])> {
        let s = s.min(length);
        let &(start, end, a, b) = segments
            .iter()
            .find(|(_, end, _, _)| s <= *end)
            .or_else(|| segments.last())
            .context("course segment")?;
        let t = (s - start) / (end - start);
        Ok((
            std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t),
            std::array::from_fn(|i| b[i] - a[i]),
        ))
    };
    let old = world["markers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| {
            m["ref"]
                .as_str()
                .is_some_and(|s| s.contains("_checkpoint_") || s.contains("_checkpoint_mark_"))
        })
        .map(|m| m["ref"].clone())
        .collect::<Vec<_>>();
    for key in ["markers", "triggers"] {
        if let Some(rows) = world[key].as_array_mut() {
            rows.retain(|m| {
                !old.iter().any(|r| {
                    m["ref"] == *r
                        || m["ref"] == json!(format!("{}_trigger", r.as_str().unwrap_or("")))
                })
            });
        }
    }
    let mut stations = Vec::new();
    let mut s = spacing;
    while config["checkpoint_placement"] != "manual" && s < length - spacing * 0.35 {
        stations.push((s, None));
        s += spacing;
    }
    let absolute_points = points
        .iter()
        .map(|p| std::array::from_fn(|i| p[i] + origin[i]))
        .collect::<Vec<_>>();
    for mark in crate::route::group(route, "checkpoint") {
        // A saved distance disambiguates repeated passes, but dragging a mark
        // must move the generated checkpoint to its new course position.
        let station = crate::route::station_near(
            position(&mark["position"])?,
            &absolute_points,
            mark["options"]["station_m"].as_f64(),
        )
        .0;
        ensure!(
            station.is_finite() && station > 0. && station < length,
            "Checkpoint {} must lie within the course",
            mark["id"]
        );
        stations.push((station, Some(mark)));
    }
    stations.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Keep each authored gate, including distinct passes through the same road.
    // Only discard automatic gates that would duplicate an authored one.
    let authored_stations = stations
        .iter()
        .filter(|s| s.1.is_some())
        .map(|s| s.0)
        .collect::<Vec<_>>();
    stations.retain(|s| s.1.is_some() || !authored_stations.iter().any(|a| (s.0 - a).abs() < 1.));
    if stations.is_empty() {
        ensure!(
            config["checkpoint_placement"] != "manual",
            "Place at least one manual checkpoint"
        );
        stations.push((length / 2., None));
    }
    let mut gates = Vec::new();
    for (i, (s, authored)) in stations.into_iter().enumerate() {
        let (p, delta) = sample(s)?;
        let (_, next) = sample(s + 25.)?;
        let turn = (delta[0] * next[1] - delta[1] * next[0])
            .atan2(delta[0] * next[0] + delta[1] * next[1])
            .to_degrees();
        let direction = if turn.abs() < 20. {
            "Forward"
        } else if turn > 0. {
            "Left"
        } else {
            "Right"
        };
        let direction = authored
            .and_then(|m| m["options"]["direction"].as_str())
            .unwrap_or(direction);
        ensure!(
            matches!(direction, "Forward" | "Left" | "Right"),
            "Checkpoint arrow must be Forward, Left or Right"
        );
        let reference = format!("#{ns}_route_checkpoint_{:03}", i + 1);
        let trigger = format!("{reference}_trigger");
        let course_position: [f64; 3] = std::array::from_fn(|j| p[j] + origin[j]);
        let p = authored
            .map(|m| position(&m["position"]))
            .transpose()?
            .unwrap_or(course_position);
        let yaw = authored
            .filter(|m| m["options"]["align_to_route"] == false)
            .and_then(|m| m["yaw"].as_f64())
            .unwrap_or_else(|| (-delta[0]).atan2(delta[1]).to_degrees().rem_euclid(360.));
        push(
            world,
            "markers",
            json!({"ref":reference,"position":point(p),"yaw":yaw,"marker_type":"null"}),
        );
        let options = authored.map(|m| &m["options"]);
        let width = options
            .and_then(|o| o["width_m"].as_f64())
            .unwrap_or(config["checkpoint_width_m"].as_f64().unwrap_or(12.));
        let depth = options.and_then(|o| o["depth_m"].as_f64()).unwrap_or(4.);
        let height = options.and_then(|o| o["height_m"].as_f64()).unwrap_or(6.);
        if let Some(mark) = authored {
            let offset = (p[0] - course_position[0]).hypot(p[1] - course_position[1]);
            ensure!(
                offset <= width.hypot(depth) / 2.,
                "Checkpoint {} is {offset:.1} m off the course; its {width:.1} by {depth:.1} m gate cannot reach the route. Move it onto the route or adjust its dimensions.",
                mark["id"]
            );
        }
        area(world, &trigger, p, yaw, width, depth, height);
        gates.push(json!({"ref":reference,"trigger_ref":trigger,"order":i,"direction":direction,"station_m":s}));
    }
    Ok(gates)
}

fn finish_layout(
    world: &mut Value,
    config: &Value,
    finish: &Value,
    finish_trigger: &str,
) -> Result<Value> {
    let ns = text(config, "id")?;
    let origin = position(&finish["position"])?;
    let yaw = finish["yaw"].as_f64().unwrap_or(0.);
    let width = world["race_splines"][0]["width_default"]
        .as_f64()
        .unwrap_or(8.);
    let entrants = config["entrants"].as_array().context("entrants")?;
    if let Some(stop) = world.get("finish_vehicle_forbidden_area") {
        let p = position(&stop["position"])?;
        let y = crate::route::numeric(&stop["yaw"], "stopping yaw")?;
        ensure!(
            stop["speed_limit"].as_f64().unwrap_or(20.) > 0.,
            "Stopping speed must be positive"
        );
        for key in ["outline", "inner_outline"] {
            ensure!(
                stop[key]["type"] == "box",
                "Finish stopping outlines must be surveyed boxes"
            );
            for axis in ["width", "depth", "height"] {
                ensure!(
                    crate::route::numeric(&stop[key][axis], axis)? > 0.,
                    "Stopping dimensions must be positive"
                );
            }
        }
        for axis in ["width", "depth", "height"] {
            ensure!(
                stop["inner_outline"][axis].as_f64() <= stop["outline"][axis].as_f64(),
                "Inner stopping box must fit its outer box"
            );
        }
        let angle = (y - yaw).to_radians();
        let half = (angle.sin().abs() * stop["outline"]["width"].as_f64().unwrap_or(0.)
            + angle.cos().abs() * stop["outline"]["depth"].as_f64().unwrap_or(0.))
            / 2.;
        let f = absolute([0.; 3], yaw, 1., 0.);
        let ahead = (p[0] - origin[0]) * f[0] + (p[1] - origin[1]) * f[1];
        let crossing = world["triggers"]
            .as_array()
            .context("triggers")?
            .iter()
            .find(|t| t["ref"] == finish_trigger)
            .context("finish trigger")?;
        ensure!(
            crossing["outline"]["type"] == "box",
            "Finish crossing must be a box"
        );
        let cp = position(&crossing["position"])?;
        let a = (crossing["yaw"].as_f64().unwrap_or(0.) - yaw).to_radians();
        let edge = (cp[0] - origin[0]) * f[0]
            + (cp[1] - origin[1]) * f[1]
            + (a.sin().abs() * crossing["outline"]["width"].as_f64().unwrap_or(0.)
                + a.cos().abs() * crossing["outline"]["depth"].as_f64().unwrap_or(0.))
                / 2.;
        ensure!(
            ahead - half > edge,
            "Entire stopping area must be beyond the finish crossing"
        );
    }
    if !world["finish_spots"].is_object() {
        world["finish_spots"] = json!({});
    }
    let mut routes = world.get("finish_splines").cloned().unwrap_or(json!({}));
    for (i, entrant) in entrants.iter().enumerate() {
        let id = text(entrant, "id")?;
        if world["finish_spots"].get(id).is_none() {
            world["finish_spots"][id] = json!({"position":point(absolute(origin,yaw,10.*(i+1)as f64,if i%2==0{width/4.}else{-width/4.})),"yaw":yaw});
        }
        let spot = &world["finish_spots"][id];
        let endpoint = position(&spot.get("vehicle").unwrap_or(spot)["position"])?;
        if routes.get(id).is_none() {
            let reference = format!("#{ns}_{id}_finish_parking");
            spline(
                world,
                &reference,
                &[
                    origin,
                    std::array::from_fn(|i| (origin[i] + endpoint[i]) / 2.),
                    endpoint,
                ],
                None,
            );
            routes[id] = json!(reference);
        }
    }
    let route_refs = routes
        .as_object()
        .context("finish splines")?
        .values()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    ensure!(
        route_refs.iter().collect::<HashSet<_>>().len() == route_refs.len()
            && !route_refs
                .iter()
                .any(|r| world["race_splines"][0]["ref"] == *r),
        "Each entrant needs a distinct parking spline separate from the race spline"
    );
    let end = world["end_area_ref"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or(format!("#{ns}_finish_cleanup_area"));
    let depth = 80f64.max(entrants.len() as f64 * 10. + 40.);
    if world.get("end_area_ref").is_none() {
        area(
            world,
            &end,
            absolute(origin, yaw, depth / 2. - 20., 0.),
            yaw,
            width * 2. + 12.,
            depth,
            12.,
        );
    }
    let cleanup = world["final_cleanup_area_ref"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or(format!("#{ns}_final_cleanup_area"));
    if world.get("final_cleanup_area_ref").is_none() {
        let area_data = world["triggers"]
            .as_array()
            .context("triggers")?
            .iter()
            .find(|a| a["ref"] == end)
            .context("finish exit area")?
            .clone();
        let mut p = position(&area_data["position"])?;
        p[2] -= 2.;
        area(
            world,
            &cleanup,
            p,
            area_data["yaw"].as_f64().unwrap_or(yaw),
            area_data["outline"]["width"]
                .as_f64()
                .unwrap_or(width * 2. + 12.)
                + 20.,
            area_data["outline"]["depth"].as_f64().unwrap_or(depth) + 20.,
            area_data["outline"]["height"].as_f64().unwrap_or(12.) + 4.,
        );
    }
    let drop = world
        .get("reward_drop")
        .cloned()
        .unwrap_or(json!({"position":point(absolute(origin,yaw,0.,width/2.)),"yaw":yaw}));
    let dp = position(&drop["position"])?;
    let courier = world
        .get("courier_start")
        .cloned()
        .unwrap_or(json!({"position":point(absolute(dp,yaw,1.,0.)),"yaw":yaw}));
    let bike = world
        .get("courier_bike_start")
        .cloned()
        .unwrap_or(json!({"position":point(absolute(origin,yaw,8.,width/2.)),"yaw":yaw}));
    let pickup = world["courier_trigger_ref"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or(format!("#{ns}_reward_pickup_area"));
    if world.get("courier_trigger_ref").is_none() {
        area(world, &pickup, dp, yaw, 8., 8., 12.);
    }
    let departure = world["courier_spline"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or(format!("#{ns}_courier_departure"));
    if world.get("courier_spline").is_none() {
        let points = [
            position(&bike["position"])?,
            absolute(origin, yaw, 20., width / 4.),
            absolute(
                origin,
                yaw,
                80f64.max(entrants.len() as f64 * 10. + 30.),
                width / 4.,
            ),
        ];
        let length = points.windows(2).map(|p| distance(p[0], p[1])).sum::<f64>();
        spline(
            world,
            &departure,
            &points,
            Some(
                json!([{"start":0,"end":10,"speed_mps":5.555555820465088},{"start":10,"end":length,"speed_mps":25.000001907348633}]),
            ),
        );
    }
    ensure!(
        departure != world["race_splines"][0]["ref"].as_str().unwrap_or("")
            && !route_refs.contains(&departure.as_str()),
        "Courier departure must have its own spline"
    );
    ensure!(
        [
            finish_trigger,
            end.as_str(),
            cleanup.as_str(),
            pickup.as_str()
        ]
        .iter()
        .collect::<HashSet<_>>()
        .len()
            == 4,
        "Finish, exit, cleanup and pickup must use distinct areas"
    );
    let workspot = format!("#{ns}_courier_wait");
    push(
        world,
        "ai_spots",
        json!({"ref":workspot,"position":courier["position"],"yaw":courier["yaw"],"workspot":"base\\workspots\\archetype\\dirt\\common\\wall\\dirt__stand_wall_lean_back__stand_around__01.workspot"}),
    );
    Ok(
        json!({"finish_splines":routes,"end_area":end,"cleanup_area":cleanup,"reward_drop":drop,"courier_start":courier,"courier_bike_start":bike,"courier_spline":departure,"courier_trigger":pickup,"courier_workspot":workspot}),
    )
}

fn dressing(
    world: &mut Value,
    bindings: &mut Value,
    waiting: &Value,
    _finish: &Value,
) -> Result<()> {
    let ns = text(bindings, "id")?.to_owned();
    let mut groups = json!({"barriers":[],"checkpoints":bindings["checkpoint_visuals"],"checkpoints2":[bindings["finish_visual"]],"crate":[bindings["reward_crate"]]});
    let gates = bindings["checkpoint_stations_m"]
        .as_array()
        .context("checkpoint stations")?
        .iter()
        .map(|v| v.as_f64().context("checkpoint station"))
        .collect::<Result<Vec<_>>>()?;
    let legs = (0..=gates.len())
        .map(|i| format!("gps_leg_{i:03}"))
        .collect::<Vec<_>>();
    for leg in &legs {
        groups[leg] = json!([]);
    }
    bindings["guidance_legs"] = json!(legs);
    let course = world["race_splines"][0]["points"]
        .as_array()
        .context("race points")?
        .iter()
        .map(array_point)
        .collect::<Result<Vec<_>>>()?;
    let mut course_stations = vec![0.];
    for pair in course.windows(2) {
        course_stations
            .push(course_stations.last().context("course station")? + distance(pair[0], pair[1]));
    }
    let dressing = if let Some(authored) = world.get("race_dressing") {
        authored.clone()
    } else {
        let origin = position(&world["race_splines"][0]["position"])?;
        let points = world["race_splines"][0]["points"]
            .as_array()
            .context("spline points")?
            .iter()
            .map(array_point)
            .collect::<Result<Vec<_>>>()?;
        let mut guidance = Vec::new();
        for (i, p) in points.iter().enumerate() {
            let (a, b) = if i + 1 < points.len() {
                (*p, points[i + 1])
            } else {
                (points[i - 1], *p)
            };
            guidance.push(json!({"position":point(std::array::from_fn(|j|origin[j]+p[j])),"yaw":(-(b[0]-a[0])).atan2(b[1]-a[1]).to_degrees()}));
        }
        json!({"gps_path":world.get("race_guidance").cloned().unwrap_or(json!(guidance)),"barriers":[]})
    };
    let mut tile_count = 0;
    for (kind, placements) in dressing.as_object().context("race dressing")? {
        let asset = match kind.as_str() {
            "gps_path" => "base\\quest\\side_quests\\sq024\\entities\\sq024_gps_path.ent",
            "barriers" => {
                "base\\items\\interactive\\public_utility\\int_public_utility_002__road_barrier_a_poor.ent"
            }
            _ => anyhow::bail!("Unknown dressing group {kind}"),
        };
        let mut placements = placements
            .as_array()
            .context("dressing placements")?
            .clone();
        if kind == "gps_path" && placements.len() > 1 {
            // The native hologram is five metres long. Carry spacing across
            // segment boundaries instead of restarting and stacking full tiles.
            let spacing = world["race_guidance_spacing_m"].as_f64().unwrap_or(5.);
            ensure!(
                spacing.is_finite() && spacing >= 5.,
                "Guidance spacing must be at least the native tile length (5 m)"
            );
            let mut tiles = Vec::new();
            let synchronized = placements.len() == course_stations.len();
            let guidance_length = placements
                .windows(2)
                .map(|pair| {
                    Ok(distance(
                        position(&pair[0]["position"])?,
                        position(&pair[1]["position"])?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?
                .iter()
                .sum::<f64>();
            let mut traveled = 0.;
            let mut next_center = 2.5;
            for (segment, pair) in placements.windows(2).enumerate() {
                let a = position(&pair[0]["position"])?;
                let b = position(&pair[1]["position"])?;
                let length = distance(a, b);
                if length == 0. {
                    continue;
                }
                let yaw = (-(b[0] - a[0])).atan2(b[1] - a[1]).to_degrees();
                let pitch = (b[2] - a[2]).atan2((b[0] - a[0]).hypot(b[1] - a[1]));
                while next_center <= traveled + length {
                    ensure!(tiles.len() < 100000, "Excessive guidance density");
                    let t = (next_center - traveled) / length;
                    let p: [f64; 3] = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
                    let rotation = glam::DQuat::from_rotation_z(yaw.to_radians())
                        * glam::DQuat::from_rotation_x(pitch);
                    let station = if synchronized {
                        course_stations[segment]
                            + t * (course_stations[segment + 1] - course_stations[segment])
                    } else {
                        next_center / guidance_length
                            * course_stations.last().context("course length")?
                    };
                    tiles.push(json!({"position":point(p),"yaw":yaw,"station_m":station,"node_data":{"orientation":{"i":rotation.x,"j":rotation.y,"k":rotation.z,"r":rotation.w}}}));
                    next_center += spacing;
                }
                traveled += length;
            }
            placements = tiles;
        }
        for (i, p) in placements.iter().enumerate() {
            if kind == "gps_path" {
                tile_count += 1;
            }
            let reference = format!("#{ns}_{kind}_{i}");
            let mut entity = json!({"ref":reference,"position":p["position"],"yaw":p["yaw"],"entity_template":asset});
            if let Some(data) = p.get("node_data") {
                entity["node_data"] = data.clone();
            }
            let fitted = if kind == "gps_path" {
                world["race_guidance_placements"]
                    .get(&reference)
                    .and_then(Value::as_array)
                    .cloned()
            } else {
                None
            };
            let parts = fitted.unwrap_or_else(|| vec![entity.clone()]);
            for (part, fit) in parts.into_iter().enumerate() {
                let group = if kind == "gps_path" {
                    let station = p["station_m"].as_f64().unwrap_or(0.)
                        + fit["station_offset_m"].as_f64().unwrap_or(0.);
                    legs[gates.partition_point(|s| *s < station)].as_str()
                } else {
                    kind.as_str()
                };
                let reference = if part == 0 {
                    reference.clone()
                } else {
                    format!("{reference}_part_{part}")
                };
                let mut placed = entity.clone();
                placed
                    .as_object_mut()
                    .context("road marker")?
                    .extend(fit.as_object().context("road marker placement")?.clone());
                placed["ref"] = json!(reference);
                if placed.get("mesh").is_some() {
                    placed
                        .as_object_mut()
                        .context("mesh marker")?
                        .remove("entity_template");
                }
                placed
                    .as_object_mut()
                    .context("marker station")?
                    .remove("station_offset_m");
                push(world, "entities", placed);
                groups[group]
                    .as_array_mut()
                    .context("variant group")?
                    .push(json!(reference));
            }
        }
    }
    let first_id = text(&bindings["entrants"][0], "id")?;
    let finish_placement = world["finish_spots"][first_id].clone();
    let finish_placement = finish_placement.get("vehicle").unwrap_or(&finish_placement);
    for (name, placement, sizes) in [
        ("race_crowd_start", waiting, vec!["medium", "medium"]),
        (
            "race_crowd_finish",
            finish_placement,
            vec!["medium", "big", "medium", "small"],
        ),
    ] {
        let origin = position(&placement["position"])?;
        let yaw = placement["yaw"].as_f64().unwrap_or(0.);
        let defaults=sizes.iter().enumerate().map(|(i,size)|json!({"position":point(absolute(origin,yaw,(i/2)as f64*12.,if i%2==0{-6.}else{6.})),"yaw":yaw,"event":format!("amb_ext_crowd_sq024_racetrack_{size}")})).collect::<Vec<_>>();
        let audio = world["race_audio"]
            .get(name)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or(defaults);
        groups[name] = json!([]);
        for (i, audio) in audio.iter().enumerate() {
            let reference = format!("#{ns}_{name}_{i}");
            push(
                world,
                "entities",
                json!({"ref":reference,"position":audio["position"],"yaw":audio["yaw"].as_f64().unwrap_or(yaw),"ambient_event":audio["event"],"ambient_outline":{"type":"box","width":2,"depth":2,"height":2}}),
            );
            groups[name]
                .as_array_mut()
                .context("audio group")?
                .push(json!(reference));
        }
    }
    for (role, key) in [("start", "staging"), ("courier", "courier_trigger")] {
        let footprint = world["triggers"]
            .as_array()
            .context("triggers")?
            .iter()
            .find(|a| a["ref"] == bindings[key])
            .context("exclusion footprint")?
            .clone();
        let reference = format!("#{ns}_vehicle_forbidden_{role}");
        let mut entity = json!({"ref":reference,"position":footprint["position"],"yaw":footprint["yaw"],"vehicle_forbidden_outline":footprint["outline"]});
        if role == "start" {
            let p = position(&waiting["position"])?;
            let mut spots = world["ai_spots"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|s| s["vehicle_spawn"] == true)
                .cloned()
                .collect::<Vec<_>>();
            for community in world["communities"].as_array().into_iter().flatten() {
                for entry in community["entries"].as_array().into_iter().flatten() {
                    spots.extend(
                        entry["spots"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|s| s["vehicle_spawn"] == true && s["phase"] != "finish")
                            .cloned(),
                    );
                }
            }
            let clearance = spots
                .iter()
                .map(|s| position(&s["position"]).map(|q| (q[0] - p[0]).hypot(q[1] - p[1])))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .fold(f64::INFINITY, f64::min);
            let half = 1.5f64.min((clearance - 3.) / 2f64.sqrt());
            ensure!(
                half > 0.,
                "Waiting point must clear vehicle spawn centres by more than 3 m"
            );
            entity["position"] = waiting["position"].clone();
            entity["yaw"] = waiting["yaw"].clone();
            entity["vehicle_forbidden_outline"] =
                json!({"type":"box","width":half*2.,"depth":half*2.,"height":5});
            entity["vehicle_forbidden_inner_outline"] =
                json!({"type":"box","width":half,"depth":half,"height":4});
            entity["vehicle_forbidden_inner_bound"] = json!(false);
            entity["vehicle_forbidden_speed_limit"] = json!(15);
            entity["vehicle_forbidden_null_area"] = json!(true);
        }
        push(world, "entities", entity);
        bindings[format!("forbidden_{role}")] = json!(reference);
        groups[format!("vehicle_forbidden_{role}")] = json!([reference]);
    }
    if let Some(stop) = world.get("finish_vehicle_forbidden_area").cloned() {
        let reference = format!("#{ns}_vehicle_forbidden_finish");
        push(
            world,
            "entities",
            json!({"ref":reference,"position":stop["position"],"yaw":stop["yaw"],"vehicle_forbidden_outline":stop["outline"],"vehicle_forbidden_inner_outline":stop["inner_outline"],"vehicle_forbidden_inner_bound":false,"vehicle_forbidden_speed_limit":stop["speed_limit"].as_f64().unwrap_or(20.),"vehicle_forbidden_null_area":true}),
        );
        bindings["forbidden_finish"] = json!(reference);
        groups["vehicle_forbidden_finish"] = json!([reference]);
    }
    let mut water = Vec::new();
    for (i, volume) in world["water_areas"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        let reference = format!("#{ns}_water_hazard_{i}");
        let mut v = volume;
        v["ref"] = json!(reference);
        push(world, "triggers", v);
        water.push(reference);
    }
    bindings["water_areas"] = json!(water);
    bindings["road_marker_surface_audit"] = json!({
        "measured_tiles":0,
        "unmeasured_tiles":tile_count,
        "clearance_m":0.08
    });
    if let Some(audit) = world.get("race_guidance_surface_audit") {
        bindings["road_marker_surface_audit"] = audit.clone();
    }
    bindings["variants"] = groups;
    Ok(())
}

pub fn variants(
    documents: &mut BTreeMap<String, Value>,
    world: &Value,
    bindings: &Value,
) -> Result<()> {
    let depot = text(world, "quest_sector_path")?;
    let sector = &mut documents.get_mut(depot).context("course sector")?["Data"]["RootChunk"];
    let nodes = sector["nodes"].as_array().context("sector nodes")?.clone();
    let refs = sector["nodeRefs"]
        .as_array()
        .context("sector references")?
        .clone();
    let data = sector["nodeData"]["Data"]
        .as_array()
        .context("sector placements")?
        .clone();
    let mut groups: BTreeMap<String, Vec<(Value, Value, Value)>> = BTreeMap::new();
    groups.insert("base".to_owned(), Vec::new());
    for name in bindings["variants"].as_object().context("variants")?.keys() {
        groups.insert(name.clone(), Vec::new());
    }
    for (i, (mut node, reference)) in nodes.into_iter().zip(refs).enumerate() {
        let alias = reference["$value"]
            .as_str()
            .unwrap_or("")
            .rsplit('/')
            .next()
            .unwrap_or("");
        let group = bindings["variants"]
            .as_object()
            .context("variants")?
            .iter()
            .find(|(_, refs)| {
                refs.as_array()
                    .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(alias)))
            })
            .map_or("base", |(name, _)| name.as_str());
        if bindings["checkpoint_visuals"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|v| v.as_str() == Some(alias))
            || bindings["finish_visual"].as_str() == Some(alias)
        {
            node["Data"]["isVisibleInGame"] = json!(0);
        }
        let placement = data
            .iter()
            .find(|d| d["NodeIndex"].as_u64() == Some(i as u64))
            .context("placement for node")?
            .clone();
        groups
            .get_mut(group)
            .context("variant group")?
            .push((node, reference, placement));
    }
    let mut order = vec!["base".to_owned()];
    order.extend(groups.keys().filter(|k| k.as_str() != "base").cloned());
    let mut nodes = Vec::new();
    let mut refs = Vec::new();
    let mut data = Vec::new();
    let mut indices = Vec::new();
    let mut descriptors = Vec::new();
    for (range, name) in order.iter().enumerate() {
        indices.push(data.len());
        for (mut node, reference, mut placement) in groups.remove(name).unwrap_or_default() {
            placement["NodeIndex"] = json!(nodes.len());
            node["Data"]["sourcePrefabHash"] =
                json!(node_hash(text(bindings, "prefab")?).to_string());
            nodes.push(node);
            refs.push(reference);
            data.push(placement);
        }
        if name != "base" {
            descriptors.push(json!({"$type":"worldStreamingSectorVariant","nodeRef":n::node_ref(text(bindings,"prefab")?),"variantId":crc32fast::hash(format!("{}:{name}",text(bindings,"id")?).as_bytes()),"parentVariantID":0,"name":n::cname(name),"rangeIndex":range,"enabledByDefault":0}));
        }
    }
    sector["nodes"] = json!(nodes);
    sector["nodeRefs"] = json!(refs);
    sector["nodeData"]["Data"] = json!(data);
    sector["variantIndices"] = json!(indices);
    sector["variantNodes"] = json!([]);
    let block = &mut documents
        .get_mut(text(world, "block_path")?)
        .context("streaming block")?["Data"]["RootChunk"];
    for descriptor in block["descriptors"]
        .as_array_mut()
        .context("streaming descriptors")?
    {
        if descriptor["data"]["DepotPath"]["$value"] == depot {
            descriptor["variants"] = json!(descriptors);
            descriptor["numNodeRanges"] = json!(order.len());
        }
    }
    Ok(())
}

pub fn stock_checkpoints(documents: &mut BTreeMap<String, Value>, world: &Value) -> Result<()> {
    fn maximum(v: &Value, key: &str) -> u64 {
        let own = v
            .get(key)
            .and_then(|v| {
                v.as_str()
                    .and_then(|s| s.parse().ok())
                    .or_else(|| v.as_u64())
            })
            .unwrap_or(0);
        match v {
            Value::Object(m) => m.values().map(|v| maximum(v, key)).fold(own, u64::max),
            Value::Array(a) => a.iter().map(|v| maximum(v, key)).fold(own, u64::max),
            _ => own,
        }
    }
    let placements = world["entities"]
        .as_array()
        .context("entities")?
        .iter()
        .filter(|v| v.get("checkpoint_direction").is_some())
        .map(|v| (v["ref"].as_str().unwrap_or("").to_owned(), v.clone()))
        .collect::<BTreeMap<_, _>>();
    for document in documents.values_mut() {
        if document["Data"]["RootChunk"]["$type"] != "worldStreamingSector" {
            continue;
        }
        let mut handle = maximum(document, "HandleId");
        let mut buffer = maximum(document, "BufferId");
        let sector = &mut document["Data"]["RootChunk"];
        let refs = sector["nodeRefs"].as_array().context("node refs")?.clone();
        for (node, reference) in sector["nodes"]
            .as_array_mut()
            .context("sector nodes")?
            .iter_mut()
            .zip(refs)
        {
            let alias = reference["$value"]
                .as_str()
                .unwrap_or("")
                .rsplit('/')
                .next()
                .unwrap_or("");
            let Some(p) = placements.get(alias) else {
                continue;
            };
            let direction = text(p, "checkpoint_direction")?;
            let record = if direction == "Finish" {
                "LCDScreens.RaceFinish".to_owned()
            } else {
                format!("LCDScreens.RaceCheckpoint{direction}")
            };
            let mut wrap = |data: Value| {
                handle += 1;
                json!({"HandleId":handle.to_string(),"Data":data})
            };
            let mut chunks = Vec::new();
            for _ in 0..2 {
                let selector =
                    wrap(json!({"$type":"RaceCheckpointSelector","recordID":n::tweak_id(&record)}));
                chunks.push(json!({"$type":"StreetSignWidgetComponent","signSelector":selector}));
            }
            let trigger = world["triggers"]
                .as_array()
                .context("triggers")?
                .iter()
                .find(|t| t["ref"] == p["checkpoint_trigger"])
                .context("checkpoint trigger")?;
            let width = trigger["outline"]["width"].as_f64().context("gate width")? / 2.;
            let depth = trigger["outline"]["depth"].as_f64().context("gate depth")? / 2.;
            let outline = wrap(
                json!({"$type":"AreaShapeOutline","buffer":"","height":trigger["outline"]["height"],"points":([[-width,-depth],[width,-depth],[width,depth],[-width,depth]].iter().map(|p|json!({"$type":"Vector3","X":p[0],"Y":p[1],"Z":0})).collect::<Vec<_>>())}),
            );
            chunks.push(json!({"$type":"gameStaticTriggerAreaComponent","isEnabled":0,"localTransform":super::resources::transform(-2.,false),"outline":outline}));
            buffer += 1;
            node["Data"]["instanceData"] = wrap(
                json!({"$type":"entEntityInstanceData","buffer":{"BufferId":buffer.to_string(),"Flags":4063232,"Type":super::resources::PACKAGE,"Data":{"Version":4,"Sections":7,"CruidIndex":-1,"CruidDict":{"0":"2451685433260048384","1":"2451685433209716736","2":"1734314053524791296"},"Chunks":chunks}}}),
            );
        }
    }
    Ok(())
}

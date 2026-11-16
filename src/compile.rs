//! Direct resource generation. Inputs are authoring data, never packed donors.
use anyhow::{Context, Result, ensure};
use ghostline_quest_builder::{compiler::Build, native};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

#[path = "compile/actors.rs"]
mod actors;
#[path = "compile/phase.rs"]
mod phase;
#[path = "compile/presentation.rs"]
mod presentation;
#[path = "compile/resources.rs"]
mod resources;
mod series_resources;
mod validation;
#[path = "compile/world.rs"]
mod world;

pub(crate) fn series_resources(
    series: &crate::series::RaceSeries,
    root: &str,
) -> Result<((String, Value), (String, Value))> {
    series_resources::build(series, root)
}
pub(crate) fn series_phase(
    series: &crate::series::RaceSeries,
    races: &[Value],
    depot: &str,
) -> Result<Value> {
    phase::series(series, races, depot)
}
pub(crate) fn audit_series(
    artifacts: &ghostline_quest_builder::compiler::Artifacts,
    bindings: &[Value],
) -> Result<()> {
    let mut all = bindings
        .first()
        .context("Series has no race bindings")?
        .clone();
    all["prefabs"] = json!(
        bindings
            .iter()
            .map(|race| race["prefab"].clone())
            .collect::<Vec<_>>()
    );
    all["native_refs"] = json!(["~/../fluff_vendor_ws"]);
    validation::resources(artifacts, &all)
}

pub fn validate(config: &Value, route: &Value) -> Result<Value> {
    if let Some(membership) = config.get("series") {
        let membership: crate::series::Membership = serde_json::from_value(membership.clone())?;
        membership.validate()?;
        ensure!(
            config["id"] == membership.race,
            "Series membership refers to a different race"
        );
    }
    ensure!(
        config["schema_version"] == 1,
        "Expected race settings schema_version 1"
    );
    let result = crate::route::validate(route, true)?;
    let ns = crate::route::text(config, "id")?;
    ensure!(crate::route::identifier(ns), "Invalid race namespace");
    ensure!(
        !crate::route::text(config, "title")?.trim().is_empty(),
        "Race title is required"
    );
    let entrants = config["entrants"]
        .as_array()
        .context("entrants must be an array")?;
    ensure!((1..=12).contains(&entrants.len()), "Choose 1–12 opponents");
    let grids = crate::route::group(route, "start_grid")
        .iter()
        .filter_map(|m| m["order"].as_u64())
        .collect::<std::collections::HashSet<_>>();
    let mut ids = std::collections::HashSet::new();
    let mut used = std::collections::HashSet::new();
    for entrant in entrants {
        let id = crate::route::text(entrant, "id")?;
        ensure!(
            crate::route::identifier(id) && ids.insert(id),
            "Invalid or duplicate entrant ID {id}"
        );
        let grid = entrant["grid"]
            .as_u64()
            .context("Opponent grid must be a nonnegative integer")?;
        ensure!(
            used.insert(grid) && grids.contains(&grid),
            "Opponent grid {grid} is missing or assigned twice"
        );
        for (key, prefix) in [
            ("vehicle", "Vehicle."),
            ("driver", "Character."),
            ("passenger", "Character."),
        ] {
            if key == "passenger" && entrant.get(key).is_none() {
                continue;
            }
            record(crate::route::text(entrant, key)?, prefix)?;
        }
    }
    if let Some(player) = config.get("player_vehicle") {
        record(crate::route::text(player, "record")?, "Vehicle.")?;
        if let Some(appearance) = player.get("appearance") {
            ensure!(
                appearance.as_str().is_some_and(|s| !s.trim().is_empty()),
                "Player vehicle appearance must be a nonempty name"
            );
        }
    }
    for entrant in entrants {
        for key in [
            "vehicle_appearance",
            "driver_appearance",
            "passenger_appearance",
        ] {
            if let Some(value) = entrant.get(key) {
                ensure!(
                    value.as_str().is_some_and(|s| !s.trim().is_empty()),
                    "{key} must be a nonempty name"
                );
            }
        }
    }
    if let Some(value) = config.get("vehicle_diagnostics") {
        ensure!(value.is_boolean(), "vehicle_diagnostics must be boolean");
    }
    let spacing = config["checkpoint_spacing_m"].as_f64().unwrap_or(40.);
    if let Some(width) = config.get("checkpoint_width_m") {
        ensure!(
            width.as_f64().is_some_and(|w| w.is_finite() && w > 0.),
            "Checkpoint width must be positive"
        );
    }
    ensure!(
        (25. ..=1000.).contains(&spacing),
        "Checkpoint spacing must be 25–1000 m"
    );
    ensure!(
        config["laps"].as_u64().unwrap_or(1) == 1,
        "The native replay lifecycle supports one ordered course per attempt"
    );
    ensure!(
        ["custom", "vanilla"].contains(&config["checkpoint_mode"].as_str().unwrap_or("custom")),
        "Unknown checkpoint mode"
    );
    ensure!(
        ["automatic", "manual"].contains(
            &config["checkpoint_placement"]
                .as_str()
                .unwrap_or("automatic")
        ),
        "Unknown checkpoint placement"
    );
    ensure!(
        ["route", "road_center"]
            .contains(&config["guidance_alignment"].as_str().unwrap_or("route")),
        "Unknown guidance alignment"
    );
    Ok(json!({"valid":true,"complete":true,"marks":result["marks"],"opponents":entrants.len()}))
}
fn record(value: &str, prefix: &str) -> Result<()> {
    ensure!(
        value.starts_with(prefix)
            && value.len() > prefix.len()
            && value[prefix.len()..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "Expected an explicit {prefix} record, got {value}"
    );
    Ok(())
}

pub(crate) fn marker_world(config: &Value, route: &Value) -> Result<(Value, Value)> {
    world::author(config, &crate::route::survey(route)?)
}

pub fn documents(config: &Value, route: &Value) -> Result<(Build, Value, Value)> {
    validate(config, route)?;
    ensure!(
        config.get("actor_ownership").is_none_or(|value| matches!(
            value.as_str(),
            Some("population" | "community" | "scene")
        )),
        "actor_ownership must be population, community or scene"
    );
    let survey = crate::route::survey(route)?;
    let (mut world_spec, mut bindings) = world::author(config, &survey)?;
    actors::reserve_workspots(config, &mut world_spec, &mut bindings)?;
    let cast = actors::prepare(config, &mut world_spec, &mut bindings)?;
    if let Some(series) = config.get("series") {
        bindings["series"] = series.clone();
    }
    let generated = ghostline_quest_builder::world::compile(&world_spec)?;
    let mut artifacts = generated.artifacts;
    world::variants(&mut artifacts, &world_spec, &bindings)?;
    if config["checkpoint_mode"] == "vanilla" {
        world::stock_checkpoints(&mut artifacts, &world_spec)?;
    } else {
        for direction in ["Forward", "Left", "Right", "Finish"] {
            let depot = bindings["checkpoint_entities"][direction]
                .as_str()
                .context("checkpoint depot")?;
            artifacts.insert(
                depot.to_owned(),
                resources::checkpoint(config, depot, direction)?,
            );
        }
    }
    let ns = crate::route::text(config, "id")?;
    for (key, result) in [("countdown_scene", false), ("result_scene", true)] {
        let depot = crate::route::text(&bindings, key)?;
        artifacts.insert(
            depot.to_owned(),
            presentation::compile(config, &bindings, depot, result)?,
        );
    }
    let activation = crate::route::text(&bindings, "activation_scene")?;
    artifacts.insert(
        activation.to_owned(),
        presentation::activation(config, &bindings, activation)?,
    );
    for name in ["preparation", "attempt", "lifecycle", "root"] {
        let depot = crate::route::text(&bindings, &format!("{name}_phase"))?;
        artifacts.insert(
            depot.to_owned(),
            phase::compile(name, config, &bindings, depot)?,
        );
    }
    let root = crate::route::text(&bindings, "depot_root")?;
    if let Some(cast) = cast {
        artifacts.extend(actors::compile(&cast, &artifacts)?);
    }
    let journal = format!("{root}\\journal\\{ns}.journal");
    let onscreens = format!("{root}\\localization\\en-us\\onscreens\\{ns}.json");
    artifacts.insert(
        journal.clone(),
        resources::journal(config, &bindings, &journal)?,
    );
    artifacts.insert(
        onscreens.clone(),
        resources::localization(config, &onscreens),
    );
    validation::resources(&artifacts, &bindings)?;
    let registration = json!({"quest":{"phases":[{"parent":"base\\quest\\cyberpunk2077.quest","path":bindings["root_phase"]}]},"streaming":{"blocks":[world_spec["block_path"]]},"journal":[journal],"localization":{"onscreens":{"en-us":[onscreens]}}});
    let mut loose = BTreeMap::new();
    loose.insert(
        format!("{ns}.archive.xl"),
        serde_json::to_vec_pretty(&registration)?,
    );
    let build = Build {
        bootstrap: None,
        artifacts,
        resources: loose,
        assets: BTreeMap::new(),
        required_loose: Default::default(),
        character_bundles: Default::default(),
        dependencies: Default::default(),
        bindings: Vec::new(),
        authoring_sha256: format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&json!({"config":config,"route":route}))?)
        ),
        integration_contracts: Vec::new(),
    };
    Ok((build, bindings, world_spec))
}

pub fn generate(
    config: &Value,
    route: &Value,
    out: &Path,
    schema: Option<&Path>,
    game: Option<&Path>,
) -> Result<Value> {
    // Native phase graphs contain deeply linked cyclic handle arenas. Isolate
    // serialization/readback from the small stacks of GUI and async callers.
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("race-compiler".to_owned())
            .stack_size(64 * 1024 * 1024)
            .spawn_scoped(scope, || generate_inner(config, route, out, schema, game))?
            .join()
            .map_err(|_| anyhow::anyhow!("Race compiler worker panicked"))?
    })
}

fn generate_inner(
    config: &Value,
    route: &Value,
    out: &Path,
    schema: Option<&Path>,
    game: Option<&Path>,
) -> Result<Value> {
    ensure!(!out.exists(), "Build output must be a new directory");
    let projects =
        crate::roads::guidance::prepare(&[json!({"config":config,"route":route})], game, schema)?;
    let route = &projects[0]["route"];
    let documents = documents(config, route)?;
    publish(config, route, out, schema, game, documents)
}

pub(crate) fn publish(
    config: &Value,
    route: &Value,
    out: &Path,
    schema: Option<&Path>,
    game: Option<&Path>,
    documents: (Build, Value, Value),
) -> Result<Value> {
    ensure!(!out.exists(), "Build output must be a new directory");
    let parent = out.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = tempfile::Builder::new()
        .prefix(".race-")
        .tempdir_in(parent)?;
    let (build, bindings, world) = documents;
    let native_schema = if game.is_some() || schema.is_some() {
        Some(ghostline_red::schema::RedSchema::load(game, schema)?)
    } else {
        None
    };
    let binary = native_schema
        .as_ref()
        .map(|schema| native::encode_build(&build, schema, game))
        .transpose()?;
    for (depot, document) in &build.artifacts {
        if build.bootstrap.is_some()
            && depot == ghostline_quest_builder::bootstrap::REGISTERED_SCENE
        {
            ghostline_quest_builder::project::depot_path(depot)?;
        } else {
            safe_depot(depot)?;
        }
        write_json(
            &temporary
                .path()
                .join("source/raw")
                .join(format!("{}.json", depot.replace('\\', "/"))),
            document,
        )?;
    }
    if let Some(binary) = &binary {
        for (depot, bytes) in binary {
            let path = temporary
                .path()
                .join("source/archive")
                .join(depot.replace('\\', "/"));
            fs::create_dir_all(path.parent().context("resource parent")?)?;
            fs::write(path, bytes)?;
        }
    }
    for (relative, bytes) in &build.resources {
        let path = temporary.path().join("source/resources").join(relative);
        fs::create_dir_all(path.parent().context("loose parent")?)?;
        fs::write(path, bytes)?;
    }
    let hashes = build
        .artifacts
        .iter()
        .map(|(p, v)| {
            Ok((
                p.clone(),
                format!("{:x}", Sha256::digest(serde_json::to_vec(v)?)),
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut report = json!({"format":"ghostline-race-build","schema_version":1,"id":config["id"],"title":config["title"],"resource_count":build.artifacts.len(),"native":binary.is_some(),"readback_verified":binary.is_some(),"resources":build.artifacts.keys().collect::<Vec<_>>(),"bindings":bindings,"source_sha256":build.authoring_sha256,"resource_sha256":hashes,"runtime_verified":false,"playtest_required":["route heights and road access","standing activation","crew spawn, mounting and driving","forced player car retirement","HUD and ordered checkpoints","recovery, finish, police and unmount","reward, courier, save/load, cleanup and replay"]});
    if config["format"] == "ghostline-race-series" {
        if !bindings["quest_core_registration"].is_null() {
            write_json(
                &temporary.path().join("quest-core-registration.json"),
                &bindings["quest_core_registration"],
            )?;
            report["quest_core"] = json!({"required":true,"slot":config["quest_core_slot"],"registration":"quest-core-registration.json"});
        }
        report["bootstrap"] = serde_json::to_value(&build.bootstrap)?;
        report["bootstrap_workspots"] = json!(
            build
                .bootstrap
                .as_ref()
                .map(|bootstrap| bootstrap.workspot_count())
        );
        report["native_resource_sha256"] = json!(binary.as_ref().map(|resources| {
            resources
                .iter()
                .map(|(path, bytes)| {
                    (
                        path.clone(),
                        ghostline_quest_builder::evidence::digest(bytes),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        }));
        report["format"] = json!("ghostline-race-series-build");
        report["playtest_required"]
            .as_array_mut()
            .context("playtests")?
            .extend([
                json!("phone introduction and sequential race unlocks"),
                json!("unique wins, save/load and replay"),
                json!("one-time championship payout"),
            ]);
    }
    if let Some(binary) = binary {
        let ns = crate::route::text(config, "id")?;
        let registration_name = format!("{ns}.archive.xl");
        let package = ghostline_quest_builder::package::Package {
            name: ns.to_owned(),
            payloads: binary,
            loose: build
                .resources
                .iter()
                .filter(|(path, _)| **path != registration_name)
                .map(|(p, b)| (p.clone(), b.clone()))
                .collect(),
            registration: serde_json::from_slice(
                build
                    .resources
                    .get(&registration_name)
                    .context("ArchiveXL registration")?,
            )?,
            audit: report.clone(),
        };
        let receipt =
            ghostline_quest_builder::package::publish(&package, &temporary.path().join("package"))?;
        report["package"] = json!({"directory":"package","zip":format!("package/{ns}.zip"),"archive_verified":receipt["archive_verified"],"zip_verified":receipt["zip_verified"],"zip_sha256":receipt["zip_sha256"]});
        match ghostline_quest_builder::package::validate_modio(&package) {
            Ok(()) => {
                let receipt = ghostline_quest_builder::package::publish_modio(
                    &package,
                    &temporary.path().join("modio"),
                )?;
                report["modio"] = json!({
                    "directory":"modio", "zip":format!("modio/{ns}.zip"),
                    "archive_verified":receipt["archive_verified"],
                    "zip_verified":receipt["zip_verified"],
                    "zip_sha256":receipt["zip_sha256"],
                    "console_resource_types_verified":true,
                    "platform_runtime_verified":false
                });
            }
            Err(error) => report["modio_unavailable"] = json!(format!("{error:#}")),
        }
    }
    let mut published_config = config.clone();
    let series = config["format"] == "ghostline-race-series";
    if !series {
        published_config["route"] = json!("route.json");
    }
    if series {
        write_json(&temporary.path().join("series.json"), &published_config)?;
        write_json(&temporary.path().join("series.world.json"), &world)?;
        write_json(&temporary.path().join("series-build.json"), &report)?;
        for project in route.as_array().context("Series source projects")? {
            let id = crate::route::text(&project["route"], "id")?;
            ensure!(crate::route::identifier(id), "Invalid source project ID");
            write_json(
                &temporary.path().join("projects").join(format!("{id}.json")),
                project,
            )?;
        }
    } else {
        for (file, value) in [
            ("race.json", &published_config),
            ("route.json", route),
            ("race.world.json", &world),
            ("race-build.json", &report),
        ] {
            write_json(&temporary.path().join(file), value)?;
        }
    }
    fs::rename(temporary.path(), out).context("Publish completed race build")?;
    Ok(report)
}
pub fn read_json(path: &Path) -> Result<Value> {
    let bytes = fs::read(path)?;
    let mut deserializer = serde_json::Deserializer::from_slice(
        bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes),
    );
    deserializer.disable_recursion_limit();
    Value::deserialize(&mut deserializer).map_err(Into::into)
}
use serde::Deserialize;
pub fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
fn safe_depot(path: &str) -> Result<()> {
    ensure!(
        path.starts_with("mod\\")
            && !path.contains(':')
            && !path.replace('\\', "/").split('/').any(|p| p == ".."),
        "Unsafe owned depot {path}"
    );
    Ok(())
}

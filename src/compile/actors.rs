//! Race cast ownership and native population lifecycle.
use super::resources::entity;
use anyhow::{Context, Result, ensure};
use ghostline_quest_builder::{compiler::Artifacts, graph::Graph, nodes as n};
use serde_json::{Value, json};
#[path = "actors/population.rs"]
mod population;

pub fn group(reference: &str) -> Option<(&str, &str)> {
    let local = reference.rsplit('/').next()?;
    let value = local
        .strip_prefix("#scene_owned_")
        .or_else(|| local.strip_prefix("#population_owned_"))?;
    for suffix in ["entrants", "spectators"] {
        if let Some(ns) = value.strip_suffix(&format!("_{suffix}")) {
            return Some((ns, suffix));
        }
    }
    None
}
pub fn dynamic(reference: &str, entry: &str) -> Option<String> {
    group(reference).map(|(ns, _)| format!("{ns}_{entry}"))
}
pub fn population_reference(reference: &str, entry: &str) -> Option<String> {
    population::reference(reference, entry)
}
pub fn control_path(
    reference: &str,
    action: &str,
    entry: Option<&str>,
    phase: &str,
) -> Result<String> {
    let (ns, group) = group(reference).context("scene-owned group")?;
    Ok(format!(
        "mod\\races\\{ns}\\cast\\{group}_{}_{}_{}.questphase",
        action.to_lowercase(),
        entry.unwrap_or("all"),
        phase
    ))
}

/// Bind community placements to the owning quest slot's reserved world identities.
pub fn reserve_workspots(config: &Value, world: &mut Value, bindings: &mut Value) -> Result<()> {
    if config["actor_ownership"] != "community" {
        return Ok(());
    }
    let Some(slot) = config.get("quest_core_slot").and_then(Value::as_u64) else {
        return Ok(());
    };
    let slot = u16::try_from(slot).context("quest core slot")?;
    let offset = config["quest_core_workspot_offset"].as_u64().unwrap_or(0);
    let mut index = offset;
    for community in world["communities"]
        .as_array_mut()
        .context("race communities")?
    {
        for entry in community["entries"]
            .as_array_mut()
            .context("community entries")?
        {
            for spot in entry["spots"]
                .as_array_mut()
                .context("community placements")?
            {
                spot["ref"] = json!(ghostline_quest_builder::quest_core::workspot_reference(
                    slot,
                    u16::try_from(index).context("reserved workspot index")?,
                )?);
                index += 1;
            }
        }
    }
    bindings["reserved_workspot_count"] = json!(index - offset);
    bindings["reserved_workspot_offset"] = json!(offset);
    Ok(())
}

/// Lower authored cast placements to the selected ownership backend.
pub fn prepare(config: &Value, world: &mut Value, bindings: &mut Value) -> Result<Option<Value>> {
    if config["actor_ownership"] != "scene" && config["actor_ownership"] != "community" {
        return population::prepare(world, bindings).map(Some);
    }
    if config["actor_ownership"] != "scene" {
        return Ok(None);
    }
    let ns = bindings["id"].as_str().context("race ID")?.to_owned();
    let communities = world["communities"]
        .as_array_mut()
        .context("race communities")?;
    let groups = std::mem::take(communities);
    let mut model = Vec::new();
    let mut members = json!({});
    for (index, mut group) in groups.into_iter().enumerate() {
        let kind = if index == 0 { "entrants" } else { "spectators" };
        let reference = format!("#scene_owned_{ns}_{kind}");
        bindings[if index == 0 { "community" } else { "crowd" }] = json!(reference);
        let entries = group["entries"].as_array_mut().context("cast entries")?;
        members[&reference] = json!(
            entries
                .iter()
                .filter(|e| !e["spots"]
                    .as_array()
                    .is_some_and(|spots| spots.iter().any(|s| s["vehicle_spawn"] == true)))
                .map(|e| e["entry"].clone())
                .collect::<Vec<_>>()
        );
        for entry in entries {
            // Johnny is started by his presentation branch, independently of the crowd.
            entry["activate_with_group"] =
                json!(!(kind == "spectators" && entry["entry"] == "johnny"));
            let vehicle = entry["spots"]
                .as_array()
                .context("cast spots")?
                .iter()
                .any(|s| s["vehicle_spawn"] == true);
            for spot in entry["spots"].as_array_mut().context("cast placements")? {
                let reference = spot["ref"].as_str().context("cast marker")?;
                spot["ref"] = json!(format!(
                    "{}/{}",
                    bindings["prefab"].as_str().context("prefab")?,
                    reference
                ));
                let marker = json!({"ref":spot["ref"],"position":spot["position"],"yaw":spot["yaw"],"scene_marker":true,"sector":"always_loaded"});
                world["markers"]
                    .as_array_mut()
                    .context("world markers")?
                    .push(marker);
            }
            if vehicle {
                let spot = entry["spots"]
                    .as_array()
                    .context("cast spots")?
                    .iter()
                    .find(|s| s["phase"] != "finish")
                    .context("vehicle grid placement")?;
                let spawn = format!(
                    "{}_population",
                    spot["ref"].as_str().context("vehicle marker")?
                );
                world["entities"].as_array_mut().context("world entities")?.push(json!({"ref":spawn,"position":spot["position"],"yaw":spot["yaw"],"population_record":entry["character"],"appearance":entry["appearance"],"is_vehicle":true,"sector":"always_loaded","node_data":{"max_streaming_distance":15131.8017578125,"streaming_distance":15131.8017578125}}));
                entry["vehicle_reference"] = json!(spawn);
                let member = entry["entry"].as_str().context("vehicle entry")?;
                if bindings.get("race_vehicle_refs").is_none() {
                    bindings["race_vehicle_refs"] = json!({});
                }
                bindings["race_vehicle_refs"][member] = entry["vehicle_reference"].clone();
            }
        }
        group["ref"] = json!(reference);
        model.push(group);
    }
    bindings["scene_owned_groups"] = members;
    Ok(Some(json!({"groups":model})))
}

fn member_reference(reference: &str, entry: &Value) -> Result<Value> {
    if let Some(vehicle) = entry["vehicle_reference"].as_str() {
        Ok(n::entity(Some(vehicle), &[]))
    } else {
        Ok(entity(
            reference,
            &[entry["entry"].as_str().context("cast entry")?],
        ))
    }
}

fn population(
    g: &mut Graph,
    id: u32,
    reference: &str,
    action: &str,
) -> Result<ghostline_quest_builder::graph::NodeId> {
    let kind = g.wrap(json!({"$type":"questSpawner_NodeType","action":action,"spawnerReference":n::node_ref(reference)}));
    g.node(
        id,
        "questSpawnManagerNodeDefinition",
        &["In"],
        &["Out"],
        json!({"actions":[{"$type":"questSpawnManagerNodeActionEntry","type":kind}]}),
    )
}

fn teleport(
    g: &mut Graph,
    id: u32,
    vehicle: Value,
    marker: &str,
) -> Result<ghostline_quest_builder::graph::NodeId> {
    let destination = g.wrap(json!({"$type":"questUniversalRef","entityReference":n::entity(Some(marker),&[]),"refLocalPlayer":false}));
    let kind = g.wrap(json!({"$type":"questTeleport_NodeType","entityReference":vehicle,"params":{"$type":"questTeleportPuppetParams","destinationRef":destination}}));
    g.node(
        id,
        "questVehicleNodeDefinition",
        &["In"],
        &["Out"],
        json!({"type":kind}),
    )
}

fn wait(
    g: &mut Graph,
    id: u32,
    reference: Value,
    present: bool,
    vehicle: bool,
) -> Result<ghostline_quest_builder::graph::NodeId> {
    let comparison = if present { "Greater" } else { "Equal" };
    let (class, kind) = if vehicle {
        (
            "questVehicleCondition",
            json!({"$type":"questVehicleSpawned_ConditionType","vehicleType":"EntityReferenced","vehicleRef":reference,"count":0,"comparisonType":comparison}),
        )
    } else {
        let compare = g.wrap(json!({"$type":"questComparisonParam","count":0,"comparisonType":comparison,"entireCommunity":false}));
        (
            "questCharacterCondition",
            json!({"$type":"questCharacterSpawned_ConditionType","objectRef":reference,"comparisonParams":compare}),
        )
    };
    let kind = g.wrap(kind);
    let condition = g.wrap(json!({"$type":class,"type":kind}));
    g.node(
        id,
        "questPauseConditionNodeDefinition",
        &["In"],
        &["Out"],
        json!({"condition":condition}),
    )
}

fn owner(reference: &str, entry: &Value, spot: &Value, phase: &str) -> Result<(String, Value)> {
    let name = entry["entry"].as_str().context("cast entry")?;
    let dynamic = dynamic(reference, name).context("cast dynamic name")?;
    let path = owner_path(reference, name, phase)?;
    let marker = spot["ref"].as_str().context("cast marker")?;
    ensure!(
        spot["vehicle_spawn"] != true,
        "vehicle must use a population spawner"
    );
    let actor = json!({"key":"member","kind":"spawn","dynamic_name":dynamic,"record":entry["character"],"appearance":entry["appearance"],"spawn_marker":marker,"always_spawned":true,"force_max_visibility":true,"spawn_on_start":true,"keep_alive":false,"validate_spawn_position":true,"lipsync":false});
    let mut commands = vec![
        json!({"key":"retire","kind":"wait_fact","fact":format!("{dynamic}_cast_retire"),"on_out":[{"node":"despawn"}]}),
        json!({"key":"despawn","kind":"spawn_actor","dynamic_name":dynamic,"action":"Deactivate","on_out":[{"node":"end"}]}),
    ];
    let mut workspots = Vec::new();
    let mut on_start = vec![json!({"node":"retire"})];
    if group(reference).context("cast group")?.1 != "entrants" {
        commands.extend([
            json!({"key":"ready","kind":"wait_actor_spawned","dynamic_name":dynamic,"on_out":[{"node":"idle"}]}),
            json!({"key":"idle","kind":"use_scene_workspot","dynamic_name":dynamic,"workspot":"idle"}),
        ]);
        workspots.push(json!({"key":"idle","resource":spot.get("workspot").and_then(Value::as_str).unwrap_or("base\\workspots\\common\\ground\\generic__stand_ground__stand_around__07.workspot"),"origin":marker,"play_at_actor_location":true}));
        on_start.push(json!({"node":"ready"}));
    }
    let spec = json!({"name":format!("{dynamic}_{phase}"),"archive_path":path,"speaker_distance_interruptions":false,"actors":[actor],"props":[],"workspots":workspots,"start_node":{"key":"start","on_start":on_start},"entry_point":{"name":"Start","node":"start"},"quest_nodes":commands,"end_node":{"key":"end"},"exit_points":[{"name":"Out","node":"end"}]});
    Ok((
        path,
        ghostline_quest_builder::scene::compile(&spec, &json!({}))?,
    ))
}

fn owner_path(reference: &str, name: &str, phase: &str) -> Result<String> {
    let (ns, _) = group(reference).context("cast group")?;
    Ok(format!("mod\\races\\{ns}\\cast\\{name}_{phase}.scene"))
}

fn command(
    reference: &str,
    entries: &[Value],
    action: &str,
    selector: Option<&str>,
    phase: &str,
) -> Result<(String, Value)> {
    if reference.contains("/#population_owned_") {
        return population::command(reference, entries, action, selector, phase);
    }
    let path = control_path(reference, action, selector, phase)?;
    let mut g = Graph::default();
    let start = n::input(&mut g, "In")?;
    let out = n::output(&mut g, 1, "Out")?;
    let spawning = ["Activate", "Reactivate"].contains(&action);
    let done = if spawning {
        Some(n::output(&mut g, 2, "Done")?)
    } else {
        None
    };
    let selected = entries
        .iter()
        .filter(|e| selector.is_none_or(|name| e["entry"] == name))
        .filter(|e| selector.is_some() || !spawning || e["activate_with_group"] != false)
        .collect::<Vec<_>>();
    ensure!(!selected.is_empty(), "empty cast selection");
    if ["Activate", "Reactivate"].contains(&action) {
        g.set(out, "type", json!("NonTerminating"))?;
    }
    if action == "ResetKillCount" {
        let reset = n::fact(
            &mut g,
            5,
            &format!("{}_cast_killed", group(reference).context("cast group")?.0),
            0,
        )?;
        g.then(start, reset)?;
        g.then(reset, out)?;
        return Ok((path, g.document("cast-command", &[])?));
    }
    if ["Immortal", "Mortal"].contains(&action) {
        let mut previous = start;
        for (index, entry) in selected.iter().enumerate() {
            let subtype = g.wrap(json!({"$type":"questCharacterManagerParameters_SetMortality","puppetRef":member_reference(reference,entry)?,"isPlayer":false,"state":action,"source":n::cname(group(reference).context("cast group")?.0),"resetToDefault":false}));
            let kind = g.wrap(
                json!({"$type":"questCharacterManagerParameters_NodeType","subtype":subtype}),
            );
            let node = g.node(
                10 + index as u32,
                "questCharacterManagerNodeDefinition",
                &["In"],
                &["Out"],
                json!({"type":kind}),
            )?;
            g.then(previous, node)?;
            previous = node;
        }
        g.then(previous, out)?;
        return Ok((path, g.document("cast-command", &[])?));
    }
    let ready = n::join(&mut g, 3, selected.len(), true)?;
    let retired = if spawning {
        Some(n::join(&mut g, 4, selected.len(), true)?)
    } else {
        None
    };
    g.connect(ready, "Out1", out, "In")?;
    if let Some(retired) = retired {
        g.connect(retired, "Out1", done.context("cast completion")?, "In")?;
    }
    for (index, entry) in selected.iter().enumerate() {
        let name = entry["entry"].as_str().context("cast entry")?;
        let dynamic = dynamic(reference, name).context("dynamic cast member")?;
        let stop = format!("{dynamic}_cast_retire");
        let id = 10 + index as u32 * 20;
        let entity_ref = member_reference(reference, entry)?;
        let vehicle = entry["spots"]
            .as_array()
            .context("cast spots")?
            .iter()
            .any(|spot| spot["vehicle_spawn"] == true);
        let stop_node = n::fact(&mut g, id, &stop, 1)?;
        g.then(start, stop_node)?;
        let reset_ready = n::fact(&mut g, id + 5, &format!("{dynamic}_cast_ready"), 0)?;
        g.then(stop_node, reset_ready)?;
        let mut retiring = reset_ready;
        if let Some(reference) = entry["vehicle_reference"].as_str() {
            let despawn = population(&mut g, id + 8, reference, "Deactivate")?;
            g.then(retiring, despawn)?;
            retiring = despawn;
        }
        let absent = if vehicle {
            wait(&mut g, id + 1, entity_ref.clone(), false, true)?
        } else {
            let mut conditions = Vec::new();
            for (fact, comparison, value) in [
                (format!("{dynamic}_cast_spawn_requested"), "Equal", 0),
                (format!("{dynamic}_cast_owner_ended"), "Greater", 0),
            ] {
                let kind = g.wrap(json!({"$type":"questVarComparison_ConditionType","comparisonType":comparison,"factName":fact,"value":value}));
                conditions.push(g.wrap(json!({"$type":"questFactsDBCondition","type":kind})));
            }
            // Scene-owned actors acknowledge retirement when their owning scene ends.
            // CharacterSpawned observes population spawning and can stay true here.
            let condition = g.wrap(
                json!({"$type":"questLogicalCondition","operation":"OR","conditions":conditions}),
            );
            g.node(
                id + 1,
                "questPauseConditionNodeDefinition",
                &["In"],
                &["Out"],
                json!({"condition":condition}),
            )?
        };
        g.then(retiring, absent)?;
        if action == "Deactivate" {
            g.connect(absent, "Out", ready, &format!("In{}", index + 1))?;
            continue;
        }
        let placement = entry["spots"]
            .as_array()
            .context("cast spots")?
            .iter()
            .find(|spot| {
                spot.get("phase")
                    .and_then(Value::as_str)
                    .unwrap_or("default")
                    == phase
            })
            .or_else(|| entry["spots"].as_array()?.first())
            .context("cast placement")?;
        let clear = n::fact(&mut g, id + 2, &stop, 0)?;
        g.then(absent, clear)?;
        let requested = n::fact(
            &mut g,
            id + 6,
            &format!("{dynamic}_cast_spawn_requested"),
            1,
        )?;
        g.then(clear, requested)?;
        let reset_ended = n::fact(&mut g, id + 13, &format!("{dynamic}_cast_owner_ended"), 0)?;
        g.then(requested, reset_ended)?;
        let mut spawned = requested;
        if let Some(reference) = entry["vehicle_reference"].as_str() {
            let activate = population(&mut g, id + 3, reference, "Activate")?;
            g.then(reset_ended, activate)?;
            spawned = activate;
            let stop = n::fact_condition(&mut g, id + 9, &stop, "Greater", 0, false)?;
            g.then(reset_ended, stop)?;
            g.connect(
                stop,
                "Out",
                retired.context("cast retirement join")?,
                &format!("In{}", index + 1),
            )?;
        } else {
            let scene_path = owner_path(reference, name, phase)?;
            let scene = n::scene(
                &mut g,
                id + 3,
                &scene_path,
                placement["ref"].as_str().context("cast marker")?,
                &["Start"],
                &["Out"],
            )?;
            g.set(scene, "notAllowedToBeFrozen", json!(1))?;
            g.connect(reset_ended, "Out", scene, "Start")?;
            let ended = n::fact(&mut g, id + 14, &format!("{dynamic}_cast_owner_ended"), 1)?;
            g.then(scene, ended)?;
            g.connect(
                ended,
                "Out",
                retired.context("cast retirement join")?,
                &format!("In{}", index + 1),
            )?;
        }
        let present = wait(&mut g, id + 4, entity_ref.clone(), true, vehicle)?;
        if !vehicle {
            spawned = reset_ended;
        }
        g.then(spawned, present)?;
        let mut positioned = present;
        if vehicle {
            let move_vehicle = teleport(
                &mut g,
                id + 10,
                entity_ref,
                placement["ref"].as_str().context("vehicle destination")?,
            )?;
            g.then(present, move_vehicle)?;
            positioned = move_vehicle;
        }
        let member_ready = n::fact(&mut g, id + 7, &format!("{dynamic}_cast_ready"), 1)?;
        g.then(positioned, member_ready)?;
        g.connect(member_ready, "Out", ready, &format!("In{}", index + 1))?;
    }
    Ok((path, g.document("cast-command", &[])?))
}

pub fn compile(model: &Value, existing: &Artifacts) -> Result<Artifacts> {
    fn references(value: &Value, paths: &mut std::collections::BTreeSet<String>) {
        if value["$type"] == "ResourcePath" {
            if let Some(path) = value["$value"].as_str() {
                paths.insert(path.to_owned());
            }
        }
        match value {
            Value::Object(fields) => {
                for child in fields.values() {
                    references(child, paths);
                }
            }
            Value::Array(rows) => {
                for child in rows {
                    references(child, paths);
                }
            }
            _ => {}
        }
    }
    let mut requested = std::collections::BTreeSet::new();
    for document in existing.values() {
        references(document, &mut requested);
    }
    let mut artifacts = Artifacts::new();
    for group in model["groups"].as_array().context("cast groups")? {
        let reference = group["ref"].as_str().context("cast group reference")?;
        let entries = group["entries"].as_array().context("cast entries")?;
        for action in [
            "Activate",
            "Reactivate",
            "Deactivate",
            "ResetKillCount",
            "Immortal",
            "Mortal",
        ] {
            for phase in ["default", "finish"] {
                for selector in
                    std::iter::once(None).chain(entries.iter().map(|e| e["entry"].as_str()))
                {
                    if !requested.contains(&control_path(reference, action, selector, phase)?) {
                        continue;
                    }
                    let (path, document) = command(reference, entries, action, selector, phase)
                        .with_context(|| {
                            format!("cast command {reference} {action} {selector:?} {phase}")
                        })?;
                    artifacts.insert(path, document);
                }
            }
        }
    }
    for document in artifacts.values() {
        references(document, &mut requested);
    }
    for group in model["groups"].as_array().context("cast groups")? {
        let reference = group["ref"].as_str().context("cast group reference")?;
        for entry in group["entries"].as_array().context("cast entries")? {
            if entry.get("vehicle_reference").is_some() || reference.contains("/#population_owned_")
            {
                continue;
            }
            for phase in ["default", "finish"] {
                if !requested.contains(&owner_path(
                    reference,
                    entry["entry"].as_str().context("cast entry")?,
                    phase,
                )?) {
                    continue;
                }
                let spot = entry["spots"]
                    .as_array()
                    .context("cast spots")?
                    .iter()
                    .find(|spot| {
                        spot.get("phase")
                            .and_then(Value::as_str)
                            .unwrap_or("default")
                            == phase
                    })
                    .or_else(|| entry["spots"].as_array()?.first())
                    .context("cast spot")?;
                let (path, document) = owner(reference, entry, spot, phase).with_context(|| {
                    format!("cast owner {reference} {} {phase}", entry["entry"])
                })?;
                artifacts.insert(path, document);
            }
        }
    }
    Ok(artifacts)
}

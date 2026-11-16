//! Population-owned cast, with direct world workspots for stationary activities.
use super::*;

pub(super) fn reference(group: &str, entry: &str) -> Option<String> {
    let (prefab, local) = group.rsplit_once('/')?;
    local.strip_prefix("#population_owned_")?;
    let (ns, _) = super::group(group)?;
    Some(format!("{prefab}/#{ns}_{entry}_population"))
}

pub(super) fn prepare(world: &mut Value, bindings: &mut Value) -> Result<Value> {
    let ns = bindings["id"].as_str().context("race ID")?.to_owned();
    let prefab = bindings["prefab"]
        .as_str()
        .context("race prefab")?
        .to_owned();
    let groups = std::mem::take(
        world["communities"]
            .as_array_mut()
            .context("race communities")?,
    );
    let mut model = Vec::new();
    let mut members = json!({});
    bindings["race_vehicle_refs"] = json!({});
    for (index, mut group) in groups.into_iter().enumerate() {
        let kind = if index == 0 { "entrants" } else { "spectators" };
        let group_ref = format!("{prefab}/#population_owned_{ns}_{kind}");
        bindings[if index == 0 { "community" } else { "crowd" }] = json!(group_ref);
        let entries = group["entries"].as_array_mut().context("cast entries")?;
        let mut npc_names = Vec::new();
        for entry in entries {
            let name = entry["entry"].as_str().context("cast member")?.to_owned();
            let spots = entry["spots"].as_array_mut().context("cast spots")?;
            let vehicle = spots.iter().any(|spot| spot["vehicle_spawn"] == true);
            for spot in spots.iter_mut() {
                let local = spot["ref"].as_str().context("cast placement")?;
                spot["ref"] = json!(format!("{prefab}/{local}"));
                world["markers"]
                    .as_array_mut()
                    .context("markers")?
                    .push(json!({
                        "ref": format!("{}_marker", spot["ref"].as_str().context("cast ref")?),
                        "position":spot["position"],"yaw":spot["yaw"],
                        "scene_marker":true,"sector":"always_loaded"
                    }));
                if !vehicle && kind == "spectators" {
                    let mut workspot = spot.clone();
                    workspot["sector"] = json!("always_loaded");
                    workspot["snap_to_ground"] = json!(true);
                    if workspot.get("workspot").is_none() {
                        workspot["workspot"] = json!(
                            "base\\workspots\\common\\ground\\generic__stand_ground__stand_around__07.workspot"
                        );
                    }
                    workspot
                        .as_object_mut()
                        .context("workspot")?
                        .remove("phase");
                    world["ai_spots"]
                        .as_array_mut()
                        .context("world spots")?
                        .push(workspot);
                }
            }
            let spot = spots
                .iter()
                .find(|spot| spot["phase"] != "finish")
                .context("initial placement")?;
            let spawner = reference(&group_ref, &name).context("population reference")?;
            world["entities"].as_array_mut().context("world entities")?.push(json!({
                "ref":spawner,"position":spot["position"],"yaw":spot["yaw"],
                "population_record":entry["character"],"appearance":entry["appearance"],
                "is_vehicle":vehicle,"spawn_on_start":false,"always_spawned":"true",
                "spawn_in_view":"default__true_","prefetch_appearance":true,
                "sector":"always_loaded",
                "node_data":{"max_streaming_distance":15131.8017578125,"streaming_distance":15131.8017578125}
            }));
            entry["population_reference"] = json!(spawner);
            entry["is_vehicle"] = json!(vehicle);
            entry["idle_workspot"] = json!(kind == "spectators");
            entry["activate_with_group"] = json!(!(kind == "spectators" && name == "johnny"));
            if vehicle {
                bindings["race_vehicle_refs"][&name] = json!(spawner);
            } else {
                npc_names.push(name);
            }
        }
        members[&group_ref] = json!(npc_names);
        group["ref"] = json!(group_ref);
        model.push(group);
    }
    bindings["cast_groups"] = members;
    bindings["actor_ownership"] = json!("population");
    bindings["reserved_workspot_count"] = json!(0);
    Ok(json!({"groups":model}))
}

fn position(
    g: &mut Graph,
    id: u32,
    actor: Value,
    marker: &str,
    vehicle: bool,
) -> Result<ghostline_quest_builder::graph::NodeId> {
    if vehicle {
        return super::teleport(g, id, actor, marker);
    }
    let destination = g.wrap(json!({"$type":"questUniversalRef","entityReference":n::entity(Some(marker),&[]),"refLocalPlayer":false}));
    let params = g.wrap(json!({"$type":"questTeleportPuppetParamsV1","destinationRef":destination,"doNavTest":0,"healAtTeleport":0}));
    let actor = g.wrap(json!({"$type":"questUniversalRef","entityReference":actor,"mainPlayerObject":0,"refLocalPlayer":0}));
    g.node(
        id,
        "questTeleportPuppetNodeDefinition",
        &["In"],
        &["Out"],
        json!({"entityReference":actor,"lookAtAction":"Reset","params":params}),
    )
}

pub(super) fn command(
    group: &str,
    entries: &[Value],
    action: &str,
    selector: Option<&str>,
    phase: &str,
) -> Result<(String, Value)> {
    let path = super::control_path(group, action, selector, phase)?;
    let ns = super::group(group).context("cast group")?.0;
    let mut g = Graph::default();
    let start = n::input(&mut g, "In")?;
    let out = n::output(&mut g, 1, "Out")?;
    let spawning = ["Activate", "Reactivate"].contains(&action);
    let selected = entries
        .iter()
        .filter(|entry| selector.is_none_or(|name| entry["entry"] == name))
        .filter(|entry| selector.is_some() || !spawning || entry["activate_with_group"] != false)
        .collect::<Vec<_>>();
    ensure!(!selected.is_empty(), "empty population cast selection");
    if action == "ResetKillCount" {
        let reset = n::fact(&mut g, 5, &format!("{ns}_cast_killed"), 0)?;
        g.then(start, reset)?;
        g.then(reset, out)?;
        return Ok((path, g.document("population-cast-command", &[])?));
    }
    if ["Immortal", "Mortal"].contains(&action) {
        let mut tail = start;
        for (index, entry) in selected.iter().enumerate() {
            if entry["is_vehicle"] == true {
                continue;
            }
            let actor = n::entity(
                Some(entry["population_reference"].as_str().context("spawner")?),
                &[],
            );
            let subtype = g.wrap(json!({"$type":"questCharacterManagerParameters_SetMortality","puppetRef":actor,"isPlayer":false,"state":action,"source":n::cname(ns),"resetToDefault":false}));
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
            g.then(tail, node)?;
            tail = node;
        }
        g.then(tail, out)?;
        return Ok((path, g.document("population-cast-command", &[])?));
    }
    ensure!(
        spawning || action == "Deactivate",
        "unsupported population action {action}"
    );
    let ready = n::join(&mut g, 3, selected.len(), true)?;
    g.connect(ready, "Out1", out, "In")?;
    let retired = if spawning {
        g.set(out, "type", json!("NonTerminating"))?;
        let done = n::output(&mut g, 2, "Done")?;
        let join = n::join(&mut g, 4, selected.len(), true)?;
        g.connect(join, "Out1", done, "In")?;
        Some(join)
    } else {
        None
    };
    for (index, entry) in selected.iter().enumerate() {
        let id = 20 + index as u32 * 20;
        let name = entry["entry"].as_str().context("member")?;
        let spawner = entry["population_reference"].as_str().context("spawner")?;
        let actor = n::entity(Some(spawner), &[]);
        let vehicle = entry["is_vehicle"] == true;
        let retire_fact = format!("{ns}_{name}_cast_retire");
        let retire = n::fact(&mut g, id, &retire_fact, 1)?;
        g.then(start, retire)?;
        let deactivate = n::population(&mut g, id + 1, spawner, "Deactivate")?;
        g.then(retire, deactivate)?;
        let absent = super::wait(&mut g, id + 2, actor.clone(), false, vehicle)?;
        g.then(deactivate, absent)?;
        if !spawning {
            let reset = n::fact(&mut g, id + 3, &format!("{ns}_{name}_cast_ready"), 0)?;
            g.then(absent, reset)?;
            g.connect(reset, "Out", ready, &format!("In{}", index + 1))?;
            continue;
        }
        let clear = n::fact(&mut g, id + 3, &retire_fact, 0)?;
        g.then(absent, clear)?;
        let requested = n::fact(
            &mut g,
            id + 4,
            &format!("{ns}_{name}_cast_spawn_requested"),
            1,
        )?;
        g.then(clear, requested)?;
        let activate = n::population(&mut g, id + 5, spawner, "Activate")?;
        g.then(requested, activate)?;
        let present = super::wait(&mut g, id + 6, actor.clone(), true, vehicle)?;
        g.then(activate, present)?;
        let spots = entry["spots"].as_array().context("spots")?;
        let spot = spots
            .iter()
            .find(|spot| spot["phase"].as_str().unwrap_or("default") == phase)
            .or_else(|| spots.first())
            .context("placement")?;
        let spot_ref = spot["ref"].as_str().context("spot reference")?;
        let marker = format!("{spot_ref}_marker");
        let placed = position(&mut g, id + 7, actor.clone(), &marker, vehicle)?;
        g.then(present, placed)?;
        let member_ready = n::fact(&mut g, id + 9, &format!("{ns}_{name}_cast_ready"), 1)?;
        if entry["idle_workspot"] == true {
            let params = g.wrap(json!({"$type":"questUseWorkspotParamsV1","function":"UseWorkspot","workspotNode":n::node_ref(spot_ref),"isWorkspotInfinite":true,"enableIdleMode":true,"instant":true,"teleport":true,"finishAnimation":false,"repeatCommandOnInterrupt":false}));
            let work = g.node(
                id + 8,
                "questUseWorkspotNodeDefinition",
                &["In"],
                &["Success", "Work Started"],
                json!({"entityReference":actor,"paramsV1":params}),
            )?;
            g.then(placed, work)?;
            g.connect(work, "Work Started", member_ready, "In")?;
        } else {
            g.then(placed, member_ready)?;
        }
        g.connect(member_ready, "Out", ready, &format!("In{}", index + 1))?;
        let stop = n::fact_condition(&mut g, id + 10, &retire_fact, "Greater", 0, false)?;
        // Retirement must also cancel a pending spawn/workspot readiness wait.
        g.then(requested, stop)?;
        g.connect(
            stop,
            "Out",
            retired.context("retirement join")?,
            &format!("In{}", index + 1),
        )?;
    }
    Ok((path, g.document("population-cast-command", &[])?))
}

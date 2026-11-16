use super::*;

pub(super) fn compile(
    mut b: Builder,
    config: &Value,
    bindings: &Value,
    depot: &str,
) -> Result<Value> {
    let ns = binding(bindings, "id")?;
    let community = binding(bindings, "community")?;
    let player = binding(bindings, "player_vehicle")?;
    let mut source = b.start;
    if let Some(success) = b.outputs.get("success") {
        b.g.set(success.node, "type", json!("NonTerminating"))?;
    }
    step!(b, source, restriction(ns, "grid", true));
    trace(&mut b, source, config, bindings, "before_despawn")?;
    if bindings.get("supplied_player_vehicle").is_some() {
        step!(b, source, fact(&format!("{ns}_supplied_car_retire"), 0));
        step!(b, source, population(player, "Deactivate"));
    } else {
        let reference =
            b.g.wrap(json!({"$type":"questUniversalRef","refLocalPlayer":false}));
        step!(
            b,
            source,
            vehicle(
                json!({"$type":"questSpawnPlayerVehicle_NodeType","despawn":true,"despawnAllEnabledVehicles":true,"positionRef":reference,"vehicle":"Currently Active Car"})
            )
        );
    }
    trace(&mut b, source, config, bindings, "despawn_output")?;
    step!(
        b,
        source,
        manager(
            "questTimeManagerNodeDefinition",
            json!({"$type":"questSetTime_NodeType","hours":3,"minutes":0,"seconds":0,"source":n::cname(ns)})
        )
    );
    step!(
        b,
        source,
        manager(
            "questTimeManagerNodeDefinition",
            json!({"$type":"questPauseTime_NodeType","pause":1,"source":n::cname(ns)})
        )
    );
    step!(
        b,
        source,
        manager(
            "questEnvironmentManagerNodeDefinition",
            json!({"$type":"questPlayEnv_SetWeather","weatherID":tweak64("89407126858"),"blendTime":0.0,"source":n::cname(ns)})
        )
    );
    source = environment(&mut b, source, bindings, true)?;
    step!(b, source, delay(0.5));
    trace(&mut b, source, config, bindings, "spawn_requested")?;
    let ready = b.join(2)?;
    let spawn_reference = json!({"$type":"gameEntityReference","reference":n::node_ref(binding(bindings,"player_vehicle_spawn")?)});
    let owned_reference = json!({"$type":"gameEntityReference","reference":n::node_ref(player)});
    let mut owned = source;
    if bindings.get("supplied_player_vehicle").is_some() {
        step!(b, owned, population(player, "Activate"));
    } else {
        let reference=b.g.wrap(json!({"$type":"questUniversalRef","refLocalPlayer":false,"entityReference":spawn_reference}));
        step!(
            b,
            owned,
            vehicle(
                json!({"$type":"questSpawnPlayerVehicle_NodeType","positionRef":reference,"vehicle":"Currently Active Car"})
            )
        );
    }
    trace(&mut b, owned, config, bindings, "spawn_command_output")?;
    step!(
        b,
        owned,
        spawned(
            if player == "#owned_vehicle" {
                owned_reference.clone()
            } else {
                entity(player, &[])
            },
            true
        )
    );
    trace(&mut b, owned, config, bindings, "spawn_condition_passed")?;
    let reference=b.g.wrap(json!({"$type":"questUniversalRef","refLocalPlayer":false,"entityReference":spawn_reference}));
    step!(
        b,
        owned,
        vehicle(
            json!({"$type":"questTeleport_NodeType","entityReference":owned_reference,"params":{"$type":"questTeleportPuppetParams","destinationRef":reference}})
        )
    );
    trace(&mut b, owned, config, bindings, "teleport_command_output")?;
    destination_probe(&mut b, owned, config, bindings)?;
    step!(
        b,
        owned,
        vehicle(
            json!({"$type":"questToggleDoor_NodeType","vehicleRef":owned_reference,"doorAction":"DisableAllInteractions"})
        )
    );
    step!(
        b,
        owned,
        vehicle(
            json!({"$type":"questToggleForceBrake_NodeType","vehicleRef":entity(player,&[]),"playerVehicle":false,"val":true})
        )
    );
    b.link(owned, ready, "In2")?;
    let mut opponents = source;
    step!(b, opponents, variant(bindings, "barriers", true));
    step!(
        b,
        opponents,
        community(community, "Activate", None, "default")
    );
    step!(
        b,
        opponents,
        community(binding(bindings, "crowd")?, "Activate", None, "default")
    );
    let entrants = bindings["entrants"].as_array().context("entrants")?;
    let vehicles_ready = b.join(entrants.len())?;
    for (index, entrant) in entrants.iter().enumerate() {
        let exists = b.spawned(crew(bindings, entrant, "vehicle")?, true)?;
        b.link(opponents, exists, "In")?;
        b.link(exists, vehicles_ready, &format!("In{}", index + 1))?;
    }
    opponents = vehicles_ready;
    step!(
        b,
        opponents,
        community(community, "ResetKillCount", None, "default")
    );
    for entrant in entrants {
        step!(
            b,
            opponents,
            vehicle(
                json!({"$type":"questToggleDoor_NodeType","vehicleRef":crew(bindings,entrant,"vehicle")?,"doorAction":"DisableAllInteractions"})
            )
        );
    }
    b.link(opponents, ready, "In1")?;
    let mut audio = source;
    step!(b, audio, variant(bindings, "race_crowd_finish", false));
    step!(b, audio, variant(bindings, "race_crowd_start", true));
    source = ready;
    step!(b, source, delay(0.2));
    step!(b, source, delay(1.));
    step!(b, source, mortality(ns, community, true, false));
    // Protect the resolved race car while it is spawned and V is still on foot.
    step!(b, source, mortality(ns, player, true, false));
    source = attitude(
        &mut b,
        source,
        entity(binding(bindings, "crowd")?, &[]),
        "friendly",
    )?;
    step!(b, source, fact(&format!("{ns}_crowd_spawned"), 1));
    trace(&mut b, source, config, bindings, "preparation_ready")?;
    step!(b, source, fact("radio_on", 0));
    step!(
        b,
        source,
        variant(bindings, "vehicle_forbidden_start", true)
    );
    trace(
        &mut b,
        source,
        config,
        bindings,
        "waiting_exclusion_streamed",
    )?;
    step!(
        b,
        source,
        vehicle(
            json!({"$type":"questForbiddenTrigger_NodeType","triggerNodeRef":n::node_ref(binding(bindings,"forbidden_start")?),"activate":false})
        )
    );
    trace(
        &mut b,
        source,
        config,
        bindings,
        "waiting_exclusion_disabled",
    )?;
    for door in ["seat_front_left", "seat_front_right"] {
        if bindings.get("supplied_player_vehicle").is_some() {
            step!(
                b,
                source,
                vehicle(
                    json!({"$type":"questToggleDoor_NodeType","vehicleRef":entity(player,&[]),"doorAction":"ForceUnlock","door":door})
                )
            );
        }
        step!(
            b,
            source,
            vehicle(
                json!({"$type":"questToggleDoor_NodeType","vehicleRef":entity(player,&[]),"doorAction":"EnableInteraction","door":door})
            )
        );
    }
    source = remote(&mut b, source, bindings, false)?;
    step!(b, source, restriction(ns, "preparation", true));
    source = content_tokens(&mut b, source, ns, true)?;
    let cancel = b.fact_condition(&format!("{ns}_abort"), "Greater", 0, false)?;
    b.link(b.start, cancel, "In")?;
    for (outcome, source) in b.race(vec![("success", source), ("aborted", cancel)])? {
        b.finish(source, &outcome)?;
    }
    b.document(depot, binding(bindings, "prefab")?)
}

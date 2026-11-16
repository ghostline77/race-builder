use super::*;

pub(super) fn opponents(b: &mut Builder, source: Signal, bindings: &Value) -> Result<Vec<Signal>> {
    let ns = binding(bindings, "id")?;
    let community = binding(bindings, "community")?;
    let source = attitude(b, source, entity(community, &[]), "friendly")?;
    let mut owned = Vec::new();
    for entrant in bindings["entrants"].as_array().context("entrants")? {
        let reference = crew(bindings, entrant, "vehicle")?;
        let finished = b.fact_condition(
            &format!("{ns}_{}_finished", binding(entrant, "id")?),
            "Greater",
            0,
            true,
        )?;
        b.link(source, finished, "In")?;
        let continuing = b.drive(
            reference.clone(),
            binding(bindings, "driving_spline")?,
            binding(bindings, "spline")?,
            80.,
            0.,
            false,
            false,
        )?;
        b.link(Builder::out(finished, 1), continuing, "In")?;
        let parking = b.drive(
            reference.clone(),
            binding(bindings, "driving_spline")?,
            crate::route::text(&bindings["finish_splines"], binding(entrant, "id")?)?,
            80.,
            0.,
            false,
            true,
        )?;
        b.link(finished, parking, "In")?;
        b.link(continuing, parking, "In")?;
        owned.extend([continuing, parking]);
        let wreck=b.typed_condition("questCharacterCondition",json!({"$type":"questCharacterHealth_ConditionType","objectRef":reference,"isPlayer":false}),false)?;
        b.link(source, wreck, "In")?;
        owned.push(wreck);
        let mut respawn = b.cut(wreck, &[continuing, parking], false)?;
        step!(
            b,
            respawn,
            vehicle(
                json!({"$type":"questAssignCharacter_NodeType","characterRef":crew(bindings,entrant,"passenger")?,"vehicleRef":reference,"assign":false,"slotName":n::cname("seat_front_right"),"isInstant":true})
            )
        );
        step!(b, respawn, delay(0.1));
        owned.push(respawn);
        for role in ["driver_entry", "passenger_entry", "vehicle_entry"] {
            step!(
                b,
                respawn,
                community(
                    community,
                    "Reactivate",
                    Some(binding(entrant, role)?),
                    "finish"
                )
            );
        }
        step!(b, respawn, spawned(reference.clone(), true));
        owned.push(respawn);
        step!(
            b,
            respawn,
            typed_condition(
                "questTimeCondition",
                json!({"$type":"questTickDelay_ConditionType","tickCount":5}),
                false
            )
        );
        owned.push(respawn);
        step!(
            b,
            respawn,
            vehicle(json!({"$type":"questSetImmovable_NodeType","vehicleRef":reference}))
        );
        step!(
            b,
            respawn,
            vehicle(
                json!({"$type":"questToggleDoor_NodeType","vehicleRef":reference,"doorAction":"DisableAllInteractions"})
            )
        );
    }
    Ok(owned)
}

pub(super) fn restore_health(b: &mut Builder, source: Signal, ns: &str) -> Result<Signal> {
    let condition=b.typed_condition("questCharacterCondition",json!({"$type":"questCharacterHealth_ConditionType","objectRef":entity("#mounted_vehicle",&[]),"isPlayer":false,"comparisonType":"Less","percent":40.0}),true)?;
    b.link(source, condition, "In")?;
    let mut damaged = condition;
    let subtype=b.g.wrap(json!({"$type":"questCharacterManagerCombat_ModifyHealth","puppetRef":entity("#mounted_vehicle",&[]),"percent":40.0,"setExactValue":true,"noDamageIndicator":true}));
    step!(
        b,
        damaged,
        manager(
            "questCharacterManagerNodeDefinition",
            json!({"$type":"questCharacterManagerCombat_NodeType","subtype":subtype})
        )
    );
    let merge = b.fact(&format!("{ns}_vehicle_restored"), 1)?;
    b.link(damaged, merge, "In")?;
    b.link(Builder::out(condition, 1), merge, "In")?;
    Ok(merge)
}

pub(super) fn return_vehicle(b: &mut Builder, source: Signal, bindings: &Value) -> Result<Signal> {
    let reference = entity(binding(bindings, "player_vehicle")?, &[]);
    let mounted = b.mounting(reference.clone(), None, false, true)?;
    b.link(source, mounted, "In")?;
    let stopped=b.typed_condition("questVehicleCondition",json!({"$type":"questVehicleSpeed_ConditionType","vehicleRef":reference,"comparisonType":"CT_LESS_EQUAL","speed":0.5}),false)?;
    b.link(mounted, stopped, "In")?;
    let exit = b.mounting(reference.clone(), None, true, false)?;
    b.link(stopped, exit, "In")?;
    let request=b.vehicle(json!({"$type":"questAssignCharacter_NodeType","characterRef":entity("#player",&[]),"vehicleRef":reference,"assign":false,"isPlayer":true,"isInstant":false,"slotName":n::cname("seat_front_left"),"entrySlotName":n::cname("default"),"clearAssignedVehicleIdWhenUnmounting":true}))?;
    b.link(stopped, request, "In")?;
    let locked=b.vehicle(json!({"$type":"questToggleDoor_NodeType","vehicleRef":reference,"doorAction":"ForceLock","door":"seat_front_left"}))?;
    b.link(exit, locked, "In")?;
    b.link(Builder::out(mounted, 1), locked, "In")?;
    let mut s = b.once(locked)?;
    step!(
        b,
        s,
        vehicle(
            json!({"$type":"questToggleDoor_NodeType","vehicleRef":reference,"doorAction":"ForceLock","door":"seat_front_right"})
        )
    );
    step!(
        b,
        s,
        vehicle(
            json!({"$type":"questToggleDoor_NodeType","vehicleRef":reference,"doorAction":"DisableAllInteractions"})
        )
    );
    step!(
        b,
        s,
        vehicle(
            json!({"$type":"questSetCanVehicleBeRemoteControlled_NodeType","vehicleRef":reference,"val":false})
        )
    );
    Ok(s)
}

pub(super) fn courier(
    b: &mut Builder,
    source: Signal,
    bindings: &Value,
) -> Result<(Vec<Signal>, Signal)> {
    let ns = binding(bindings, "id")?;
    let mut owned = Vec::new();
    let mut source = source;
    step!(b, source, fact(&format!("{ns}_cour_active"), 1));
    step!(b, source, variant(bindings, "crate", true));
    step!(
        b,
        source,
        variant(bindings, "vehicle_forbidden_courier", true)
    );
    step!(
        b,
        source,
        vehicle(
            json!({"$type":"questForbiddenTrigger_NodeType","triggerNodeRef":n::node_ref(binding(bindings,"forbidden_courier")?),"activate":true})
        )
    );
    let map = b.map_pin(binding(bindings, "reward_mappin")?)?;
    source = port(b, source, map, "Active")?;
    let mut delivered = source;
    step!(
        b,
        delivered,
        area(binding(bindings, "courier_trigger")?, None, false)
    );
    owned.push(delivered);
    for (table, delay) in [("68807470672", false), ("127118606071", true)] {
        if delay {
            step!(b, delivered, delay(0.2));
            owned.push(delivered);
        }
        step!(
            b,
            delivered,
            manager(
                "questItemManagerNodeDefinition",
                json!({"$type":"questReInitContainers_NodeType","params":[{"$type":"questReInitContainers_NodeTypeParams","containerNodeRef":n::node_ref(binding(bindings,"reward_crate")?),"lootTables":[tweak64(table)]}]})
            )
        );
    }
    step!(
        b,
        delivered,
        typed_condition(
            "questObjectCondition",
            json!({"$type":"questInteraction_ConditionType","objectRef":n::node_ref(binding(bindings,"reward_crate")?),"eventType":"Executed"}),
            false
        )
    );
    owned.push(delivered);
    let mut claimed = delivered;
    step!(b, claimed, increment("vehicle_metaquest_discount"));
    step!(b, claimed, fact(&format!("{ns}_reward_taken"), 1));
    let map = b.map_pin(binding(bindings, "reward_mappin")?)?;
    claimed = port(b, claimed, map, "Inactive")?;
    let objective = b.journal(
        binding(bindings, "reward_objective")?,
        "gameJournalQuestObjective",
        2,
    )?;
    claimed = port(b, claimed, objective, "Succeeded")?;
    let mut journal_done = claimed;
    step!(b, journal_done, delay(1.));
    owned.push(journal_done);
    let journal = b.journal(binding(bindings, "journal")?, "gameJournalQuest", 1)?;
    port(b, journal_done, journal, "Inactive")?;
    // The source owns actual courier activation; collection never blocks V/Johnny result VO.
    for reference in ["courier", "courier_bike"] {
        step!(
            b,
            source,
            population(binding(bindings, reference)?, "Activate")
        );
    }
    step!(
        b,
        source,
        spawned(entity(binding(bindings, "courier")?, &[]), false)
    );
    owned.push(source);
    let params=b.g.wrap(json!({"$type":"questUseWorkspotParamsV1","workspotNode":n::node_ref(binding(bindings,"courier_workspot")?)}));
    let waiting = b.node(
        "questUseWorkspotNodeDefinition",
        &["In"],
        &["Out"],
        json!({"entityReference":entity(binding(bindings,"courier")?,&[]),"paramsV1":params}),
    )?;
    owned.push(waiting);
    b.link(source, waiting, "In")?;
    step!(
        b,
        source,
        spawned(entity(binding(bindings, "courier_bike")?, &[]), true)
    );
    owned.push(source);
    step!(b, source, delay(1.));
    let ready = source;
    owned.push(ready);
    let reward = b.journal(
        binding(bindings, "reward_objective")?,
        "gameJournalQuestObjective",
        2,
    )?;
    port(b, ready, reward, "Active")?;
    let mut depart = claimed;
    step!(b, depart, delay(2.));
    owned.push(depart);
    depart = b.cut(depart, &[waiting], false)?;
    step!(
        b,
        depart,
        vehicle(
            json!({"$type":"questAssignCharacter_NodeType","characterRef":entity(binding(bindings,"courier")?,&[]),"vehicleRef":entity(binding(bindings,"courier_bike")?,&[])})
        )
    );
    let mounted = b.mounting(
        entity(binding(bindings, "courier_bike")?, &[]),
        Some(entity(binding(bindings, "courier")?, &[])),
        false,
        false,
    )?;
    b.link(depart, mounted, "In")?;
    owned.push(mounted);
    let distant = nearby(
        b,
        depart,
        entity(binding(bindings, "courier_bike")?, &[]),
        100.,
        true,
        "Greater",
    )?;
    owned.push(distant);
    let mut forced = distant;
    step!(
        b,
        forced,
        vehicle(
            json!({"$type":"questAssignCharacter_NodeType","characterRef":entity(binding(bindings,"courier")?,&[]),"vehicleRef":entity(binding(bindings,"courier_bike")?,&[]),"isInstant":true})
        )
    );
    let mounting = b.fact(&format!("{ns}_courier_mounted"), 1)?;
    b.link(mounted, mounting, "In")?;
    b.link(forced, mounting, "In")?;
    let mounting = b.once(mounting)?;
    let mut source = mounting;
    step!(b, source, delay(1.));
    owned.push(source);
    step!(
        b,
        source,
        vehicle(
            json!({"$type":"questForbiddenTrigger_NodeType","triggerNodeRef":n::node_ref(binding(bindings,"forbidden_courier")?),"activate":false})
        )
    );
    let command = b.drive(
        entity(binding(bindings, "courier_bike")?, &[]),
        binding(bindings, "courier_spline")?,
        binding(bindings, "courier_spline")?,
        70.,
        0.,
        false,
        false,
    )?;
    b.g.set_nested(
        command.node,
        &["commandParams", "additionalParamsOnSpline", "needDriver"],
        json!(true),
    )?;
    b.link(source, command, "In")?;
    owned.push(command);
    let traffic = join_traffic(b, command, entity(binding(bindings, "courier_bike")?, &[]))?;
    owned.push(traffic);
    let mut distant = nearby(
        b,
        mounting,
        entity(binding(bindings, "courier_bike")?, &[]),
        150.,
        true,
        "Greater",
    )?;
    owned.push(distant);
    for reference in ["courier", "courier_bike"] {
        step!(
            b,
            distant,
            population(binding(bindings, reference)?, "Deactivate")
        );
    }
    step!(b, distant, fact(&format!("{ns}_cour_active"), 0));
    step!(b, distant, delay(0.2));
    owned.push(distant);
    step!(b, distant, fact(&format!("{ns}_courier_finished"), 1));
    Ok((owned, ready))
}

pub(super) fn join_traffic(b: &mut Builder, source: Signal, reference: Value) -> Result<Signal> {
    let join = b.g.wrap(json!({"$type":"questvehicleJoinTrafficParams"}));
    let params=b.g.wrap(json!({"$type":"questVehicleCommandParams","type":"Join Traffic","additionalParamsJoinTraffic":join}));
    let node = b.node(
        "questVehicleNodeCommandDefinition",
        &["In"],
        &["Success", "Failure"],
        json!({"vehicle":reference,"commandParams":params}),
    )?;
    port(b, source, node, "In")
}

pub(super) fn police_opponents(b: &mut Builder, source: Signal, bindings: &Value) -> Result<()> {
    let ns = binding(bindings, "id")?;
    let community = binding(bindings, "community")?;
    let begin = b.owned.len();
    let mut traffic = source;
    step!(b, traffic, delay(1.));
    let mut camera = source;
    step!(b, camera, delay(30.));
    let timeout = b.delay(300.)?;
    b.link(source, timeout, "In")?;
    let entrants = bindings["entrants"].as_array().context("entrants")?;
    let all = b.join(entrants.len())?;
    for (index, entrant) in entrants.iter().enumerate() {
        let reference = crew(bindings, entrant, "vehicle")?;
        join_traffic(b, traffic, reference.clone())?;
        let distant = nearby(b, source, reference.clone(), 200., true, "Greater")?;
        let unseen=b.typed_condition("questSystemCondition",json!({"$type":"questCameraFocus_ConditionType","objectRef":reference,"onScreenTest":true,"useFrustrumCheck":true,"inverted":true}),false)?;
        b.link(camera, unseen, "In")?;
        let retired = b.fact(&format!("{ns}_{}_retired", binding(entrant, "id")?), 1)?;
        b.link(distant, retired, "In")?;
        b.link(unseen, retired, "In")?;
        let mut retired = b.once(retired)?;
        for role in ["vehicle_entry", "driver_entry", "passenger_entry"] {
            step!(
                b,
                retired,
                community(
                    community,
                    "Deactivate",
                    Some(binding(entrant, role)?),
                    "default"
                )
            );
        }
        b.link(retired, all, &format!("In{}", index + 1))?;
    }
    let mut departed = all;
    step!(b, departed, delay(1.));
    let owned = b.owned[begin..].to_vec();
    let departed = b.once(departed)?;
    let timed = b.once(timeout)?;
    let a = b.cut(departed, &[timed], false)?;
    let z = b.cut(timed, &[departed], false)?;
    for signal in [a, z] {
        for target in &owned {
            b.g.connect(signal.node, "CutSource", target.node, "CutDestination")?;
        }
    }
    let complete = b.community(community, "Deactivate", None, "default")?;
    b.link(a, complete, "In")?;
    b.link(z, complete, "In")?;
    let mut complete = complete;
    step!(
        b,
        complete,
        community(binding(bindings, "crowd")?, "Deactivate", None, "default")
    );
    step!(b, complete, fact(&format!("{ns}_police_cleanup_done"), 1));
    Ok(())
}

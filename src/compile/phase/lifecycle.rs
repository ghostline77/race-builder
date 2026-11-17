use super::*;

fn fx(b: &mut Builder, source: Signal, bindings: &Value, name: &str) -> Result<Signal> {
    let node=b.manager("questFXManagerNodeDefinition",json!({"$type":"questPlayFX_NodeType","params":[{"$type":"questPlayFX_NodeTypeParams","objectRef":entity(binding(bindings,"crowd")?,&["johnny"]),"effectName":n::cname(name)}]}))?;
    port(b, source, node, "In")
}
fn assign(
    b: &mut Builder,
    source: Signal,
    bindings: &Value,
    entrant: &Value,
    role: &str,
    instant: bool,
) -> Result<Signal> {
    let node=b.vehicle(json!({"$type":"questAssignCharacter_NodeType","characterRef":crew(bindings,entrant,role)?,"vehicleRef":crew(bindings,entrant,"vehicle")?,"slotName":n::cname(if role=="driver"{"seat_front_left"}else{"seat_front_right"}),"isInstant":instant}))?;
    port(b, source, node, "In")
}
fn unseen(b: &mut Builder, source: Signal, reference: Value, interval: f64) -> Result<Signal> {
    let node=b.typed_condition("questSystemCondition",json!({"$type":"questCameraFocus_ConditionType","objectRef":reference,"onScreenTest":true,"useFrustrumCheck":true,"inverted":true,"timeInterval":interval}),false)?;
    port(b, source, node, "In")
}
fn boarding(b: &mut Builder, source: Signal, bindings: &Value) -> Result<Vec<Signal>> {
    let start = b.owned.len();
    let entrants = bindings["entrants"].as_array().context("entrants")?;
    let mut timed = Vec::new();
    for (index, entrant) in entrants.iter().enumerate() {
        match index {
            0 => {
                assign(b, source, bindings, entrant, "passenger", false)?;
                let delay = b.delay(5.)?;
                b.link(source, delay, "In")?;
                timed.push(delay);
                assign(b, delay, bindings, entrant, "driver", false)?;
            }
            1 => {
                let delay = b.delay(50.)?;
                b.link(source, delay, "In")?;
                timed.push(delay);
                let passenger = assign(b, delay, bindings, entrant, "passenger", false)?;
                assign(b, passenger, bindings, entrant, "driver", false)?;
            }
            3 => {
                let delay = b.delay(20.)?;
                b.link(source, delay, "In")?;
                timed.push(delay);
                let passenger = unseen(b, delay, crew(bindings, &entrants[1], "driver")?, 3.)?;
                timed.push(passenger);
                assign(b, passenger, bindings, entrant, "passenger", false)?;
                let driver = unseen(b, delay, crew(bindings, entrant, "passenger")?, 3.)?;
                timed.push(driver);
                assign(b, driver, bindings, entrant, "driver", false)?;
            }
            _ => {
                let delay = b.delay(if index == 4 { 15. } else { 7. })?;
                b.link(source, delay, "In")?;
                timed.push(delay);
                let driver = assign(b, delay, bindings, entrant, "driver", false)?;
                assign(b, driver, bindings, entrant, "passenger", false)?;
            }
        }
    }
    let force = b.fact_condition(
        &format!("{}_everyone_mount", binding(bindings, "id")?),
        "Greater",
        0,
        false,
    )?;
    b.link(source, force, "In")?;
    let mut instant = b.cut(force, &timed, false)?;
    for entrant in entrants {
        instant = assign(b, instant, bindings, entrant, "driver", true)?;
        instant = assign(b, instant, bindings, entrant, "passenger", true)?;
    }
    step!(b, instant, delay(2.));
    let mounted = b.join(entrants.len() * 2)?;
    for (index, entrant) in entrants.iter().enumerate() {
        for (seat, role) in ["driver", "passenger"].iter().enumerate() {
            let check = b.typed_condition(
                "questCharacterCondition",
                json!({"$type":"questCharacterMount_ConditionType","parentRef":crew(bindings,entrant,"vehicle")?,"childRef":crew(bindings,entrant,role)?,"childIsPlayer":false,"anyParent":false,"anyChild":false,"condition":"OnMount","enterAnimationFinished":true,"role":if *role=="driver"{"Driver"}else{"Passenger"}}),
                false,
            )?;
            b.link(instant, check, "In")?;
            b.link(check, mounted, &format!("In{}", index * 2 + seat + 1))?;
        }
    }
    instant = mounted;
    instant = crew_audio(b, instant, bindings, "sq024_race_01_engine_revs")?;
    step!(
        b,
        instant,
        fact(
            &format!("{}_opponents_mounted", binding(bindings, "id")?),
            1
        )
    );
    Ok(b.owned[start..].to_vec())
}

fn johnny(b: &mut Builder, source: Signal, bindings: &Value) -> Result<Vec<Signal>> {
    let begin = b.owned.len();
    let ns = binding(bindings, "id")?;
    let crowd = binding(bindings, "crowd")?;
    let johnny = entity(crowd, &["johnny"]);
    let mut source = source;
    step!(
        b,
        source,
        community(crowd, "Activate", Some("johnny"), "default")
    );
    step!(b, source, spawned(johnny.clone(), false));
    let spawned = source;
    source = nearby(b, source, johnny.clone(), 4., true, "Less")?;
    let first = b.fact_condition("mq056_johnny_you_ready", "Equal", 0, true)?;
    b.link(source, first, "In")?;
    let mut initial = first;
    step!(b, initial, fact("mq056_johnny_you_ready", 1));
    initial = quip(b, initial, bindings, "you_ready")?;
    let look=b.typed_condition("questSystemCondition",json!({"$type":"questCameraFocus_ConditionType","objectRef":johnny,"angleTolerance":180.0,"inverted":true}),false)?;
    b.link(initial, look, "In")?;
    b.link(Builder::out(first, 1), look, "In")?;
    let forced = b.fact_condition(&format!("{ns}_everyone_mount"), "Greater", 0, false)?;
    b.link(spawned, forced, "In")?;
    let request = b.fact(&format!("{ns}_johnny_mount_requested"), 1)?;
    b.link(look, request, "In")?;
    b.link(forced, request, "In")?;
    let once = b.once(request)?;
    let mut source = fx(b, once, bindings, "johnny_teleport_start")?;
    if bindings.get("scene_owned_groups").is_some() || bindings.get("cast_groups").is_some() {
        source = release_workspot(b, source, johnny.clone())?;
    }
    step!(
        b,
        source,
        spawned(entity(binding(bindings, "player_vehicle")?, &[]), true)
    );
    step!(
        b,
        source,
        vehicle(
            json!({"$type":"questAssignCharacter_NodeType","characterRef":johnny,"vehicleRef":entity(binding(bindings,"player_vehicle")?,&[]),"slotName":n::cname("seat_front_right"),"isInstant":true})
        )
    );
    step!(
        b,
        source,
        mounting(
            entity(binding(bindings, "player_vehicle")?, &[]),
            Some(johnny),
            false,
            false
        )
    );
    step!(b, source, fact(&format!("{ns}_johnny_passenger"), 1));
    Ok(b.owned[begin..].to_vec())
}

fn staging(b: &mut Builder, source: Signal, bindings: &Value) -> Result<(Signal, Vec<Signal>)> {
    let start = b.owned.len();
    let mut source = source;
    step!(
        b,
        source,
        area(binding(bindings, "staging_exit")?, None, false)
    );
    step!(
        b,
        source,
        manager(
            "questSceneManagerNodeDefinition",
            json!({"$type":"questSetTier_NodeType","tier":"Tier2_StagedGameplay"})
        )
    );
    step!(
        b,
        source,
        area(binding(bindings, "staging_exit")?, None, true)
    );
    step!(
        b,
        source,
        manager(
            "questSceneManagerNodeDefinition",
            json!({"$type":"questSetTier_NodeType"})
        )
    );
    let journal = b.journal(
        &format!("{}/race", binding(bindings, "journal")?),
        "gameJournalQuestPhase",
        2,
    )?;
    source = port(b, source, journal, "Inactive")?;
    step!(
        b,
        source,
        community(
            binding(bindings, "crowd")?,
            "Deactivate",
            Some("johnny"),
            "default"
        )
    );
    step!(b, source, delay(0.2));
    let journal = b.journal(binding(bindings, "journal")?, "gameJournalQuest", 2)?;
    source = port(b, source, journal, "Inactive")?;
    Ok((source, b.owned[start..].to_vec()))
}

pub(super) fn compile(
    mut b: Builder,
    _config: &Value,
    bindings: &Value,
    depot: &str,
) -> Result<Value> {
    let ns = binding(bindings, "id")?;
    let prefab = binding(bindings, "prefab")?;
    let mut source = b.start;
    let settled = b.fact_condition(&format!("{ns}_settled"), "Greater", 0, true)?;
    b.link(source, settled, "In")?;
    let poi = b.journal(
        binding(bindings, "activation_poi")?,
        "gameJournalPointOfInterestMappin",
        1,
    )?;
    let done = port(&mut b, settled, poi, "Inactive")?;
    b.finish(done, "done")?;
    let poi = b.journal(
        binding(bindings, "activation_poi")?,
        "gameJournalPointOfInterestMappin",
        1,
    )?;
    source = port(&mut b, Builder::out(settled, 1), poi, "Active")?;
    for name in [
        "result",
        "abort",
        "standup",
        "inside_car",
        "countdown_requested",
        "countdown_started",
        "countdown_end",
        "johnny_passenger",
        "everyone_mount",
        "opponents_mounted",
        "prevention_fail",
        "police_cleanup_done",
        "reward_delivered",
        "reward_taken",
        "courier_finished",
        "courier_mounted",
        "save_unlocked",
        "waiting_armed",
        "approach_entered",
        "activation_requested",
        "cleanup_entered",
        "finish_dialogue_started",
        "finish_dialogue_done",
        "johnny_retired",
        "cast_retired",
    ] {
        step!(b, source, fact(&format!("{ns}_{name}"), 0));
    }
    for e in bindings["entrants"].as_array().context("entrants")? {
        step!(
            b,
            source,
            fact(&format!("{ns}_{}_retired", binding(e, "id")?), 0)
        );
    }
    // Completed championship races wait as race POIs, without reopening jobs.
    let replay_waiting = if let Some(series) = bindings.get("series") {
        let membership: crate::series::Membership = serde_json::from_value(series.clone())?;
        let replay = b.fact_condition(&membership.win_fact(), "Greater", 0, true)?;
        b.link(source, replay, "In")?;
        let waiting = b.node(
            "questLogicalHubNodeDefinition",
            &["In1", "In2"],
            &["Out1"],
            json!({"inputSocketCount":2,"outputSocketCount":1}),
        )?;
        b.link(replay, waiting, "In1")?;
        source = Builder::out(replay, 1);
        Some(waiting)
    } else {
        None
    };
    let journal = b.journal(binding(bindings, "journal")?, "gameJournalQuest", 2)?;
    source = port(&mut b, source, journal, "Active")?;
    let objective = b.journal(
        binding(bindings, "activation_objective")?,
        "gameJournalQuestObjective",
        2,
    )?;
    source = port(&mut b, source, objective, "Active")?;
    let map = b.map_pin(binding(bindings, "activation_mappin")?)?;
    source = port(&mut b, source, map, "Active")?;
    if let Some(waiting) = replay_waiting {
        b.link(source, waiting, "In2")?;
        source = waiting;
    }
    step!(b, source, fact(&format!("{ns}_waiting_armed"), 1));
    let approach = b.area(binding(bindings, "staging")?, None, false)?;
    b.link(source, approach, "In")?;
    source = approach;
    step!(b, source, fact(&format!("{ns}_approach_entered"), 1));
    step!(b, source, fact(&format!("{ns}_crowd_spawned"), 0));
    step!(b, source, fact(&format!("{ns}_activation_requested"), 1));
    let outside = b.area(binding(bindings, "staging")?, None, true)?;
    b.link(source, outside, "In")?;
    let entry = format!("{ns}_waiting_in");
    let exit = format!("{ns}_waiting_out");
    let activation = b.scene(
        binding(bindings, "activation_scene")?,
        binding(bindings, "activation")?,
        &[&entry],
        &[&exit, "fade_out"],
    )?;
    b.link(source, activation, &entry)?;
    let mut leaving = b.cut(outside, &[activation], false)?;
    step!(b, leaving, delay(0.2));
    let approach_pause = leaving;
    b.link(leaving, approach, "In")?;
    let mut accepted = b.cut(
        Builder::out(activation, 1),
        &[outside, approach_pause],
        false,
    )?;
    let journal = b.journal(binding(bindings, "journal")?, "gameJournalQuest", 2)?;
    accepted = port(&mut b, accepted, journal, "Active")?;
    let objective = b.journal(
        binding(bindings, "activation_objective")?,
        "gameJournalQuestObjective",
        2,
    )?;
    accepted = port(&mut b, accepted, objective, "Active")?;
    let preparing = b.child(
        binding(bindings, "preparation_phase")?,
        &["Success", "Aborted"],
        prefab,
    )?;
    b.link(accepted, preparing, "In1")?;
    let appearance = johnny(&mut b, accepted, bindings)?;
    source = activation;
    let boarding = boarding(&mut b, source, bindings)?;
    let (staging_left, staging_targets) = staging(&mut b, source, bindings)?;
    let map = b.map_pin(binding(bindings, "activation_mappin")?)?;
    source = port(&mut b, source, map, "Inactive")?;
    let objective = b.journal(
        binding(bindings, "activation_objective")?,
        "gameJournalQuestObjective",
        2,
    )?;
    source = port(&mut b, source, objective, "Succeeded")?;
    let poi = b.journal(
        binding(bindings, "activation_poi")?,
        "gameJournalPointOfInterestMappin",
        1,
    )?;
    source = port(&mut b, source, poi, "Inactive")?;
    let objective = b.journal(
        binding(bindings, "mount_objective")?,
        "gameJournalQuestObjective",
        2,
    )?;
    source = port(&mut b, source, objective, "Active")?;
    let path=b.g.wrap(json!({"$type":"gameJournalPath","realPath":bindings["mount_objective"],"fileEntryIndex":2,"className":n::cname("gameJournalQuestObjective")}));
    step!(
        b,
        source,
        manager(
            "questJournalNodeDefinition",
            json!({"$type":"questJournalTrackQuest_NodeType","path":path})
        )
    );
    let countdown = b.scene(
        binding(bindings, "countdown_scene")?,
        binding(bindings, "activation")?,
        &["announcer_in"],
        &["countdown_go", "announcer_out", "countdown_started"],
    )?;
    let mut announce = source;
    step!(b, announce, delay(1.));
    b.link(announce, countdown, "announcer_in")?;
    let mut started = b.cut(Builder::out(countdown, 2), &staging_targets, false)?;
    step!(b, started, fact(&format!("{ns}_countdown_started"), 1));
    let attempt = b.child(
        binding(bindings, "attempt_phase")?,
        &["Success", "Failure", "Aborted", "Prevention"],
        prefab,
    )?;
    b.link(source, attempt, "In1")?;
    let mut retire = vec![
        activation,
        attempt,
        countdown,
        announce,
        approach,
        outside,
        approach_pause,
    ];
    retire.extend(appearance);
    retire.extend(boarding);
    retire.extend(staging_targets);
    let routes = b.race(vec![
        ("success", attempt),
        ("failure", Builder::out(attempt, 1)),
        ("aborted", Builder::out(attempt, 2)),
        ("prevention", Builder::out(attempt, 3)),
        ("preparation_aborted", Builder::out(preparing, 1)),
        ("staging_left", staging_left),
    ])?;
    for (name, source) in routes {
        let mut s = b.cut(source, &retire, false)?;
        if !["success", "failure"].contains(&name.as_str()) {
            s = b.cut(s, &[preparing], false)?;
        }
        cleanup(&mut b, s, bindings, &name, preparing)?;
    }
    b.document(depot, prefab)
}

fn cleanup(
    b: &mut Builder,
    source: Signal,
    bindings: &Value,
    outcome: &str,
    preparing: Signal,
) -> Result<()> {
    let ns = binding(bindings, "id")?;
    let player = binding(bindings, "player_vehicle")?;
    let police = outcome == "prevention";
    let normal = outcome == "success" || outcome == "failure";
    let win = outcome == "success";
    let mut source = source;
    step!(b, source, fact(&format!("{ns}_cleanup_entered"), 1));
    if police {
        for name in ["checkpoints", "checkpoints2"] {
            step!(b, source, variant(bindings, name, false));
        }
        step!(
            b,
            source,
            manager(
                "questUIManagerNodeDefinition",
                json!({"$type":"questWarningMessage_NodeType","localizedMessage":{"unk1":"0","value":"LocKey#95051"},"duration":5.0,"show":true,"instant":true})
            )
        );
        step!(b, source, mortality(ns, "#player", false, true));
        step!(b, source, mortality(ns, player, false, false));
        step!(b, source, fact(&format!("{ns}_prevention_fail"), 1));
        source = audio_mix(b, source, "sq024_city_race_finish")?;
        let journal = b.journal(
            &format!("{}/race", binding(bindings, "journal")?),
            "gameJournalQuestPhase",
            2,
        )?;
        source = port(b, source, journal, "Inactive")?;
        step!(b, source, delay(0.2));
        source = global_restore(b, source, bindings)?;
        step!(b, source, delay(2.));
        let journal = b.journal(binding(bindings, "journal")?, "gameJournalQuest", 2)?;
        source = port(b, source, journal, "Inactive")?;
        finish::police_opponents(b, source, bindings)?;
    }
    let pin = b.map_pin(binding(bindings, "activation_mappin")?)?;
    source = port(b, source, pin, "Inactive")?;
    let poi = b.journal(
        binding(bindings, "activation_poi")?,
        "gameJournalPointOfInterestMappin",
        1,
    )?;
    source = port(b, source, poi, "Inactive")?;
    for key in [
        "activation_objective",
        "mount_objective",
        "countdown_objective",
    ] {
        let objective = b.journal(binding(bindings, key)?, "gameJournalQuestObjective", 2)?;
        source = port(b, source, objective, "Inactive")?;
    }
    if outcome == "preparation_aborted" || outcome == "staging_left" {
        let params=b.g.wrap(json!({"$type":"questUseWorkspotParamsV1","function":"StopWorkspot","instant":1,"finishAnimation":0}));
        let stop = b.node(
            "questUseWorkspotNodeDefinition",
            &["In"],
            &["Success"],
            json!({"entityReference":entity("#player",&[]),"paramsV1":params}),
        )?;
        b.link(source, stop, "In")?;
    }
    for visual in bindings["checkpoint_visuals"]
        .as_array()
        .context("visuals")?
        .iter()
        .chain(std::iter::once(&bindings["finish_visual"]))
    {
        step!(b, source, show(visual.as_str().context("visual")?, false));
    }
    step!(
        b,
        source,
        vehicle(
            json!({"$type":"questToggleForceBrake_NodeType","vehicleRef":entity(player,&[]),"val":false})
        )
    );
    step!(b, source, ui(bindings, "RaceEnd"));
    step!(
        b,
        source,
        vehicle(json!({"$type":"questStopRace_NodeType"}))
    );
    source = audio_mix(b, source, "sq024_city_race_finish")?;
    source = crew_audio(b, source, bindings, "sq024_race_04_engine_revs_stop_all")?;
    step!(b, source, ui(bindings, "Disable"));
    let mut restrictions = source;
    if normal {
        step!(b, restrictions, delay(3.));
    }
    if !police {
        for group in ["grid", "preparation", "mounted", "countdown"] {
            step!(b, restrictions, restriction(ns, group, false));
        }
        step!(
            b,
            restrictions,
            manager(
                "questGameManagerNodeDefinition",
                json!({"$type":"questGameplayRestrictions_NodeType","action":"RemoveAllRestrictions"})
            )
        );
    }
    let save_ready = if normal {
        let ready = b.join(3)?;
        b.link(restrictions, ready, "In1")?;
        let mut unlocked = ready;
        step!(b, unlocked, delay(1.));
        step!(b, unlocked, lock(ns, false));
        step!(b, unlocked, fact(&format!("{ns}_save_unlocked"), 1));
        Some(ready)
    } else {
        if !police {
            source = restrictions;
            step!(b, source, lock(ns, false));
        }
        None
    };
    if !police {
        step!(b, source, mortality(ns, "#player", false, true));
    }
    step!(b, source, mortality(ns, player, false, false));
    step!(
        b,
        source,
        mortality(ns, binding(bindings, "community")?, false, false)
    );
    if normal {
        source = finish::restore_health(b, source, ns)?;
        if bindings.get("supplied_player_vehicle").is_some() {
            step!(
                b,
                source,
                vehicle(
                    json!({"$type":"questToggleForceBrake_NodeType","vehicleRef":entity(player,&[]),"val":true})
                )
            );
        }
    }
    source = remote(b, source, bindings, true)?;
    step!(b, source, warning(false));
    step!(b, source, tooltip(false));
    for fact in [
        "mq056_johnny_talking",
        "sq024_current_race_checkpoint_fact_add",
        "RacePlayerDistanceFromSpline",
    ] {
        step!(b, source, fact(fact, 0));
    }
    step!(
        b,
        source,
        manager(
            "questSceneManagerNodeDefinition",
            json!({"$type":"questSetTier_NodeType"})
        )
    );
    step!(b, source, fade(true));
    step!(
        b,
        source,
        vehicle(
            json!({"$type":"questToggleDoor_NodeType","vehicleRef":entity(player,&[]),"doorAction":"ResetInteractions"})
        )
    );
    for door in ["seat_front_left", "seat_front_right"] {
        for action in ["ForceUnlock", "EnableInteraction"] {
            step!(
                b,
                source,
                vehicle(
                    json!({"$type":"questToggleDoor_NodeType","vehicleRef":entity(player,&[]),"doorAction":action,"door":door})
                )
            );
        }
    }
    if !police {
        step!(
            b,
            source,
            manager(
                "questTimeManagerNodeDefinition",
                json!({"$type":"questPauseTime_NodeType","pause":0,"source":n::cname(ns)})
            )
        );
        step!(
            b,
            source,
            manager(
                "questEnvironmentManagerNodeDefinition",
                json!({"$type":"questPlayEnv_SetWeather","reset":true,"blendTime":30.0,"source":n::cname(ns)})
            )
        );
    }
    let mut finish_targets = Vec::new();
    let mut handoff = None;
    if normal {
        let mut left = source;
        step!(b, left, area(binding(bindings, "end_area")?, None, true));
        step!(b, left, fact("disable_combat_events", 0));
        let join = b.join(2)?;
        b.link(left, join, "In1")?;
        handoff = Some(join);
        if bindings.get("forbidden_finish").is_some() {
            step!(
                b,
                source,
                variant(bindings, "vehicle_forbidden_finish", true)
            );
        }
        finish_targets = finish::opponents(b, source, bindings)?;
        let mut retire = left;
        step!(b, retire, delay(0.5));
        retire = b.cut(retire, &finish_targets, false)?;
        step!(
            b,
            retire,
            community(
                binding(bindings, "community")?,
                "Deactivate",
                None,
                "default"
            )
        );
        step!(b, source, variant(bindings, "race_crowd_start", false));
        step!(b, source, variant(bindings, "race_crowd_finish", true));
    } else if !police {
        step!(
            b,
            source,
            community(
                binding(bindings, "community")?,
                "Deactivate",
                None,
                "default"
            )
        );
    }
    for name in bindings["variants"].as_object().context("variants")?.keys() {
        if !["barriers", "crate", "race_crowd_start", "race_crowd_finish"].contains(&name.as_str())
            && !name.starts_with("vehicle_forbidden_")
        {
            step!(b, source, variant(bindings, name, false));
        }
    }
    let objective = b.journal(
        binding(bindings, "objective")?,
        "gameJournalQuestObjective",
        2,
    )?;
    source = port(
        b,
        source,
        objective,
        if win { "Succeeded" } else { "Failed" },
    )?;
    if outcome == "failure" {
        let mut closed = source;
        step!(b, closed, delay(1.));
        let journal = b.journal(binding(bindings, "journal")?, "gameJournalQuest", 2)?;
        port(b, closed, journal, "Inactive")?;
        b.link(
            source,
            save_ready.context("normal finish save gate")?,
            "In3",
        )?;
    }
    let mut courier_targets = Vec::new();
    if win {
        let (targets, ready) = finish::courier(b, source, bindings)?;
        courier_targets = targets;
        b.link(ready, save_ready.context("save gate")?, "In3")?;
        let mut pay = source;
        step!(b, pay, delay(2.));
        step!(
            b,
            pay,
            manager(
                "questUIManagerNodeDefinition",
                json!({"$type":"questProgressBar_NodeType","duration":2.5,"text":{"unk1":"0","value":"LocKey#95684"},"type":"Money"})
            )
        );
        step!(b, pay, delay(2.5));
        let player =
            b.g.wrap(json!({"$type":"questUniversalRef","refLocalPlayer":true}));
        let params=b.g.wrap(json!({"$type":"questAddRemoveItem_NodeTypeParams","entityRef":player,"isPlayer":0,"itemID":n::tweak_id("Items.money"),"quantity":10000,"nodeType":"AddItem","objectRef":entity("",&[]),"flagItemAddedCallbackAsSilent":0,"itemIDsToIgnoreOnRemove":[],"removeAllQuantity":0,"sendNotification":1,"tagsToIgnoreOnRemove":[],"tagToRemove":n::cname("None")}));
        let kind =
            b.g.wrap(json!({"$type":"questAddRemoveItem_NodeType","params":[params]}));
        let item = b.node(
            "questItemManagerNodeDefinition",
            &["In"],
            &["Out"],
            json!({"type":kind}),
        )?;
        pay = port(b, pay, item, "In")?;
        step!(b, pay, increment(&format!("{ns}_won")));
        if let Some(series) = bindings.get("series") {
            let membership: crate::series::Membership = serde_json::from_value(series.clone())?;
            step!(b, pay, fact(&membership.win_fact(), 1));
        }
        step!(b, pay, fact(&format!("{ns}_reward_delivered"), 1));
    }
    if normal {
        let name = if win { "win" } else { "lose" };
        step!(b, source, fact(&format!("{ns}_finish_dialogue_started"), 1));
        source = quip(b, source, bindings, name)?;
        step!(b, source, delay(1.));
        source = quip(b, source, bindings, &format!("johnny_{name}"))?;
        step!(b, source, fact(&format!("{ns}_finish_dialogue_done"), 1));
        step!(b, source, delay(2.));
        source = fx(b, source, bindings, "johnny_spawn_normal")?;
        step!(b, source, delay(1.));
    }
    step!(
        b,
        source,
        community(
            binding(bindings, "crowd")?,
            "Deactivate",
            Some("johnny"),
            "default"
        )
    );
    step!(b, source, fact(&format!("{ns}_johnny_retired"), 1));
    if normal && bindings.get("supplied_player_vehicle").is_some() {
        source = finish::return_vehicle(b, source, bindings)?;
    }
    if let Some(ready) = save_ready {
        b.link(source, ready, "In2")?;
    }
    if win {
        for suffix in ["reward_delivered", "reward_taken", "courier_finished"] {
            step!(
                b,
                source,
                fact_condition(&format!("{ns}_{suffix}"), "Greater", 0, false)
            );
        }
    }
    if normal {
        step!(b, source, delay(0.5));
        let join = handoff.context("finish handoff")?;
        b.link(source, join, "In2")?;
        finish_targets.extend(courier_targets);
        source = b.cut(join, &finish_targets, false)?;
        step!(
            b,
            source,
            community(
                binding(bindings, "community")?,
                "Deactivate",
                None,
                "default"
            )
        );
    }
    if !police {
        step!(
            b,
            source,
            community(binding(bindings, "crowd")?, "Deactivate", None, "default")
        );
    }
    step!(b, source, fact(&format!("{ns}_cast_retired"), 1));
    for key in ["courier", "courier_bike"] {
        step!(b, source, population(binding(bindings, key)?, "Deactivate"));
    }
    step!(b, source, variant(bindings, "crate", false));
    for role in ["start", "finish", "courier"] {
        if let Some(reference) = bindings[format!("forbidden_{role}")].as_str() {
            step!(
                b,
                source,
                vehicle(
                    json!({"$type":"questForbiddenTrigger_NodeType","triggerNodeRef":n::node_ref(reference),"activate":false})
                )
            );
            step!(
                b,
                source,
                variant(bindings, &format!("vehicle_forbidden_{role}"), false)
            );
        }
    }
    for name in ["barriers", "race_crowd_start", "race_crowd_finish"] {
        step!(b, source, variant(bindings, name, false));
    }
    source = environment(b, source, bindings, false)?;
    if !police {
        source = content_tokens(b, source, ns, false)?;
    }
    let subtype = b.g.wrap(json!({"$type":"questSpawnToken_NodeSubType"}));
    step!(
        b,
        source,
        manager(
            "questGameManagerNodeDefinition",
            json!({"$type":"questContentTokenManager_NodeType","subtype":subtype})
        )
    );
    step!(b, source, fact("radio_on", 1));
    step!(
        b,
        source,
        fact(
            &format!("{ns}_result"),
            if win {
                1
            } else if outcome == "failure" || police {
                2
            } else {
                3
            }
        )
    );
    step!(b, source, fact(&format!("{ns}_abort"), 0));
    let journal = b.journal(binding(bindings, "journal")?, "gameJournalQuest", 2)?;
    if win {
        source = port(b, source, journal, "Succeeded")?;
    } else if let Some(series) = bindings.get("series") {
        let membership: crate::series::Membership = serde_json::from_value(series.clone())?;
        let won = b.fact_condition(&membership.win_fact(), "Greater", 0, true)?;
        b.link(source, won, "In")?;
        b.link(won, journal, "Succeeded")?;
        b.link(Builder::out(won, 1), journal, "Inactive")?;
        source = journal;
    } else {
        source = port(b, source, journal, "Inactive")?;
    }
    step!(b, source, fact(&format!("{ns}_settled"), 1));
    if normal {
        step!(
            b,
            source,
            area(binding(bindings, "cleanup_area")?, None, true)
        );
        step!(
            b,
            source,
            fact_condition(&format!("{ns}_save_unlocked"), "Greater", 0, false)
        );
    }
    if police {
        step!(b, source, delay(0.5));
        step!(
            b,
            source,
            fact_condition(&format!("{ns}_police_cleanup_done"), "Greater", 0, false)
        );
        step!(b, source, lock(ns, false));
    }
    if bindings.get("supplied_player_vehicle").is_some() {
        step!(b, source, fact(&format!("{ns}_supplied_car_retire"), 1));
    }
    if normal {
        source = b.cut(source, &[preparing], false)?;
    }
    b.finish(source, "done")
}

fn global_restore(b: &mut Builder, mut source: Signal, bindings: &Value) -> Result<Signal> {
    let ns = binding(bindings, "id")?;
    step!(
        b,
        source,
        manager(
            "questGameManagerNodeDefinition",
            json!({"$type":"questGameplayRestrictions_NodeType","action":"RemoveAllRestrictions"})
        )
    );
    source = content_tokens(b, source, ns, false)?;
    step!(b, source, mortality(ns, "#player", false, true));
    for group in ["grid", "preparation", "mounted", "countdown"] {
        step!(b, source, restriction(ns, group, false));
    }
    step!(
        b,
        source,
        manager(
            "questTimeManagerNodeDefinition",
            json!({"$type":"questPauseTime_NodeType","pause":0,"source":n::cname(ns)})
        )
    );
    step!(
        b,
        source,
        manager(
            "questEnvironmentManagerNodeDefinition",
            json!({"$type":"questPlayEnv_SetWeather","reset":true,"blendTime":30.0,"source":n::cname(ns)})
        )
    );
    step!(b, source, fact("radio_on", 1));
    Ok(source)
}

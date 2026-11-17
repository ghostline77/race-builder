use super::*;

fn enable(b: &mut Builder, mut s: Signal, bindings: &Value, reference: &str) -> Result<Signal> {
    step!(b, s, show(reference, true));
    if bindings["checkpoint_mode"] == "vanilla" {
        step!(
            b,
            s,
            manager(
                "questEntityManagerNodeDefinition",
                json!({"$type":"questEntityManagerToggleComponent_NodeType","params":(["ui_front","ui_back","trigger"].iter().map(|name|json!({"$type":"questEntityManagerToggleComponent_NodeTypeParams","objectRef":entity(reference,&[]),"isPlayer":0,"componentName":n::cname(name),"enable":i32::from(*name!="trigger")})).collect::<Vec<_>>())})
            )
        );
    }
    Ok(s)
}

pub(super) fn compile(
    mut b: Builder,
    config: &Value,
    bindings: &Value,
    depot: &str,
) -> Result<Value> {
    let ns = binding(bindings, "id")?;
    let community = binding(bindings, "community")?;
    let player = binding(bindings, "player_vehicle")?;
    let entrants = bindings["entrants"].as_array().context("entrants")?;
    let mut source = b.start;
    for name in ["progress", "enemy_finish", "inside_car", "countdown_end"] {
        step!(b, source, fact(&format!("{ns}_{name}"), 0));
    }
    for entrant in entrants {
        step!(
            b,
            source,
            fact(&format!("{ns}_{}_finished", binding(entrant, "id")?), 0)
        );
        for role in ["driver", "passenger"] {
            step!(
                b,
                source,
                fact(&format!("{ns}_{}_{role}_dead", binding(entrant, "id")?), 0)
            );
        }
        for suffix in ["vehicle_destroyed", "drive_failed"] {
            step!(b, source, fact(&format!("{ns}_{}_{suffix}", binding(entrant, "id")?), 0));
        }
    }
    for name in [
        "sq024_current_race_checkpoint_fact_add",
        "RacePlayerDistanceFromSpline",
    ] {
        step!(b, source, fact(name, 0));
    }
    for visual in bindings["checkpoint_visuals"]
        .as_array()
        .context("visuals")?
        .iter()
        .chain(std::iter::once(&bindings["finish_visual"]))
    {
        step!(b, source, show(visual.as_str().context("visual")?, false));
    }
    step!(b, source, mounting(entity(player, &[]), None, false, false));
    let unmount = b.mounting(entity(player, &[]), None, true, false)?;
    b.link(source, unmount, "In")?;
    step!(b, source, fact(&format!("{ns}_everyone_mount"), 1));
    let mut inside = source;
    step!(b, inside, delay(2.));
    step!(b, inside, fact(&format!("{ns}_inside_car"), 1));
    step!(b, inside, fact(&format!("{ns}_countdown_requested"), 1));
    step!(b, source, lock(ns, true));
    step!(b, source, mortality(ns, "#player", true, true));
    step!(b, source, mortality(ns, player, true, false));
    let target = b.journal(
        binding(bindings, "mount_objective")?,
        "gameJournalQuestObjective",
        2,
    )?;
    source = port(&mut b, source, target, "Succeeded")?;
    let target = b.journal(
        binding(bindings, "countdown_objective")?,
        "gameJournalQuestObjective",
        2,
    )?;
    source = port(&mut b, source, target, "Active")?;
    step!(
        b,
        source,
        vehicle(
            json!({"$type":"questToggleForceBrake_NodeType","vehicleRef":entity(player,&[]),"playerVehicle":false,"val":1})
        )
    );
    step!(
        b,
        source,
        fact_condition(&format!("{ns}_countdown_started"), "Greater", 0, false)
    );
    for name in ["checkpoints", "checkpoints2"] {
        step!(b, source, variant(bindings, name, true));
    }
    source = guidance(&mut b, source, bindings, 0)?;
    trace(
        &mut b,
        source,
        config,
        bindings,
        "course_enabled_before_race_init",
    )?;
    step!(
        b,
        source,
        vehicle(json!({"$type":"questStartRace_NodeType"}))
    );
    trace(&mut b, source, config, bindings, "race_init_command_output")?;
    step!(
        b,
        source,
        manager(
            "questSceneManagerNodeDefinition",
            json!({"$type":"questSetTier_NodeType"})
        )
    );
    source = audio_mix(&mut b, source, "sq024_city_race_begin")?;
    step!(
        b,
        source,
        vehicle(
            json!({"$type":"questToggleDoor_NodeType","vehicleRef":entity("#mounted_vehicle",&[]),"doorAction":"DisableAllInteractions"})
        )
    );
    step!(b, source, restriction(ns, "mounted", true));
    step!(
        b,
        source,
        fact_condition(&format!("{ns}_opponents_mounted"), "Greater", 0, false)
    );
    step!(
        b,
        source,
        fact_condition(&format!("{ns}_countdown_end"), "Greater", 0, false)
    );
    step!(b, source, restriction(ns, "countdown", true));
    step!(b, source, restriction(ns, "preparation", false));
    // Staging protection ends at GO; crew protection keeps its separate grace period.
    for entrant in entrants {
        step!(
            b,
            source,
            actor_mortality(ns, crew(bindings, entrant, "vehicle")?, false, false)
        );
    }
    step!(
        b,
        source,
        vehicle(
            json!({"$type":"questToggleForceBrake_NodeType","vehicleRef":entity(player,&[]),"playerVehicle":false,"val":false})
        )
    );
    let go = crew_audio(
        &mut b,
        source,
        bindings,
        "sq024_race_04_engine_revs_stop_all",
    )?;
    let mut mortal = go;
    for entrant in entrants {
        for role in ["driver", "passenger"] {
            let death=b.typed_condition("questCharacterCondition",json!({"$type":"questCharacterHealth_ConditionType","objectRef":crew(bindings,entrant,role)?,"isPlayer":false,"comparisonType":"LessOrEqual","percent":0.}),false)?;
            b.link(go, death, "In")?;
            let marker = b.fact(&format!("{ns}_{}_{role}_dead", binding(entrant, "id")?), 1)?;
            b.link(death, marker, "In")?;
        }
    }
    step!(b, mortal, delay(10.));
    step!(b, mortal, mortality(ns, community, false, false));
    let veteran = b.fact_condition(&format!("{ns}_won"), "Greater", 2, true)?;
    b.link(go, veteran, "In")?;
    remote(&mut b, Builder::out(veteran, 1), bindings, true)?;
    let mut combat = go;
    step!(b, combat, delay(3.));
    for (i, entrant) in entrants.iter().enumerate() {
        let reference = entity(
            community,
            &[
                binding(entrant, "driver_entry")?,
                binding(entrant, "passenger_entry")?,
            ],
        );
        let mut team = attitude(
            &mut b,
            combat,
            reference,
            &format!("sq024_race_team_{}", i % 5 + 1),
        )?;
        let shooter = crew(bindings, entrant, "passenger")?;
        let record=b.g.wrap(json!({"$type":"questReactionPresetRecordSelector","gangerRecordID":tweak64("144575748405")}));
        let subtype=b.g.wrap(json!({"$type":"questCharacterManagerParameters_SetReactionPreset","puppetRef":shooter,"recordSelector":record}));
        step!(
            b,
            team,
            manager(
                "questCharacterManagerNodeDefinition",
                json!({"$type":"questCharacterManagerParameters_NodeType","subtype":subtype})
            )
        );
        let mut targets = vec![entity("#player", &[])];
        for other in entrants {
            if other["id"] != entrant["id"] {
                targets.push(crew(bindings, other, "passenger")?);
            }
        }
        for target in targets {
            let params=b.g.wrap(json!({"$type":"AIInjectCombatThreatCommandParams","targetPuppetRef":target,"isPersistent":true}));
            let threat = b.node(
                "questCombatNodeDefinition",
                &["In"],
                &["Out"],
                json!({"entityReference":shooter,"params":params}),
            )?;
            b.owned.push(threat);
            b.link(team, threat, "In")?;
        }
    }
    let mut commentary = go;
    step!(
        b,
        commentary,
        fact_condition(&format!("{ns}_johnny_passenger"), "Greater", 0, false)
    );
    step!(
        b,
        commentary,
        fact_condition(&format!("{ns}_countdown_end"), "Greater", 0, false)
    );
    let health_ready = b.fact(&format!("{ns}_health_commentary_requested"), 1)?;
    for percent in [90., 60., 40.] {
        let health=b.typed_condition("questCharacterCondition",json!({"$type":"questCharacterHealth_ConditionType","objectRef":entity("#mounted_vehicle",&[]),"isPlayer":false,"comparisonType":"LessOrEqual","percent":percent}),false)?;
        b.link(commentary, health, "In")?;
        b.link(health, health_ready, "In")?;
    }
    quip(&mut b, health_ready, bindings, "car_hp_drop")?;
    let storm = b.fact_condition("sq024_badlands_sandstorm_entered", "Greater", 0, false)?;
    b.link(commentary, storm, "In")?;
    quip(&mut b, storm, bindings, "sandstorm")?;
    let mut speed = commentary;
    step!(b, speed, delay(30.));
    step!(
        b,
        speed,
        typed_condition(
            "questVehicleCondition",
            json!({"$type":"questVehicleSpeed_ConditionType","vehicleRef":entity("#mounted_vehicle",&[]),"comparisonType":"CT_GREATER_EQUAL","speed":90}),
            false
        )
    );
    let heard = b.fact_condition("mq056_johnny_speed_vo", "Greater", 0, true)?;
    b.link(speed, heard, "In")?;
    let chance = b.node(
        "questRandomizerNodeDefinition",
        &["In"],
        &["Out1", "Out2"],
        json!({}),
    )?;
    b.link(heard, chance, "In")?;
    let mut first = quip(&mut b, Builder::out(heard, 1), bindings, "speeding")?;
    step!(b, first, fact("mq056_johnny_speed_vo", 1));
    quip(&mut b, Builder::out(chance, 1), bindings, "speeding")?;
    let wreck = b.fact(&format!("{ns}_wrecked_nearby"), 1)?;
    let wreck_once = b.once(wreck)?;
    quip(&mut b, wreck_once, bindings, "destroying_enemy")?;
    for entrant in entrants {
        let zero = b.fact_condition(&format!("{ns}_won"), "Equal", 0, true)?;
        b.link(go, zero, "In")?;
        let one = b.fact_condition(&format!("{ns}_won"), "Equal", 1, true)?;
        b.link(Builder::out(zero, 1), one, "In")?;
        let two = b.fact_condition(&format!("{ns}_won"), "Equal", 2, true)?;
        b.link(Builder::out(one, 1), two, "In")?;
        let failed = b.fact(&format!("{ns}_{}_drive_failed", binding(entrant, "id")?), 1)?;
        for (gate, maximum, offset) in [
            (zero, 150., 0.),
            (one, 200., 20.),
            (two, 300., 50.),
            (Builder::out(two, 1), 1000., 80.),
        ] {
            let command = b.drive(
                crew(bindings, entrant, "vehicle")?,
                binding(bindings, "spline")?,
                binding(bindings, "driving_spline")?,
                maximum,
                offset,
                true,
                false,
            )?;
            b.link(gate, command, "In")?;
            b.link(Builder::out(command, 1), failed, "In")?;
        }
        let reference = crew(bindings, entrant, "vehicle")?;
        let destroyed_marker = b.typed_condition("questCharacterCondition", json!({"$type":"questCharacterHealth_ConditionType","objectRef":reference,"isPlayer":false,"comparisonType":"LessOrEqual","percent":0.}), false)?;
        b.link(go, destroyed_marker, "In")?;
        let recorded = b.fact(&format!("{ns}_{}_vehicle_destroyed", binding(entrant, "id")?), 1)?;
        b.link(destroyed_marker, recorded, "In")?;
        let destroyed=b.typed_condition("questCharacterCondition",json!({"$type":"questCharacterHealth_ConditionType","objectRef":reference,"isPlayer":false}),false)?;
        b.link(commentary, destroyed, "In")?;
        let nearby = nearby(&mut b, destroyed, reference.clone(), 50., false, "Less")?;
        let chance = b.node(
            "questRandomizerNodeDefinition",
            &["In"],
            &["Out1", "Out2"],
            json!({}),
        )?;
        b.link(nearby, chance, "In")?;
        b.link(chance, wreck, "In")?;
        let mut opponent = go;
        step!(
            b,
            opponent,
            area(binding(bindings, "finish")?, Some(reference.clone()), true)
        );
        step!(
            b,
            opponent,
            area(binding(bindings, "finish")?, Some(reference), false)
        );
        opponent = b.once(opponent)?;
        step!(
            b,
            opponent,
            fact(&format!("{ns}_{}_finished", binding(entrant, "id")?), 1)
        );
        step!(b, opponent, fact(&format!("{ns}_enemy_finish"), 1));
    }
    let mut progress = go;
    for (i, checkpoint) in bindings["checkpoints"]
        .as_array()
        .context("checkpoints")?
        .iter()
        .enumerate()
    {
        let visual = bindings["checkpoint_visuals"][i]
            .as_str()
            .context("checkpoint visual")?;
        progress = enable(&mut b, progress, bindings, visual)?;
        if bindings["checkpoint_mode"] == "vanilla" {
            let reference = checkpoint.as_str().context("checkpoint")?;
            step!(b, progress, area(reference, None, false));
            step!(
                b,
                progress,
                fact("sq024_current_race_checkpoint_fact_add", i as i64 + 1)
            );
            let sound=b.manager("questAudioNodeDefinition",json!({"$type":"questAudioEventNodeType","event":{"$type":"audioAudEventStruct","event":n::cname("sq024_race_countdown")},"objectRef":entity("#player",&[])}))?;
            b.link(progress, sound, "In")?;
        } else {
            step!(
                b,
                progress,
                fact_condition(
                    "sq024_current_race_checkpoint_fact_add",
                    "GreaterOrEqual",
                    i as i64 + 1,
                    false
                )
            );
        }
        step!(b, progress, show(visual, false));
        step!(b, progress, fact(&format!("{ns}_progress"), i as i64 + 1));
        progress = guidance(&mut b, progress, bindings, i + 1)?;
    }
    progress = enable(
        &mut b,
        progress,
        bindings,
        binding(bindings, "finish_visual")?,
    )?;
    step!(b, progress, area(binding(bindings, "finish")?, None, false));
    if bindings["checkpoint_mode"] == "vanilla" {
        step!(
            b,
            progress,
            fact(
                "sq024_current_race_checkpoint_fact_add",
                bindings["checkpoints"]
                    .as_array()
                    .context("checkpoints")?
                    .len() as i64
                    + 1
            )
        );
    }
    let result = b.fact_condition(&format!("{ns}_enemy_finish"), "Equal", 0, true)?;
    b.link(progress, result, "In")?;
    let abort = b.fact_condition(&format!("{ns}_abort"), "Greater", 0, false)?;
    b.link(b.start, abort, "In")?;
    let police = b.fact_condition("wanted_level", "GreaterOrEqual", 3, false)?;
    b.link(go, police, "In")?;
    recovery(&mut b, go, bindings)?;
    b.outputs.insert(
        "unmounted".to_owned(),
        *b.outputs.get("aborted").context("abort output")?,
    );
    for (outcome, source) in b.race(vec![
        ("success", result),
        ("failure", Builder::out(result, 1)),
        ("aborted", abort),
        ("unmounted", unmount),
        ("prevention", police),
    ])? {
        b.finish(source, &outcome)?;
    }
    b.document(depot, binding(bindings, "prefab")?)
}

fn recovery(b: &mut Builder, go: Signal, bindings: &Value) -> Result<()> {
    let off = b.course(bindings, true)?;
    b.link(go, off, "In")?;
    let mut warned = off;
    step!(b, warned, warning(true));
    let grace = b.delay(2.)?;
    b.link(warned, grace, "In")?;
    let returned = b.course(bindings, false)?;
    b.link(warned, returned, "In")?;
    let mut reset = b.cut(returned, &[grace], false)?;
    step!(b, reset, warning(false));
    b.link(reset, off, "In")?;
    let recover = b.cut(grace, &[], false)?;
    let mut manual = go;
    step!(b, manual, delay(5.));
    step!(b, manual, tooltip(true));
    let pressed=b.typed_condition("questSystemCondition",json!({"$type":"questInputAction_ConditionType","inputAction":n::cname("Exit"),"checkIfButtonAlreadyPressed":false}),false)?;
    b.link(manual, pressed, "In")?;
    let mut held = pressed;
    step!(b, held, delay(0.35));
    let confirmed=b.typed_condition("questSystemCondition",json!({"$type":"questInputAction_ConditionType","inputAction":n::cname("Exit"),"checkIfButtonAlreadyPressed":true}),true)?;
    b.link(held, confirmed, "In")?;
    b.link(confirmed, recover, "In")?;
    b.link(Builder::out(confirmed, 1), pressed, "In")?;
    for target in [returned, grace, off] {
        b.g.connect(recover.node, "CutSource", target.node, "CutDestination")?;
    }
    let mut source = recover;
    step!(b, source, fade(false));
    step!(b, source, delay(0.1));
    step!(
        b,
        source,
        vehicle(json!({"$type":"questGoBackToRace_NodeType"}))
    );
    step!(b, source, delay(1.));
    step!(
        b,
        source,
        vehicle(
            json!({"$type":"questRepair_NodeType","vehicleRef":entity("#mounted_vehicle",&[])})
        )
    );
    step!(b, source, fade(true));
    step!(b, source, warning(false));
    for target in [off, pressed] {
        let flow = b.node(
            "questLogicalHubNodeDefinition",
            &["In1"],
            &["Out1"],
            json!({"inputSocketCount":1,"outputSocketCount":1}),
        )?;
        b.link(source, flow, "In1")?;
        b.link(flow, target, "In")?;
    }
    Ok(())
}

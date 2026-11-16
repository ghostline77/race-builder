//! Phase construction and shared native race commands.
use super::resources::{entity, tweak64};
use anyhow::{Context, Result};
use ghostline_quest_builder::{
    graph::{Graph, NodeId},
    nodes as n,
};
use serde_json::{Value, json};
use std::collections::HashMap;

#[path = "phase/attempt.rs"]
mod attempt;
#[path = "phase/finish.rs"]
mod finish;
#[path = "phase/lifecycle.rs"]
mod lifecycle;
#[path = "phase/prepare.rs"]
mod prepare;
#[path = "phase/series.rs"]
mod series;

pub(super) fn series(
    series: &crate::series::RaceSeries,
    races: &[Value],
    depot: &str,
) -> Result<Value> {
    series::compile(series, races, depot)
}

#[derive(Clone, Copy, Debug)]
struct Signal {
    node: NodeId,
    out: usize,
}
struct Builder {
    g: Graph,
    next: u32,
    ports: HashMap<NodeId, Vec<String>>,
    start: Signal,
    outputs: HashMap<String, Signal>,
    owned: Vec<Signal>,
    scene_owned_groups: HashMap<String, Vec<String>>,
}
impl Builder {
    fn new(outcomes: &[&str]) -> Result<Self> {
        let mut g = Graph::default();
        let start = g.node(
            0,
            "questInputNodeDefinition",
            &[],
            &["Out"],
            json!({"socketName":n::cname("In1")}),
        )?;
        let mut b = Self {
            g,
            next: 10,
            ports: HashMap::from([(start, vec!["Out".to_owned()])]),
            start: Signal {
                node: start,
                out: 0,
            },
            outputs: HashMap::new(),
            owned: Vec::new(),
            scene_owned_groups: HashMap::new(),
        };
        for outcome in outcomes {
            let mut chars = outcome.chars();
            let name = chars
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default()
                + chars.as_str();
            let node = b.node(
                "questOutputNodeDefinition",
                &["In"],
                &[],
                json!({"socketName":n::cname(&name),"type":"Terminating"}),
            )?;
            b.outputs.insert((*outcome).to_owned(), node);
        }
        Ok(b)
    }
    fn node(
        &mut self,
        class: &str,
        inputs: &[&str],
        outputs: &[&str],
        props: Value,
    ) -> Result<Signal> {
        let id = self.next;
        self.next += 1;
        let node = self.g.node(id, class, inputs, outputs, props)?;
        self.ports
            .insert(node, outputs.iter().map(|s| (*s).to_owned()).collect());
        let signal = Signal { node, out: 0 };
        if class == "questPauseConditionNodeDefinition"
            || class == "questVehicleNodeCommandDefinition"
        {
            self.owned.push(signal);
        }
        Ok(signal)
    }
    fn link(&mut self, from: Signal, to: Signal, input: &str) -> Result<()> {
        let output = self
            .ports
            .get(&from.node)
            .and_then(|p| p.get(from.out))
            .context("source port")?
            .clone();
        self.g.connect(from.node, &output, to.node, input)
    }
    fn out(signal: Signal, index: usize) -> Signal {
        Signal {
            node: signal.node,
            out: index,
        }
    }
    fn finish(&mut self, source: Signal, outcome: &str) -> Result<()> {
        let target = *self
            .outputs
            .get(outcome)
            .with_context(|| format!("phase outcome {outcome}"))?;
        self.link(source, target, "In")
    }
    fn document(&self, depot: &str, prefab: &str) -> Result<Value> {
        self.g.document(depot, &[prefab.to_owned()])
    }
    fn manager(&mut self, class: &str, kind: Value) -> Result<Signal> {
        let kind = self.g.wrap(kind);
        self.node(class, &["In"], &["Out"], json!({"type":kind}))
    }
    fn vehicle(&mut self, kind: Value) -> Result<Signal> {
        self.manager("questVehicleNodeDefinition", kind)
    }
    fn fact(&mut self, name: &str, value: i64) -> Result<Signal> {
        self.manager("questFactsDBManagerNodeDefinition",json!({"$type":"questSetVar_NodeType","factName":name,"value":value,"setExactValue":true}))
    }
    fn increment(&mut self, name: &str) -> Result<Signal> {
        self.manager(
            "questFactsDBManagerNodeDefinition",
            json!({"$type":"questSetVar_NodeType","factName":name,"value":1,"setExactValue":false}),
        )
    }
    fn condition(&mut self, condition: Value, snapshot: bool) -> Result<Signal> {
        let condition = self.g.wrap(condition);
        self.node(
            if snapshot {
                "questConditionNodeDefinition"
            } else {
                "questPauseConditionNodeDefinition"
            },
            &["In"],
            if snapshot {
                &["True", "False"]
            } else {
                &["Out"]
            },
            json!({"condition":condition}),
        )
    }
    fn typed_condition(&mut self, class: &str, kind: Value, snapshot: bool) -> Result<Signal> {
        let kind = self.g.wrap(kind);
        self.condition(json!({"$type":class,"type":kind}), snapshot)
    }
    fn fact_condition(
        &mut self,
        name: &str,
        comparison: &str,
        value: i64,
        snapshot: bool,
    ) -> Result<Signal> {
        self.typed_condition("questFactsDBCondition",json!({"$type":"questVarComparison_ConditionType","factName":name,"comparisonType":comparison,"value":value}),snapshot)
    }
    fn delay(&mut self, seconds: f64) -> Result<Signal> {
        self.typed_condition("questTimeCondition",json!({"$type":"questRealtimeDelay_ConditionType","hours":0,"minutes":0,"seconds":seconds.floor()as i64,"miliseconds":(seconds.fract()*1000.).round()as i64}),false)
    }
    fn area(&mut self, reference: &str, activator: Option<Value>, outside: bool) -> Result<Signal> {
        self.condition(json!({"$type":"questTriggerCondition","triggerAreaRef":n::node_ref(reference),"activatorRef":activator.clone().unwrap_or_else(||entity("#player",&[])),"isPlayerActivator":i32::from(activator.is_none()),"type":if outside{"IsOutside"}else{"IsInside"}}),false)
    }
    fn mounting(
        &mut self,
        parent: Value,
        child: Option<Value>,
        unmount: bool,
        snapshot: bool,
    ) -> Result<Signal> {
        let mut kind = json!({"$type":"questCharacterMount_ConditionType","parentRef":parent,"childRef":child.clone().unwrap_or_else(||entity("#player",&[])),"childIsPlayer":child.is_none(),"condition":if unmount{"OnUnmount"}else{"OnMount"}});
        if child.is_none() {
            kind["role"] = json!("Driver");
        }
        self.typed_condition("questCharacterCondition", kind, snapshot)
    }
    fn spawned(&mut self, reference: Value, vehicle: bool) -> Result<Signal> {
        let kind = if vehicle {
            json!({"$type":"questVehicleSpawned_ConditionType","vehicleRef":reference})
        } else {
            let comparison=self.g.wrap(json!({"$type":"questComparisonParam","comparisonType":"Greater","count":0,"entireCommunity":0}));
            json!({"$type":"questCharacterSpawned_ConditionType","objectRef":reference,"comparisonParams":comparison})
        };
        self.typed_condition(
            if vehicle {
                "questVehicleCondition"
            } else {
                "questCharacterCondition"
            },
            kind,
            false,
        )
    }
    fn cut(&mut self, source: Signal, targets: &[Signal], permanent: bool) -> Result<Signal> {
        let cut = self.node(
            "questCutControlNodeDefinition",
            &["In"],
            &["Out", "CutSource"],
            json!({"permanent":i32::from(permanent)}),
        )?;
        self.link(source, cut, "In")?;
        for target in targets {
            self.g
                .connect(cut.node, "CutSource", target.node, "CutDestination")?;
        }
        Ok(cut)
    }
    fn once(&mut self, source: Signal) -> Result<Signal> {
        let gate = self.node(
            "questLogicalXorNodeDefinition",
            &["In1"],
            &["Out1"],
            json!({"inputSocketCount":1,"outputSocketCount":1}),
        )?;
        self.link(source, gate, "In1")?;
        Ok(gate)
    }
    fn race(&mut self, routes: Vec<(&str, Signal)>) -> Result<Vec<(String, Signal)>> {
        let mut gates = Vec::new();
        for (name, s) in routes {
            gates.push((name.to_owned(), self.once(s)?));
        }
        let mut cuts = Vec::new();
        for (name, gate) in &gates {
            // Race phases are entered again for retries and replays. Cancel the
            // current execution without permanently disabling its graph nodes.
            cuts.push((name.clone(), self.cut(*gate, &[], false)?));
        }
        for (name, cut) in &cuts {
            let mut targets = self.owned.clone();
            targets.extend(gates.iter().filter(|(key, _)| key != name).map(|(_, s)| *s));
            targets.extend(cuts.iter().filter(|(key, _)| key != name).map(|(_, s)| *s));
            for target in targets {
                self.g
                    .connect(cut.node, "CutSource", target.node, "CutDestination")?;
            }
        }
        Ok(cuts)
    }
    fn join(&mut self, count: usize) -> Result<Signal> {
        let inputs = (1..=count).map(|i| format!("In{i}")).collect::<Vec<_>>();
        self.node(
            "questLogicalAndNodeDefinition",
            &inputs.iter().map(String::as_str).collect::<Vec<_>>(),
            &["Out1"],
            json!({"inputSocketCount":count,"outputSocketCount":1}),
        )
    }
    fn child(&mut self, depot: &str, outputs: &[&str], prefab: &str) -> Result<Signal> {
        self.node("questPhaseNodeDefinition",&["In1"],outputs,json!({"phaseResource":n::resource(depot),"phaseInstancePrefabs":[{"$type":"questQuestPrefabEntry","prefabNodeRef":n::node_ref(prefab)}]}))
    }
    fn scene(
        &mut self,
        depot: &str,
        origin: &str,
        inputs: &[&str],
        outputs: &[&str],
    ) -> Result<Signal> {
        self.node("questSceneNodeDefinition",inputs,outputs,json!({"interruptionOperations":[],"notAllowedToBeFrozen":0,"reapplyInterruptionOperationsAfterGameLoad":0,"sceneFile":n::resource(depot),"sceneLocation":{"$type":"scnWorldMarker","nodeRef":n::node_ref(origin),"tag":n::cname("None"),"type":"NodeRef"},"syncToMusic":0}))
    }
    fn variant(&mut self, b: &Value, name: &str, show: bool) -> Result<Signal> {
        self.manager("questWorldDataManagerNodeDefinition",json!({"$type":"questTogglePrefabVariant_NodeType","params":[{"$type":"questTogglePrefabVariant_NodeTypeParams","prefabNodeRef":n::node_ref(crate::route::text(b,"prefab")?),"variantStates":[{"$type":"questVariantState","name":n::cname(name),"show":i32::from(show)}]}]}))
    }
    fn community(
        &mut self,
        reference: &str,
        action: &str,
        entry: Option<&str>,
        phase: &str,
    ) -> Result<Signal> {
        if super::actors::group(reference).is_some() {
            let path = super::actors::control_path(reference, action, entry, phase)?;
            let outputs: &[&str] = if ["Activate", "Reactivate"].contains(&action) {
                &["Out", "Done"]
            } else {
                &["Out"]
            };
            return self.node("questPhaseNodeDefinition",&["In"],outputs,json!({"phaseGraph":null,"phaseInstancePrefabs":[],"phaseResource":n::resource(&path),"saveLock":false}));
        }
        let action=self.g.wrap(json!({"$type":"questCommunityTemplate_NodeType","action":action,"spawnerReference":n::node_ref(reference),"communityEntryName":n::cname(entry.unwrap_or("None")),"communityEntryPhaseName":n::cname(phase)}));
        self.node(
            "questSpawnManagerNodeDefinition",
            &["In"],
            &["Out"],
            json!({"actions":[{"$type":"questSpawnManagerNodeActionEntry","type":action}]}),
        )
    }
    fn population(&mut self, reference: &str, action: &str) -> Result<Signal> {
        let action=self.g.wrap(json!({"$type":"questSpawner_NodeType","action":action,"spawnerReference":n::node_ref(reference)}));
        self.node(
            "questSpawnManagerNodeDefinition",
            &["In"],
            &["Out"],
            json!({"actions":[{"$type":"questSpawnManagerNodeActionEntry","type":action}]}),
        )
    }
    fn journal(&mut self, path: &str, class: &str, index: u64) -> Result<Signal> {
        let path=self.g.wrap(json!({"$type":"gameJournalPath","realPath":path,"fileEntryIndex":index,"className":n::cname(class)}));
        let kind=self.g.wrap(json!({"$type":"questJournalQuestEntry_NodeType","path":path,"version":"OptionalProperty","sendNotification":i32::from(class=="gameJournalQuest"),"trackQuest":i32::from(class=="gameJournalQuest"),"optional":0}));
        self.node(
            "questJournalNodeDefinition",
            &["Active", "Inactive", "Succeeded", "Failed"],
            &["Out"],
            json!({"type":kind}),
        )
    }
    fn map_pin(&mut self, path: &str) -> Result<Signal> {
        let path=self.g.wrap(json!({"$type":"gameJournalPath","realPath":path,"fileEntryIndex":2,"className":n::cname("gameJournalQuestMapPin")}));
        self.node(
            "questMappinManagerNodeDefinition",
            &["Active", "Inactive"],
            &["Out"],
            json!({"path":path,"disablePreviousMappins":0}),
        )
    }
    fn ui(&mut self, b: &Value, mode: &str) -> Result<Signal> {
        let event=self.g.wrap(json!({"$type":"VehicleRaceQuestEvent","mode":mode,"maxCheckpoints":b["checkpoints"].as_array().context("checkpoints")?.len()+1,"maxPosition":b["entrants"].as_array().context("entrants")?.len()+1}));
        self.node("questEventManagerNodeDefinition",&["In"],&["Out"],json!({"objectRef":entity("#mounted_vehicle",&[]),"managerName":"VehicleManager","isObjectPlayer":0,"isUiEvent":0,"PSClassName":n::cname("VehicleComponentPS"),"componentName":n::cname("controller"),"event":event}))
    }
    fn show(&mut self, reference: &str, show: bool) -> Result<Signal> {
        self.manager("questWorldDataManagerNodeDefinition",json!({"$type":"questShowWorldNode_NodeType","objectRef":n::node_ref(reference),"show":i32::from(show),"componentName":n::cname("None"),"isPlayer":0}))
    }
    fn restriction(&mut self, ns: &str, group: &str, enable: bool) -> Result<Signal> {
        let ids: &[&str] = match group {
            "grid" => &["163802172868", "151159473294", "131161243162"],
            "preparation" => &["175584140294", "122023708999"],
            "mounted" => &["158010230621", "151247563188", "182938465435"],
            _ => &["158010230621"],
        };
        self.manager("questGameManagerNodeDefinition",json!({"$type":"questGameplayRestrictions_NodeType","source":n::cname(&format!("{ns}_{group}")),"action":if enable{"AddRestriction"}else{"RemoveRestriction"},"restrictionIDs":ids.iter().map(|id|tweak64(id)).collect::<Vec<_>>()}))
    }
    fn mortality(
        &mut self,
        ns: &str,
        reference: &str,
        enable: bool,
        player: bool,
    ) -> Result<Signal> {
        if super::actors::group(reference).is_some() {
            return self.community(
                reference,
                if enable { "Immortal" } else { "Mortal" },
                None,
                "default",
            );
        }
        let subtype=self.g.wrap(json!({"$type":"questCharacterManagerParameters_SetMortality","puppetRef":entity(reference,&[]),"isPlayer":i32::from(player),"state":if enable{"Immortal"}else{"Mortal"},"source":n::cname(ns),"resetToDefault":0}));
        self.manager(
            "questCharacterManagerNodeDefinition",
            json!({"$type":"questCharacterManagerParameters_NodeType","subtype":subtype}),
        )
    }
    fn lock(&mut self, ns: &str, locked: bool) -> Result<Signal> {
        self.node(
            "questCheckpointNodeDefinition",
            &["In"],
            &["Out"],
            json!({"saveLock":i32::from(locked),"debugString":ns}),
        )
    }
    fn fade(&mut self, fade_in: bool) -> Result<Signal> {
        self.manager("questRenderFxManagerNodeDefinition",json!({"$type":"questSetFadeInOut_NodeType","duration":0.0,"fadeColor":{"$type":"Color","Alpha":0,"Red":0,"Blue":0,"Green":0},"fadeIn":i32::from(fade_in)}))
    }
    fn warning(&mut self, show: bool) -> Result<Signal> {
        self.manager("questUIManagerNodeDefinition",json!({"$type":"questWarningMessage_NodeType","message":"RETURN TO THE RACE","localizedMessage":{"unk1":"0","value":if show{"LocKey#78795"}else{"LocKey#78783"}},"show":i32::from(show)}))
    }
    fn tooltip(&mut self, show: bool) -> Result<Signal> {
        self.manager("questUIManagerNodeDefinition",json!({"$type":"questShowCustomTooltip_NodeType","setTooltip":show,"text":{"unk1":"0","value":"LocKey#50189"},"inputAction":"Exit","holdIndicationType":"Hold"}))
    }
    fn course(&mut self, b: &Value, outside: bool) -> Result<Signal> {
        let kind=self.g.wrap(json!({"$type":"questVarComparison_ConditionType","factName":"RacePlayerDistanceFromSpline","comparisonType":if outside{"GreaterOrEqual"}else{"Less"},"value":if outside{50}else{40}}));
        let mut conditions = vec![
            self.g
                .wrap(json!({"$type":"questFactsDBCondition","type":kind})),
        ];
        for reference in b["water_areas"].as_array().into_iter().flatten() {
            conditions.push(self.g.wrap(json!({"$type":"questTriggerCondition","triggerAreaRef":n::node_ref(reference.as_str().context("water reference")?),"activatorRef":entity(crate::route::text(b,"player_vehicle")?,&[]),"isPlayerActivator":0,"type":if outside{"IsInside"}else{"IsOutside"}})));
        }
        self.condition(json!({"$type":"questLogicalCondition","operation":if outside{"OR"}else{"AND"},"conditions":conditions}),false)
    }
    fn drive(
        &mut self,
        reference: Value,
        spline: &str,
        driving: &str,
        maximum: f64,
        offset: f64,
        racing: bool,
        stop: bool,
    ) -> Result<Signal> {
        let local = self
            .g
            .wrap(json!({"$type":"questUniversalRef","mainPlayerObject":true}));
        let rubber=self.g.wrap(json!({"$type":"questParamRubberbanding","targetRef":local,"minDistance":20.0,"maxDistance":maximum,"stayInFront":1,"teleportToCatchUp":0,"targetForwardOffset":offset,"stopAndWait":0}));
        let race=self.g.wrap(json!({"$type":"questvehicleRacingParams","splineRef":n::node_ref(spline),"pushOtherVehiclesAside":false,"rubberBanding":true,"rubberBandingParam":rubber}));
        let local = self
            .g
            .wrap(json!({"$type":"questUniversalRef","mainPlayerObject":true}));
        let keep = self.g.wrap(
            json!({"$type":"questParamKeepDistance","companionTargetRef":local,"distance":10.0}),
        );
        let local = self
            .g
            .wrap(json!({"$type":"questUniversalRef","refLocalPlayer":racing}));
        let rubber=self.g.wrap(json!({"$type":"questParamRubberbanding","targetRef":local,"minDistance":20.0,"maxDistance":70.0,"teleportToCatchUp":racing}));
        let on_spline=self.g.wrap(json!({"$type":"questvehicleOnSplineParams","splineRef":n::node_ref(driving),"keepDistanceParam":keep,"rubberBanding":racing,"rubberBandingParam":rubber,"pushOtherVehiclesAside":false,"stopAtEnd":stop,"needDriver":false}));
        let portals = self.g.wrap(json!({"$type":"vehiclePortalsList"}));
        let to_node = self
            .g
            .wrap(json!({"$type":"questvehicleToNodeParams","portals":portals}));
        let mut params = json!({"$type":"questVehicleCommandParams","type":if racing{"Racing"}else{"OnSpline"},"additionalParamsRacing":race,"additionalParamsOnSpline":on_spline,"additionalParamsToNode":to_node});
        for (key, kind) in [
            ("additionalParamsChase", "questvehicleChaseParams"),
            (
                "additionalParamsJoinTraffic",
                "questvehicleJoinTrafficParams",
            ),
            ("additionalParamsPanic", "questvehiclePanicParams"),
            ("additionalParamsFollow", "questvehicleFollowParams"),
        ] {
            params[key] = self.g.wrap(json!({"$type":kind}));
        }
        let params = self.g.wrap(params);
        self.node(
            "questVehicleNodeCommandDefinition",
            &["In"],
            &["Success", "Failure"],
            json!({"vehicle":reference,"commandParams":params}),
        )
    }
}

// Append to a branch and return its new tail for callers that need it.
macro_rules! step{($b:ident,$source:ident,$method:ident($($args:expr),* $(,)?))=>{{let target=$b.$method($($args),*)?;$b.link($source,target,"In")?;$source=target;$source}};}
pub(super) use step;
fn attitude(b: &mut Builder, source: Signal, reference: Value, group: &str) -> Result<Signal> {
    if let Some(key) = reference["reference"]["$value"].as_str() {
        if let Some(members) = b.scene_owned_groups.get(key).cloned() {
            let names = reference["names"].as_array().context("cast names")?;
            let selected = if names.is_empty() {
                members
            } else {
                names
                    .iter()
                    .map(|n| {
                        n["$value"]
                            .as_str()
                            .map(str::to_owned)
                            .context("cast member")
                    })
                    .collect::<Result<Vec<_>>>()?
            };
            let mut source = source;
            for member in selected {
                source = attitude(b, source, entity(key, &[&member]), group)?;
            }
            return Ok(source);
        }
    }
    let subtype=b.g.wrap(json!({"$type":"questCharacterManagerParameters_SetAttitudeGroupForPuppet","puppetRef":reference,"groupName":n::cname(group)}));
    let node = b.manager(
        "questCharacterManagerNodeDefinition",
        json!({"$type":"questCharacterManagerParameters_NodeType","subtype":subtype}),
    )?;
    port(b, source, node, "In")
}

fn guidance(b: &mut Builder, source: Signal, bindings: &Value, stage: usize) -> Result<Signal> {
    let legs = bindings["guidance_legs"]
        .as_array()
        .context("guidance legs")?;
    let mut states = Vec::new();
    if stage == 0 {
        for leg in legs.iter().take(2) {
            states.push(json!({"$type":"questVariantState","name":n::cname(leg.as_str().context("guidance variant")?),"show":true}));
        }
    } else {
        if let Some(leg) = legs.get(stage - 1) {
            states.push(json!({"$type":"questVariantState","name":n::cname(leg.as_str().context("guidance variant")?),"show":false}));
        }
        if let Some(leg) = legs.get(stage + 1) {
            states.push(json!({"$type":"questVariantState","name":n::cname(leg.as_str().context("guidance variant")?),"show":true}));
        }
    }
    let node = b.manager("questWorldDataManagerNodeDefinition",json!({"$type":"questTogglePrefabVariant_NodeType","params":[{"$type":"questTogglePrefabVariant_NodeTypeParams","prefabNodeRef":n::node_ref(binding(bindings,"prefab")?),"variantStates":states}]}))?;
    port(b, source, node, "In")
}

fn release_workspot(b: &mut Builder, source: Signal, actor: Value) -> Result<Signal> {
    let params = b.g.wrap(json!({"$type":"questUseWorkspotParamsV1","function":"StopWorkspot","instant":true,"finishAnimation":false}));
    let node = b.node(
        "questUseWorkspotNodeDefinition",
        &["In"],
        &["Success"],
        json!({"entityReference":actor,"paramsV1":params}),
    )?;
    port(b, source, node, "In")
}
fn content_tokens(b: &mut Builder, source: Signal, ns: &str, block: bool) -> Result<Signal> {
    let mut operation =
        json!({"$type":"questBlockTokenActivation_NodeSubType","source":n::cname(ns)});
    if !block {
        operation["action"] = json!("UnblockAll");
    }
    let subtype = b.g.wrap(operation);
    let node = b.manager(
        "questGameManagerNodeDefinition",
        json!({"$type":"questContentTokenManager_NodeType","subtype":subtype}),
    )?;
    port(b, source, node, "In")
}
fn audio_mix(b: &mut Builder, source: Signal, event: &str) -> Result<Signal> {
    let target = b.manager(
        "questAudioNodeDefinition",
        json!({"$type":"questAudioMixNodeType","mixSignpost":n::cname(event)}),
    )?;
    port(b, source, target, "In")
}
fn crew_audio(b: &mut Builder, source: Signal, bindings: &Value, event: &str) -> Result<Signal> {
    let entrants = bindings["entrants"].as_array().context("entrants")?;
    let entrant = &entrants[2.min(entrants.len() - 1)];
    let target=b.manager("questAudioNodeDefinition",json!({"$type":"questAudioEventNodeType","event":{"$type":"audioAudEventStruct","event":n::cname(event)},"objectRef":crew(bindings,entrant,"vehicle")?}))?;
    port(b, source, target, "In")
}
fn trace(
    b: &mut Builder,
    source: Signal,
    config: &Value,
    bindings: &Value,
    stage: &str,
) -> Result<()> {
    if config["vehicle_diagnostics"] != true {
        return Ok(());
    }
    for target in ["#player", binding(bindings, "player_vehicle")?] {
        let event=b.g.wrap(json!({"$type":"AITaggedAIEvent","name":n::cname("ghostline_race_vehicle_trace"),"tags":[n::cname(crate::route::text(config,"id")?),n::cname(stage),n::cname(target)]}));
        let node=b.node("questEventManagerNodeDefinition",&["In"],&["Out"],json!({"isObjectPlayer":target=="#player","objectRef":entity(target,&[]),"managerName":"None","event":event}))?;
        b.link(source, node, "In")?;
    }
    Ok(())
}

fn destination_probe(
    b: &mut Builder,
    source: Signal,
    config: &Value,
    bindings: &Value,
) -> Result<()> {
    if config["vehicle_diagnostics"] != true {
        return Ok(());
    }
    let vehicle=b.g.wrap(json!({"$type":"questUniversalRef","refLocalPlayer":false,"entityReference":entity(binding(bindings,"player_vehicle")?,&[])}));
    let distance=b.g.wrap(json!({"$type":"questObjectDistance","entityRef":vehicle,"nodeRef2":entity(binding(bindings,"player_vehicle_spawn")?,&[])}));
    let threshold =
        b.g.wrap(json!({"$type":"questValueDistance","distanceValue":3.0}));
    let probe=b.typed_condition("questDistanceCondition",json!({"$type":"questDistanceComparison_ConditionType","comparisonType":"Less","distanceDefinition1":distance,"distanceDefinition2":threshold}),true)?;
    b.link(source, probe, "In")?;
    trace(b, probe, config, bindings, "destination_within_3m")?;
    trace(
        b,
        Builder::out(probe, 1),
        config,
        bindings,
        "destination_not_within_3m_or_unresolved",
    )
}
fn binding<'a>(b: &'a Value, key: &str) -> Result<&'a str> {
    crate::route::text(b, key)
}
fn nearby(
    b: &mut Builder,
    source: Signal,
    reference: Value,
    distance: f64,
    pause: bool,
    comparison: &str,
) -> Result<Signal> {
    let local =
        b.g.wrap(json!({"$type":"questUniversalRef","refLocalPlayer":true}));
    let object =
        b.g.wrap(json!({"$type":"questObjectDistance","entityRef":local,"nodeRef2":reference}));
    let value =
        b.g.wrap(json!({"$type":"questValueDistance","distanceValue":distance}));
    let kind=b.g.wrap(json!({"$type":"questDistanceComparison_ConditionType","comparisonType":comparison,"distanceDefinition1":object,"distanceDefinition2":value}));
    let condition = b.condition(
        json!({"$type":"questDistanceCondition","type":kind}),
        !pause,
    )?;
    port(b, source, condition, "In")
}
fn crew(b: &Value, e: &Value, role: &str) -> Result<Value> {
    if role == "vehicle" {
        if let Some(reference) = b["race_vehicle_refs"][crate::route::text(e,"vehicle_entry")?].as_str() {
            return Ok(n::entity(Some(reference),&[]));
        }
    }
    Ok(entity(
        binding(b, "community")?,
        &[crate::route::text(e, &format!("{role}_entry"))?],
    ))
}
fn port(b: &mut Builder, source: Signal, target: Signal, input: &str) -> Result<Signal> {
    b.link(source, target, input)?;
    Ok(target)
}
fn environment(b: &mut Builder, mut s: Signal, bindings: &Value, enable: bool) -> Result<Signal> {
    for reference in bindings["crowd_null_areas"]
        .as_array()
        .into_iter()
        .flatten()
    {
        step!(
            b,
            s,
            manager(
                "questCrowdManagerNodeDefinition",
                json!({"$type":"questCrowdManagerNodeType_EnableNullArea","areaReference":n::node_ref(reference.as_str().context("crowd area")?),"enable":i32::from(enable)})
            )
        );
    }
    step!(b, s, fact("disable_combat_events", i64::from(enable)));
    step!(
        b,
        s,
        fact(
            &format!("{}_environment_active", binding(bindings, "id")?),
            i64::from(enable)
        )
    );
    Ok(s)
}
fn remote(b: &mut Builder, source: Signal, bindings: &Value, enabled: bool) -> Result<Signal> {
    let mut refs = bindings["entrants"]
        .as_array()
        .context("entrants")?
        .iter()
        .map(|e| crew(bindings, e, "vehicle"))
        .collect::<Result<Vec<_>>>()?;
    refs.push(entity(binding(bindings, "player_vehicle")?, &[]));
    let ready = b.join(refs.len())?;
    for (i, reference) in refs.into_iter().enumerate() {
        let node=b.vehicle(json!({"$type":"questSetCanVehicleBeRemoteControlled_NodeType","vehicleRef":reference,"val":enabled}))?;
        b.link(source, node, "In")?;
        b.link(node, ready, &format!("In{}", i + 1))?;
    }
    let delay = b.delay(0.2)?;
    port(b, ready, delay, "In")
}
fn quip(b: &mut Builder, source: Signal, bindings: &Value, name: &str) -> Result<Signal> {
    let mut s = source;
    step!(
        b,
        s,
        fact_condition("mq056_johnny_talking", "Equal", 0, false)
    );
    step!(b, s, fact("mq056_johnny_talking", 1));
    let input = format!("{name}_in");
    let output = format!("{name}_out");
    let scene = b.scene(
        binding(bindings, "result_scene")?,
        binding(bindings, "activation")?,
        &[&input],
        &[&output],
    )?;
    b.owned.push(scene);
    s = port(b, s, scene, &input)?;
    step!(b, s, fact("mq056_johnny_talking", 0));
    Ok(s)
}
fn root(b: &mut Builder, bindings: &Value, depot: &str) -> Result<Value> {
    let ns = binding(bindings, "id")?;
    let ready = b.typed_condition(
        "questContentCondition",
        json!({"$type":"questContentLock_ConditionType","isContentBlocked":0}),
        false,
    )?;
    let initial = if let Some(series) = bindings.get("series") {
        let membership: crate::series::Membership = serde_json::from_value(series.clone())?;
        let gate = b.fact_condition(&membership.unlock_fact(), "Greater", 0, false)?;
        b.link(b.start, gate, "In")?;
        gate
    } else {
        b.start
    };
    let reentry = if let Some(series) = bindings.get("series") {
        let membership: crate::series::Membership = serde_json::from_value(series.clone())?;
        let unwon = b.g.wrap(json!({"$type":"questVarComparison_ConditionType","factName":membership.win_fact(),"comparisonType":"Equal","value":0}));
        let unwon =
            b.g.wrap(json!({"$type":"questFactsDBCondition","type":unwon}));
        let completed = b.g.wrap(json!({"$type":"questVarComparison_ConditionType","factName":format!("series_{}_completed",membership.id),"comparisonType":"Greater","value":0}));
        let completed =
            b.g.wrap(json!({"$type":"questFactsDBCondition","type":completed}));
        let eligible = b.condition(json!({"$type":"questLogicalCondition","operation":"OR","conditions":[unwon,completed]}),false)?;
        b.link(initial, eligible, "In")?;
        b.link(eligible, ready, "In")?;
        eligible
    } else {
        b.link(initial, ready, "In")?;
        ready
    };
    let mut source = ready;
    step!(b, source, fact(&format!("{ns}_settled"), 0));
    let call = b.child(
        binding(bindings, "lifecycle_phase")?,
        &["Done"],
        binding(bindings, "prefab")?,
    )?;
    b.link(source, call, "In1")?;
    b.link(call, reentry, "In")?;
    if let Some(reference) = bindings["supplied_player_vehicle_full"].as_str() {
        let fact=b.g.wrap(json!({"$type":"questVarComparison_ConditionType","factName":format!("{ns}_supplied_car_retire"),"comparisonType":"Greater","value":0}));
        let eligible =
            b.g.wrap(json!({"$type":"questFactsDBCondition","type":fact}));
        let local =
            b.g.wrap(json!({"$type":"questUniversalRef","refLocalPlayer":true}));
        let object=b.g.wrap(json!({"$type":"questObjectDistance","entityRef":local,"nodeRef2":entity(reference,&[])}));
        let value =
            b.g.wrap(json!({"$type":"questValueDistance","distanceValue":150.0}));
        let kind=b.g.wrap(json!({"$type":"questDistanceComparison_ConditionType","comparisonType":"Greater","distanceDefinition1":object,"distanceDefinition2":value}));
        let distant =
            b.g.wrap(json!({"$type":"questDistanceCondition","type":kind}));
        let gate=b.condition(json!({"$type":"questLogicalCondition","operation":"AND","conditions":[eligible,distant]}),false)?;
        b.link(b.start, gate, "In")?;
        let mut s = gate;
        step!(b, s, population(reference, "Deactivate"));
        step!(b, s, fact(&format!("{ns}_supplied_car_retire"), 0));
        b.link(s, gate, "In")?;
    }
    b.document(depot, binding(bindings, "prefab")?)
}
pub fn compile(name: &str, config: &Value, bindings: &Value, depot: &str) -> Result<Value> {
    let mut b = Builder::new(match name {
        "root" => &[],
        "preparation" => &["success", "aborted"],
        "attempt" => &["success", "failure", "aborted", "prevention"],
        _ => &["done"],
    })?;
    if let Some(groups) = bindings
        .get("scene_owned_groups")
        .or_else(|| bindings.get("cast_groups"))
        .and_then(Value::as_object)
    {
        for (reference, members) in groups {
            b.scene_owned_groups.insert(
                reference.clone(),
                members
                    .as_array()
                    .context("cast group members")?
                    .iter()
                    .map(|v| v.as_str().map(str::to_owned).context("cast member"))
                    .collect::<Result<Vec<_>>>()?,
            );
        }
    }
    match name {
        "root" => root(&mut b, bindings, depot),
        "preparation" => prepare::compile(b, config, bindings, depot),
        "attempt" => attempt::compile(b, config, bindings, depot),
        _ => lifecycle::compile(b, config, bindings, depot),
    }
}

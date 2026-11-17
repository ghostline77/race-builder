//! MQ056 presentation authored as native scene nodes, with no scene donors.
use super::resources::{Handles, document, entity, hash, node64, tweak64};
use anyhow::{Context, Result};
use ghostline_quest_builder::nodes as n;
use serde_json::{Value, json};
fn id(kind: &str, value: u64) -> Value {
    json!({"$type":kind,"id":value})
}
fn time(value: u64) -> Value {
    json!({"$type":"scnSceneTime","stu":value})
}
fn socket(name: u64, ordinal: u64, dest: &[(u64, u64, u64)]) -> Value {
    json!({"$type":"scnOutputSocket","stamp":{"$type":"scnOutputSocketStamp","name":name,"ordinal":ordinal},"destinations":dest.iter().map(|(node,name,ordinal)|json!({"$type":"scnInputSocketId","nodeId":id("scnNodeId",*node),"isockStamp":{"$type":"scnInputSocketStamp","name":name,"ordinal":ordinal}})).collect::<Vec<_>>()})
}
struct Scene {
    h: Handles,
    graph: Vec<Value>,
    lines: Vec<Value>,
    options: Vec<Value>,
    entries: Vec<Value>,
    exits: Vec<Value>,
    payloads: Vec<Value>,
    descriptors: Vec<Value>,
    ns: String,
}
impl Scene {
    fn new(ns: &str) -> Self {
        Self {
            h: Handles::default(),
            graph: Vec::new(),
            lines: Vec::new(),
            options: Vec::new(),
            entries: Vec::new(),
            exits: Vec::new(),
            payloads: Vec::new(),
            descriptors: Vec::new(),
            ns: ns.to_owned(),
        }
    }
    fn node(&mut self, value: Value) {
        self.graph.push(self.h.wrap(value));
    }
    fn start(&mut self, number: u64, name: &str, target: u64, ordinal: u64) {
        self.entries.push(
            json!({"$type":"scnEntryPoint","name":n::cname(name),"nodeId":id("scnNodeId",number)}),
        );
        self.node(json!({"$type":"scnStartNode","ffStrategy":"automatic","nodeId":id("scnNodeId",number),"outputSockets":[socket(0,0,&[(target,0,ordinal)])]}));
    }
    fn end(&mut self, number: u64, name: &str, nonterminating: bool) {
        self.exits.push(
            json!({"$type":"scnExitPoint","name":n::cname(name),"nodeId":id("scnNodeId",number)}),
        );
        self.node(json!({"$type":"scnEndNode","nodeId":id("scnNodeId",number),"ffStrategy":"automatic","outputSockets":[],"type":if nonterminating{"NonTerminating"}else{"Terminating"}}));
    }
    fn quest(
        &mut self,
        number: u64,
        class: &str,
        inputs: &[&str],
        outputs: &[&str],
        props: Value,
        dest: &[(u64, u64, u64)],
    ) {
        let mut sockets=vec![self.h.wrap(json!({"$type":"questSocketDefinition","connections":[],"name":n::cname("CutDestination"),"type":"CutDestination"}))];
        for (kind, names) in [("Input", inputs), ("Output", outputs)] {
            for name in names {
                if *name == "CutDestination" {
                    continue;
                }
                sockets.push(self.h.wrap(json!({"$type":"questSocketDefinition","connections":[],"name":n::cname(name),"type":kind})));
            }
        }
        let mut quest = json!({"$type":class,"id":number,"sockets":sockets});
        quest
            .as_object_mut()
            .expect("static quest object")
            .extend(props.as_object().expect("static quest fields").clone());
        let quest = self.h.wrap(quest);
        self.node(json!({"$type":"scnQuestNode","ffStrategy":"automatic","isockMappings":inputs.iter().map(|s|n::cname(s)).collect::<Vec<_>>(),"osockMappings":outputs.iter().map(|s|n::cname(s)).collect::<Vec<_>>(),"nodeId":id("scnNodeId",number),"outputSockets":[socket(0,0,dest)],"questNode":quest}));
    }
    fn manager(&mut self, number: u64, class: &str, data: Value, dest: &[(u64, u64, u64)]) {
        let kind = self.h.wrap(data);
        self.quest(
            number,
            class,
            &["CutDestination", "In"],
            &["Out"],
            json!({"type":kind}),
            dest,
        );
    }
    fn fact_wait(&mut self, number: u64, fact: &str, destinations: &[(u64, u64, u64)]) {
        let kind=self.h.wrap(json!({"$type":"questVarComparison_ConditionType","factName":fact,"comparisonType":"Greater","value":0}));
        let condition = self
            .h
            .wrap(json!({"$type":"questFactsDBCondition","type":kind}));
        self.quest(
            number,
            "questPauseConditionNodeDefinition",
            &["CutDestination", "In"],
            &["Out"],
            json!({"condition":condition}),
            destinations,
        );
    }
    fn delay(&mut self, number: u64, ms: u64, dest: &[(u64, u64, u64)]) {
        let kind=self.h.wrap(json!({"$type":"questRealtimeDelay_ConditionType","hours":0,"minutes":0,"seconds":ms/1000,"miliseconds":ms%1000}));
        let condition = self
            .h
            .wrap(json!({"$type":"questTimeCondition","type":kind}));
        self.quest(
            number,
            "questPauseConditionNodeDefinition",
            &["CutDestination", "In"],
            &["Out"],
            json!({"condition":condition}),
            dest,
        );
    }
    fn fade(&mut self, number: u64, fade_in: bool, dest: &[(u64, u64, u64)]) {
        self.manager(number,"questRenderFxManagerNodeDefinition",json!({"$type":"questSetFadeInOut_NodeType","fadeIn":i32::from(fade_in),"duration":0.0,"fadeColor":{"$type":"Color","Alpha":0,"Blue":0,"Green":0,"Red":0}}),dest);
    }
    fn tier(&mut self, number: u64, tier: &str, dest: &[(u64, u64, u64)]) {
        self.manager(number,"questSceneManagerNodeDefinition",json!({"$type":"questSetTier_NodeType","tier":tier,"forceEmptyHands":tier!="Tier1_FullGameplay","motionConstrainedTierDataParams":{"$type":"gameMotionConstrainedTierDataParams","adjustingDuration":0,"adjustingSpeed":0,"notificationBackwardIndex":0,"splineRef":{"$type":"NodeRef","$storage":"uint64","$value":"0"},"travellingDuration":0,"travellingSpeed":0},"useEnterAnim":0,"useExitAnim":0,"usePlayerWorkspot":0}),dest);
    }
    fn event(&mut self, number: u64, props: Value, targets: &[(u64, u64, u64)]) {
        let event = self.h.wrap(props);
        self.quest(number,"questEventManagerNodeDefinition",&["CutDestination","In"],&["Out"],json!({"objectRef":entity("#mounted_vehicle",&[]),"managerName":"VehicleManager","PSClassName":n::cname("VehicleComponentPS"),"componentName":n::cname("controller"),"event":event}),targets);
    }
    fn line(
        &mut self,
        string: u64,
        speaker: u64,
        start: u64,
        duration: u64,
        section: u64,
        phone: bool,
    ) -> Value {
        let item = ((self.lines.len() as u64) << 8) | 1;
        self.lines.push(json!({"$type":"scnscreenplayDialogLine","itemId":id("scnscreenplayItemId",item),"speaker":id("scnActorId",speaker),"addressee":id("scnActorId",1-speaker),"locstringId":{"$type":"scnlocLocstringId","ruid":string.to_string()},"usage":{"$type":"scnscreenplayLineUsage","playerGenderMask":{"$type":"scnGenderMask","mask":3}},"maleLipsyncAnimationName":n::cname(&format!("m_{string:016X}")),"femaleLipsyncAnimationName":n::cname(&format!("f_{string:016X}"))}));
        self.h.wrap(json!({"$type":"scnDialogLineEvent","id":{"$type":"scnSceneEventId","id":hash(&format!("{}:{section}:{item}",self.ns)).to_string()},"startTime":start,"duration":duration,"screenplayLineId":id("scnscreenplayItemId",item),"voParams":{"$type":"scnDialogLineVoParams","voExpression":if phone{"Vo_Expression_Phone"}else{"Vo_Expression_Spoken"}}}))
    }
    fn section(
        &mut self,
        number: u64,
        duration: u64,
        events: Vec<Value>,
        sockets: Vec<Value>,
        actors: u64,
        block: bool,
    ) {
        self.node(json!({"$type":"scnSectionNode","nodeId":id("scnNodeId",number),"ffStrategy":if block{"block_fully"}else{"automatic"},"events":events,"sectionDuration":time(duration),"actorBehaviors":(0..actors).map(|i|json!({"$type":"scnSectionInternalsActorBehavior","actorId":id("scnActorId",i)})).collect::<Vec<_>>(),"outputSockets":sockets}));
    }
    fn loc(&mut self, string: u64, variant: u64, text: &str, locale: Option<&str>, signature: u64) {
        let index = self.payloads.len();
        self.payloads.push(json!({"$type":"scnlocLocStoreEmbeddedVariantPayloadEntry","variantId":{"$type":"scnlocVariantId","ruid":variant.to_string()},"content":text}));
        let mut descriptor = json!({"$type":"scnlocLocStoreEmbeddedVariantDescriptorEntry","variantId":{"$type":"scnlocVariantId","ruid":variant.to_string()},"locstringId":{"$type":"scnlocLocstringId","ruid":string.to_string()},"signature":{"$type":"scnlocSignature","val":signature.to_string()},"vpeIndex":index});
        if let Some(locale) = locale {
            descriptor["localeId"] = json!(locale);
        }
        self.descriptors.push(descriptor);
    }
    fn finish(mut self, depot: &str, b: &Value, result: bool, activation: bool) -> Result<Value> {
        let mut player = actor(true, if activation { 0 } else { 1 }, "Player");
        player["acquisitionPlan"] = json!("findInContext");
        player["specCharacterRecordId"] = n::tweak_id("Character.Player_Puppet_Base");
        player["voicetagId"] = json!({"$type":"scnVoicetagId","id":"1103967280742240864"});
        player["specAppearance"] = n::cname("default");
        player["specTemplate"] = n::cname("(None)");
        player["findActorInContextParams"] = json!({"$type":"scnFindEntityInContextParams","contextualName":"Player","contextActorName":n::cname("None"),"forceMaxVisibility":0,"voiceVagId":{"$type":"scnVoicetagId","id":"0"},"specRecordId":n::tweak_id("Character.Player_Puppet_Base")});
        if !activation {
            // Native race VO acquires V by record, without a named scene context
            // or an appearance/template override (the same as the original builder).
            player
                .as_object_mut()
                .expect("player actor")
                .remove("specAppearance");
            player
                .as_object_mut()
                .expect("player actor")
                .remove("specTemplate");
            player["findActorInContextParams"] = json!({"$type":"scnFindEntityInContextParams","specRecordId":n::tweak_id("Character.Player_Puppet_Base")});
        }
        let mut actors = Vec::new();
        let mut symbols = vec![
            json!({"$type":"scnPerformerSymbol","editorPerformerId":"0","entityRef":entity("#player",&[]),"performerId":id("scnPerformerId",if activation{1}else{257})}),
        ];
        if !activation {
            let mut npc = actor(false, 0, if result { "Johnny" } else { "announcer" });
            if result {
                npc["acquisitionPlan"] = json!("community");
                npc["communityParams"] = json!({"$type":"scnCommunityParams","entryName":n::cname("johnny"),"reference":n::node_ref(crate::route::text(b,"crowd")?),"forceMaxVisibility":0});
                if b.get("scene_owned_groups").is_some() {
                    npc["acquisitionPlan"] = json!("findInWorld");
                    npc.as_object_mut().context("Johnny actor")?.remove("communityParams");
                    npc["findActorInWorldParams"] = json!({"$type":"scnFindEntityInWorldParams","actorRef":entity(crate::route::text(b,"crowd")?,&["johnny"]),"forceMaxVisibility":false});
                }
                if b["actor_ownership"] == "population" {
                    npc["acquisitionPlan"] = json!("spawner");
                    npc.as_object_mut().context("Johnny actor")?.remove("communityParams");
                    let reference = super::actors::population_reference(crate::route::text(b,"crowd")?, "johnny").context("Johnny spawner")?;
                    npc["spawnerParams"] = json!({"$type":"scnSpawnerParams","reference":n::node_ref(&reference),"forceMaxVisibility":true});
                }
                npc["voicetagId"] = json!({"$type":"scnVoicetagId","id":"1103967280742240508"});
                symbols.push(json!({"$type":"scnPerformerSymbol","editorPerformerId":"0","entityRef":entity(crate::route::text(b,"crowd")?,&["johnny"]),"performerId":id("scnPerformerId",1)}));
            } else {
                npc["acquisitionPlan"] = json!("findInContext");
                npc["voicetagId"] = json!({"$type":"scnVoicetagId","id":"1796388644539244544"});
                npc["findActorInContextParams"] = json!({"$type":"scnFindEntityInContextParams","contextualName":"Voice","voiceVagId":{"$type":"scnVoicetagId","id":"1796388644539244544"},"specRecordId":tweak64("129901368005")});
                if b["actor_ownership"] == "population" {
                    let reference = super::actors::population_reference(crate::route::text(b,"crowd")?, "announcer").context("announcer spawner")?;
                    npc["acquisitionPlan"] = json!("spawner");
                    npc.as_object_mut().context("announcer actor")?.remove("findActorInContextParams");
                    npc["spawnerParams"] = json!({"$type":"scnSpawnerParams","reference":n::node_ref(&reference),"forceMaxVisibility":true});
                    symbols.push(json!({"$type":"scnPerformerSymbol","editorPerformerId":"4118870146959667200","entityRef":n::entity(Some(&reference),&[]),"performerId":id("scnPerformerId",1)}));
                } else {
                    symbols.push(json!({"$type":"scnPerformerSymbol","editorPerformerId":"4118870146959667200","performerId":id("scnPerformerId",1)}));
                }
            }
            actors.push(npc);
        }
        let graph=self.h.wrap(json!({"$type":"scnSceneGraph","startNodes":self.graph.iter().filter(|n|n["Data"]["$type"]=="scnStartNode").map(|n|n["Data"]["nodeId"].clone()).collect::<Vec<_>>(),"endNodes":self.graph.iter().filter(|n|n["Data"]["$type"]=="scnEndNode").map(|n|n["Data"]["nodeId"].clone()).collect::<Vec<_>>(),"graph":self.graph}));
        let mut refs = json!({"$type":"scnSRRefCollection","cinematicAnimNames":[],"cinematicAnimSets":[],"dynamicAnimNames":[],"dynamicAnimSets":[],"gameplayAnimNames":[],"gameplayAnimSets":[],"lipsyncAnimSets":[],"ridAnimationContainers":[],"ridAnimations":[],"ridAnimSets":[],"ridCameraAnimations":[],"ridCyberwareAnimSets":[],"ridDeformationAnimSets":[],"ridFacialAnimSets":[]});
        if activation {
            refs["lipsyncAnimSets"] = json!([{"$type":"scnLipsyncAnimSetSRRef","asyncRefLipsyncAnimSet":n::resource("base\\animations\\facial\\generic\\interactive_scene\\generic_facial_lipsync_gestures.anims")}]);
        } else {
            let folder = if result {
                "mq056_00_commentary"
            } else {
                "mq056_01_city_race"
            };
            refs["lipsyncAnimSets"]=json!([if result{"johnny"}else{"jim_jarry"},"v"].iter().map(|name|json!({"$type":"scnLipsyncAnimSetSRRef","asyncRefLipsyncAnimSet":n::resource(&format!("base\\quest\\minor_quests\\mq056\\scenes\\lipsync\\en\\{folder}\\{name}.anims"))})).collect::<Vec<_>>());
        }
        let root = json!({"$type":"scnSceneResource","version":5,"cookingPlatform":"PLATFORM_PC","sceneCategoryTag":"minorQuests","actors":actors,"playerActors":[player],"entryPoints":self.entries,"exitPoints":self.exits,"sceneGraph":graph,"screenplayStore":{"$type":"scnscreenplayStore","lines":self.lines,"options":self.options},"locStore":{"$type":"scnlocLocStoreEmbedded","vdEntries":self.descriptors,"vpEntries":self.payloads},"resouresReferences":refs,"debugSymbols":{"$type":"scnDebugSymbols","performersDebugSymbols":symbols,"sceneEventsDebugSymbols":[],"sceneNodesDebugSymbols":[],"workspotsDebugSymbols":[]},"workspots":[],"workspotInstances":[],"props":[],"localMarkers":[],"effectDefinitions":[],"effectInstances":[],"referencePoints":[],"notablePoints":[],"voInfo":[],"executionTags":[],"executionTagEntries":[],"interruptionScenarios":[],"ridResources":[]});
        ghostline_quest_builder::graph::order_handles(&document(root, depot))
    }
}
fn actor(player: bool, number: u64, name: &str) -> Value {
    let mut actor = json!({"$type":if player{"scnPlayerActorDef"}else{"scnActorDef"},"actorId":id("scnActorId",number),"lipsyncAnimSet":id("scnLipsyncAnimSetSRRefId",number),"animSets":[],"bodyCinematicAnimSets":[],"cyberwareAnimSets":[],"cyberwareCinematicAnimSets":[],"deformationAnimSets":[],"dynamicAnimSets":[],"facialAnimSets":[],"facialCinematicAnimSets":[]});
    actor[if player { "playerName" } else { "actorName" }] = json!(name);
    actor
}

impl Scene {
    fn choice(&mut self, number: u64, target: u64, item: u64, key: &str, world: bool, title: &str) {
        let shape = if world {
            self.h.wrap(json!({"$type":"scnInteractionShapeParams","preset":"small","offset":{"$type":"Vector3","X":0,"Y":0,"Z":0},"rotation":{"$type":"Quaternion","i":0,"j":0,"k":0,"r":1},"customIndicationRange":0,"customActivationRange":0,"activationYawLimit":360,"activationBaseLength":1,"activationHeight":3}))
        } else {
            Value::Null
        };
        // A standing interaction must not turn the camera toward its low root.
        let look = Value::Null;
        let mappin=self.h.wrap(json!({"$type":"scnChoiceNodeNsMappinParams","locationType":if world{"Interaction"}else{"None"},"mappinSettings":tweak64(if world{"183169082076"}else{"0"})}));
        let mut option = json!({"$type":"scnChoiceNodeOption","blueline":0,"bluelineCondition":null,"caption":n::cname(key),"emphasisCondition":null,"exDataFlags":0,"gameplayAction":tweak64("0"),"iconCondition":null,"iconTagIds":[],"isFixedAsRead":0,"isSingleChoice":0,"mappinReferencePointId":id("scnReferencePointId",u32::MAX as u64),"questCondition":null,"screenplayOptionId":id("scnscreenplayItemId",item),"timedCondition":null,"timedParams":null,"triggerCondition":null,"type":{"$type":"gameinteractionsChoiceTypeWrapper","properties":if world{257}else{1}}});
        if world {
            let script = self
                .h
                .wrap(json!({"$type":"PaymentFixedAmount_ScriptConditionType","payAmount":2000}));
            let fixed = self.h.wrap(
                json!({"$type":"questPaymentFixedAmount_ConditionType","scriptCondition":script}),
            );
            option["bluelineCondition"] = self
                .h
                .wrap(json!({"$type":"questPaymentCondition","type":fixed}));
            option["iconTagIds"] = json!([tweak64("119896558755")]);
            let mount=self.h.wrap(json!({"$type":"questCharacterMount_ConditionType","anyParent":1,"childIsPlayer":1,"childRef":entity("#player",&[]),"parentRef":n::entity(None,&[]),"condition":"OnUnmount"}));
            let unmounted = self
                .h
                .wrap(json!({"$type":"questCharacterCondition","type":mount}));
            let combat=self.h.wrap(json!({"$type":"questCharacterCombat_ConditionType","inverted":1,"isPlayer":1,"objectRef":entity("#player",&[])}));
            let peaceful = self
                .h
                .wrap(json!({"$type":"questCharacterCondition","type":combat}));
            option["questCondition"]=self.h.wrap(json!({"$type":"questLogicalCondition","operation":"AND","conditions":[unmounted,peaceful]}));
        }
        let mut outputs = vec![socket(0, 0, &[(target, 0, 0)])];
        outputs.extend((1..7).map(|i| socket(i, 0, &[])));
        if world {
            outputs[0]["destinations"].as_array_mut().expect("static socket").push(json!({"$type":"scnInputSocketId","isockStamp":{"$type":"scnInputSocketStamp","name":0,"ordinal":1},"nodeId":id("scnNodeId",199)}));
        }
        self.node(json!({"$type":"scnChoiceNode","nodeId":id("scnNodeId",number),"ffStrategy":"automatic","mode":if world{"attachToWorld"}else{"attachToScreen"},"interruptCapability":"Interruptable","displayNameOverride":if world{title}else{""},"localizedDisplayNameOverride":{"unk1":"0","value":""},"shapeParams":shape,"lookAtParams":look,"mappinParams":mappin,"ataParams":{"$type":"scnChoiceNodeNsAttachToActorParams","actorId":id("scnActorId",u32::MAX as u64),"visualizerStyle":"onScreen"},"atpParams":{"$type":"scnChoiceNodeNsAttachToPropParams","propId":id("scnPropId",u32::MAX as u64),"visualizerStyle":"inWorld"},"atgoParams":{"$type":"scnChoiceNodeNsAttachToGameObjectParams","nodeRef":node64("0"),"visualizerStyle":"onScreen"},"atsParams":{"$type":"scnChoiceNodeNsAttachToScreenParams"},"atwParams":{"$type":"scnChoiceNodeNsAttachToWorldParams","customEntityRadius":0,"entityPosition":{"$type":"Vector3","X":0,"Y":0,"Z":if world{0.82}else{0.}},"entityOrientation":{"$type":"Quaternion","i":0,"j":0,"k":0,"r":1},"visualizerStyle":"onScreen"},"customPersistentLine":id("scnscreenplayItemId",4294967040),"interruptionSpeakerOverride":id("scnActorId",u32::MAX as u64),"persistentLineEvents":[],"options":[option],"outputSockets":outputs}));
        let string = hash(&format!("{}:{key}", self.ns));
        self.options.push(json!({"$type":"scnscreenplayChoiceOption","itemId":id("scnscreenplayItemId",item),"locstringId":{"$type":"scnlocLocstringId","ruid":string.to_string()},"usage":{"$type":"scnscreenplayOptionUsage","playerGenderMask":{"$type":"scnGenderMask","mask":3}}}));
        for locale in ["db_db", "pl_pl", "en_us"] {
            self.loc(
                string,
                hash(&format!("{}:{key}:{locale}:text", self.ns)),
                if world {
                    "[Wait for the race]"
                } else {
                    "[Start the race]"
                },
                if locale == "db_db" {
                    None
                } else {
                    Some(locale)
                },
                3,
            );
        }
    }
    fn transition(
        &mut self,
        number: u64,
        target: u64,
        duration: u64,
        exit: u64,
        mut events: Vec<Value>,
        extra: &[(u64, u64, u64)],
        restoring: bool,
    ) {
        if restoring {
            events.push(json!({"$type":"scnGameplayTransitionEvent","startTime":2632,"duration":0,"performer":id("scnPerformerId",1),"vehState":"IdleStand"}));
        }
        if number != 101 {
            events.push(json!({"$type":"scneventsSocket","startTime":duration-1,"duration":0,"osockStamp":{"$type":"scnOutputSocketStamp","name":exit,"ordinal":0}}));
        }
        let mut wrapped = Vec::new();
        for (i, mut event) in events.into_iter().enumerate() {
            event["id"] = json!({"$type":"scnSceneEventId","id":hash(&format!("{}:{number}:{i}",self.ns)).to_string()});
            event["executionTagFlags"] = json!(0);
            event["scalingData"]=self.h.wrap(json!({"$type":"scnScalingData_KeepRelationWithOtherEvents","groupRfrncNdspaceEndtime":time(duration),"groupRfrncNdspaceStarttime":time(0)}));
            wrapped.push(self.h.wrap(event));
        }
        let max = extra
            .iter()
            .map(|(port, _, _)| *port)
            .max()
            .unwrap_or(0)
            .max(exit);
        let sockets = (0..=max)
            .map(|port| {
                let mut targets = Vec::new();
                if port == exit {
                    targets.push((target, 0, 1));
                }
                for (p, dest, ordinal) in extra {
                    if *p == port {
                        targets.push((*dest, 0, *ordinal));
                    }
                }
                socket(port, 0, &targets)
            })
            .collect();
        self.section(number, duration, wrapped, sockets, 1, false);
    }
    fn glitch(&mut self, number: u64, target: u64, ordinal: u64) {
        self.manager(number,"questFXManagerNodeDefinition",json!({"$type":"questPlayFX_NodeType","params":[{"$type":"questPlayFX_NodeTypeParams","objectRef":entity("#player",&[]),"isPlayer":1,"effectName":n::cname("fast_travel_glitch")}]}),&[(target,0,ordinal)]);
    }
}

pub fn activation(config: &Value, b: &Value, depot: &str) -> Result<Value> {
    let ns = crate::route::text(config, "id")?;
    let mut s = Scene::new(ns);
    s.start(55, &format!("{ns}_waiting_in"), 119, 0);
    s.choice(
        119,
        121,
        2,
        "wait",
        true,
        crate::route::text(config, "title")?,
    );
    s.transition(121,125,2600,3,vec![json!({"$type":"scneventsSocket","startTime":2001,"duration":0,"osockStamp":{"$type":"scnOutputSocketStamp","name":2,"ordinal":0}})],&[(2,164,1)],false);
    s.manager(199,"questUIManagerNodeDefinition",json!({"$type":"questProgressBar_NodeType","duration":2.0,"text":{"unk1":"0","value":"LocKey#83354"},"type":"Money"}),&[]);
    s.tier(125, "Tier2_StagedGameplay", &[]);
    s.glitch(164, 129, 1);
    s.delay(129, 600, &[(127, 0, 1)]);
    s.fade(127, false, &[(97, 0, 0), (101, 0, 0)]);
    s.end(97, "fade_out", true);
    s.transition(101,99,1750,0,vec![json!({"$type":"scneventsSocket","startTime":510,"duration":0,"osockStamp":{"$type":"scnOutputSocketStamp","name":2,"ordinal":0}})],&[(2,111,1)],false);
    s.tier(111, "Tier2_StagedGameplay", &[]);
    s.fact_wait(99, &format!("{ns}_crowd_spawned"), &[(140, 0, 1)]);
    s.delay(140, 2000, &[(146, 0, 1)]);
    let target=s.h.wrap(json!({"$type":"questUniversalRef","entityReference":n::entity(None,&[]),"mainPlayerObject":0,"refLocalPlayer":1}));
    let destination=s.h.wrap(json!({"$type":"questUniversalRef","entityReference":entity(crate::route::text(b,"activation")?,&[]),"mainPlayerObject":0,"refLocalPlayer":0}));
    let params=s.h.wrap(json!({"$type":"questTeleportPuppetParamsV1","destinationOffset":{"$type":"Vector3","X":0,"Y":0,"Z":0},"destinationRef":destination,"doNavTest":0,"healAtTeleport":0,"useFastTravelMechanism":0}));
    let look=s.h.wrap(json!({"$type":"questPlayerLookAtParams","adjustPitch":1,"adjustYaw":1,"cameraInputMagToBreak":0.2,"duration":0.25,"easeIn":1,"easeOut":1,"endOnCameraInputApplied":1,"endOnTargetReached":1,"endOnTimeExceeded":1,"lookAtTarget":entity(crate::route::text(b,"activation_view")?,&[]),"maxDuration":0.3,"offset":{"$type":"Vector3","X":0,"Y":0,"Z":0},"precision":0.1,"slotName":n::cname("None"),"useOffsetToPlayer":0}));
    s.quest(
        146,
        "questTeleportPuppetNodeDefinition",
        &["CutDestination", "In"],
        &["Out"],
        json!({"entityReference":target,"lookAtAction":"Set","params":params,"playerLookAt":look}),
        &[(147, 0, 1), (142, 0, 1)],
    );
    s.delay(147, 300, &[(138, 0, 1)]);
    s.delay(142, 2000, &[(144, 0, 1)]);
    s.manager(144,"questUIManagerNodeDefinition",json!({"$type":"questShowOnscreen_NodeType","localizedMessage":{"unk1":"0","value":"LocKey#77682"}}),&[]);
    s.fade(138, true, &[(168, 0, 1)]);
    s.glitch(168, 151, 0);
    s.choice(151, 154, 258, "start", false, "");
    s.transition(154,172,3133,3,vec![json!({"$type":"scneventsSocket","startTime":3132,"duration":0,"osockStamp":{"$type":"scnOutputSocketStamp","name":4,"ordinal":0}})],&[(4,158,1)],true);
    s.tier(158, "Tier1_FullGameplay", &[]);
    s.manager(172,"questFactsDBManagerNodeDefinition",json!({"$type":"questSetVar_NodeType","factName":format!("{ns}_standup"),"value":1,"setExactValue":1}),&[(161,0,0)]);
    s.end(161, &format!("{ns}_waiting_out"), false);
    // LocaleId uses engine order (fallback, Polish, English), not alphabetical order.
    s.descriptors.sort_by(|a, b| {
        let locale_order = |v: &Value| match v["localeId"].as_str() {
            None | Some("db_db") => 0,
            Some("pl_pl") => 1,
            Some("en_us") => 2,
            _ => 3,
        };
        locale_order(a).cmp(&locale_order(b)).then_with(|| {
            a["locstringId"]["ruid"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
                .cmp(
                    &b["locstringId"]["ruid"]
                        .as_str()
                        .and_then(|s| s.parse::<u64>().ok()),
                )
        })
    });
    s.finish(depot, b, false, true)
}

pub fn compile(config: &Value, b: &Value, depot: &str, result: bool) -> Result<Value> {
    let ns = crate::route::text(config, "id")?;
    let mut s = Scene::new(ns);
    if result {
        let groups: [(&str, u64, u64, &[(u64, u64)]); 9] = [
            (
                "lose",
                0,
                5000,
                &[
                    (4159369413620789248, 1390),
                    (4159369428082749440, 1750),
                    (4159369438400737280, 2545),
                    (4159369447779201024, 1825),
                    (4159369458030080000, 2305),
                ],
            ),
            (
                "win",
                100,
                5000,
                &[
                    (4159372547403411456, 1410),
                    (4159372566193893376, 1805),
                    (4159372575001931776, 1485),
                    (4159372586695651328, 1265),
                    (4159372680513843200, 2995),
                ],
            ),
            (
                "johnny_win",
                200,
                5000,
                &[(4159362752931819520, 2540), (4159362765078523904, 1410)],
            ),
            (
                "johnny_lose",
                300,
                5000,
                &[(4159363896584302592, 1390), (4159363909402095616, 2575)],
            ),
            ("you_ready", 400, 5000, &[(4159347102540754944, 2785)]),
            ("car_hp_drop", 500, 5000, &[(4159349620666658816, 1825)]),
            ("sandstorm", 600, 5000, &[(4159351819706691584, 2200)]),
            ("speeding", 700, 6170, &[(4159353074558562304, 5895)]),
            (
                "destroying_enemy",
                800,
                5000,
                &[
                    (4159361710546612224, 1770),
                    (4159361737893474304, 1620),
                    (4159361750291836928, 1685),
                ],
            ),
        ];
        for (name, offset, duration, lines) in groups {
            s.start(
                offset + 1,
                &format!("{name}_in"),
                if lines.len() > 1 {
                    offset + 3
                } else {
                    offset + 10
                },
                0,
            );
            s.end(offset + 2, &format!("{name}_out"), false);
            if lines.len() > 1 {
                // Native randomizers default to zero weights, which selects no
                // output. Populate every active slot and retain the 32-slot layout.
                let weights = (0..32)
                    .map(|i| u8::from(i < lines.len()))
                    .collect::<Vec<_>>();
                s.node(json!({"$type":"scnRandomizerNode","nodeId":id("scnNodeId",offset+3),"ffStrategy":"automatic","numOutSockets":lines.len(),"weights":{"Elements":weights},"outputSockets":(0..lines.len()).map(|i|socket(0,i as u64,&[(offset+10+i as u64,0,0)])).collect::<Vec<_>>()}));
            }
            for (i, (string, line_duration)) in lines.iter().enumerate() {
                let section = offset + 10 + i as u64;
                let line = s.line(
                    *string,
                    if offset < 200 { 1 } else { 0 },
                    0,
                    *line_duration,
                    section,
                    false,
                );
                s.section(
                    section,
                    duration,
                    vec![line],
                    vec![socket(0, 0, &[(offset + 2, 0, 0)]), socket(1, 0, &[])],
                    2,
                    false,
                );
            }
        }
        result_localization(&mut s);
    } else {
        s.start(1, "announcer_in", 12, 0);
        s.end(26, "announcer_out", false);
        s.end(10, "countdown_go", true);
        s.end(16, "countdown_started", true);
        let events = [
            (2008388904462454784, 0, 4195),
            (1831399069043400704, 4246, 3783),
            (1855979224088842240, 8029, 5389),
        ]
        .iter()
        .map(|(id, start, duration)| s.line(*id, 0, *start, *duration, 12, true))
        .collect();
        s.section(
            12,
            13418,
            events,
            vec![socket(0, 0, &[(195, 0, 1)]), socket(1, 0, &[])],
            2,
            false,
        );
        // Boarding can finish long after the announcer. Read the saved readiness
        // facts in this scene entry so the two events can arrive in either order.
        s.fact_wait(195, &format!("{ns}_countdown_requested"), &[(189, 0, 1)]);
        s.fact_wait(
            189,
            &format!("{ns}_opponents_mounted"),
            &[(14, 0, 0), (16, 0, 0), (28, 0, 1), (38, 0, 1)],
        );
        s.manager(28,"questAudioNodeDefinition",json!({"$type":"questAudioMixNodeType","mixSignpost":n::cname("sq024_city_race_countdown_start")}),&[]);
        let mut events=vec![s.h.wrap(json!({"$type":"scneventsSocket","startTime":0,"duration":0,"id":{"$type":"scnSceneEventId","id":hash(&format!("{ns}:14:2")).to_string()},"osockStamp":{"$type":"scnOutputSocketStamp","name":2,"ordinal":0}}))];
        events.extend(
            [
                (1856145996930748416, 0, 1539),
                (1856146462783705088, 1539, 1405),
                (1856146646410334208, 2944, 1478),
            ]
            .iter()
            .map(|(id, start, duration)| s.line(*id, 0, *start, *duration, 14, true)),
        );
        events.push(s.h.wrap(json!({"$type":"scneventsSocket","startTime":4130,"duration":0,"id":{"$type":"scnSceneEventId","id":hash(&format!("{ns}:14:3")).to_string()},"osockStamp":{"$type":"scnOutputSocketStamp","name":2,"ordinal":1}})));
        let mut sockets = vec![
            socket(0, 0, &[(26, 0, 0)]),
            socket(1, 0, &[]),
            socket(2, 0, &[(32, 0, 1)]),
            socket(2, 1, &[(20, 0, 3)]),
        ];
        if config["vehicle_diagnostics"] == true {
            for (i, (at, label)) in [
                (0, "countdown_three"),
                (1538, "countdown_before_two"),
                (1540, "countdown_after_two"),
                (2943, "countdown_before_one"),
                (2945, "countdown_after_one"),
                (4130, "countdown_go"),
                (4421, "countdown_section_end"),
            ]
            .iter()
            .enumerate()
            {
                let ordinal = i as u64 + 2;
                events.push(s.h.wrap(json!({"$type":"scneventsSocket","startTime":at,"duration":0,"id":{"$type":"scnSceneEventId","id":hash(&format!("{ns}:14:{label}")).to_string()},"osockStamp":{"$type":"scnOutputSocketStamp","name":2,"ordinal":ordinal}})));
                sockets.push(socket(2, ordinal, &[(200 + ordinal, 0, 1)]));
                let event=s.h.wrap(json!({"$type":"AITaggedAIEvent","name":n::cname("ghostline_race_vehicle_trace"),"tags":[n::cname(ns),n::cname(label),n::cname("#player")]}));
                s.quest(200+ordinal,"questEventManagerNodeDefinition",&["CutDestination","In"],&["Out"],json!({"isObjectPlayer":true,"objectRef":entity("#player",&[]),"managerName":"None","event":event}),&[]);
            }
            events.sort_by_key(|e| e["Data"]["startTime"].as_u64().unwrap_or(0));
        }
        s.section(14, 4422, events, sockets, 2, true);
        let checkpoints = b["checkpoints"].as_array().context("checkpoints")?.len() + 1;
        let entrants = b["entrants"].as_array().context("entrants")?.len() + 1;
        for (number, mode, next) in [
            (32, "CountdownStart", vec![(34, 0, 1)]),
            (38, "PreRaceSetup", vec![]),
            (30, "RaceStart", vec![]),
        ] {
            s.event(number,json!({"$type":"VehicleRaceQuestEvent","mode":mode,"maxCheckpoints":checkpoints,"maxPosition":entrants}),&next);
        }
        for (number, names, next) in [
            (34, vec!["checkpoints", "checkpoints2"], vec![(36, 0, 1)]),
            (36, vec!["gps_path"], vec![]),
        ] {
            s.manager(number,"questWorldDataManagerNodeDefinition",json!({"$type":"questTogglePrefabVariant_NodeType","params":[{"$type":"questTogglePrefabVariant_NodeTypeParams","prefabNodeRef":n::node_ref(crate::route::text(b,"prefab")?),"variantStates":names.iter().map(|name|json!({"$type":"questVariantState","name":n::cname(name),"show":true})).collect::<Vec<_>>()}]}),&next);
        }
        for (number, key, next) in [(20, "countdown_objective", 22), (22, "objective", 24)] {
            let path=s.h.wrap(json!({"$type":"gameJournalPath","realPath":b[key],"fileEntryIndex":2,"className":n::cname("gameJournalQuestObjective")}));
            let kind=s.h.wrap(json!({"$type":"questJournalQuestEntry_NodeType","path":path,"version":"OptionalProperty"}));
            s.quest(
                number,
                "questJournalNodeDefinition",
                &[
                    "CutDestination",
                    "Active",
                    "Inactive",
                    "Succeeded",
                    "Failed",
                ],
                &["Out"],
                json!({"type":kind}),
                &[(next, 0, 1)],
            );
        }
        s.manager(24,"questFactsDBManagerNodeDefinition",json!({"$type":"questSetVar_NodeType","factName":format!("{ns}_countdown_end"),"value":1,"setExactValue":true}),&[(10,0,0),(30,0,1)]);
        countdown_localization(&mut s);
    }
    s.finish(depot, b, result, false)
}

fn result_localization(s: &mut Scene) {
    let lines: [(u64, &str, u64, &str); 21] = [
        (
            4159347102540754944,
            "So – you ready?",
            4169929575761231872,
            "To jak? Gotowa?",
        ),
        (
            4159349620666658816,
            "Fuck!",
            4169929575761231876,
            "Ja pierdolę!",
        ),
        (
            4159351819706691584,
            "Shit, get ready.",
            4169929575778009088,
            "Przygotuj się.",
        ),
        (
            4159353074558562304,
            "Slow down, turbo. Or you’ll get us killed. Again",
            4169929575778009092,
            "Co ty wyprawiasz?! Chcesz nas znowu pozabijać?",
        ),
        (
            4159361710546612224,
            "Not half bad, that.",
            4169929575778009096,
            "To było dobre.",
        ),
        (
            4159361737893474304,
            "Guess it's your lucky day.",
            4169929575778009100,
            "No wreszcie!",
        ),
        (
            4159361750291836928,
            "Fuck yeah, nice!",
            4169929575778009104,
            "Dobra robota.",
        ),
        (
            4159362752931819520,
            "Ugh, finally.",
            4169929575794786304,
            "To twój szczęśliwy dzień.",
        ),
        (
            4159362765078523904,
            "Finally.",
            4169929575794786308,
            "Nieźle.",
        ),
        (
            4159363896584302592,
            "Ohhh, V…",
            4169929575794786312,
            "Kurwa.",
        ),
        (
            4159363909402095616,
            "Oh, V…",
            4169929575794786316,
            "Oj, V, V...",
        ),
        (4159369413620789248, "Fuck.", 4169929575794786320, "Kurwa."),
        (
            4159369428082749440,
            "Great. Just great.",
            4169929575794786324,
            "No to pięknie.",
        ),
        (
            4159369438400737280,
            "Fan-fucking-tastic.",
            4169929575811563520,
            "Wspaniale, kurwa.",
        ),
        (
            4159369447779201024,
            "Fucking hell...",
            4169929575811563524,
            "Ja pierdolę...",
        ),
        (
            4159369458030080000,
            "Goddamnit all...",
            4169929575811563528,
            "Szlag by to wszystko.",
        ),
        (
            4159372547403411456,
            "Fuuuuuu-uuu--uuuuck! Aaaaaaaaaaugh!",
            4169929575811563532,
            "Nieźle!",
        ),
        (
            4159372566193893376,
            "Whoooooooo... Aaaaaaah!",
            4169929575811563536,
            "Udało się.",
        ),
        (
            4159372575001931776,
            "Well, well, well.",
            4169929575828340736,
            "Mamy to!",
        ),
        (
            4159372586695651328,
            "Gonna be a good day.",
            4169929575828340740,
            "Tak!",
        ),
        (
            4159372680513843200,
            "Can't have bad luck all the time.",
            4169929575828340744,
            "Czasem nawet ja mam szczęście.",
        ),
    ];
    for (string, english, _, _) in lines {
        s.loc(string, string + 4, english, None, 3);
    }
    for (string, _, polish_id, polish) in lines {
        s.loc(
            string,
            polish_id,
            polish,
            Some("pl_pl"),
            if string == 4159347102540754944 { 2 } else { 3 },
        );
        if string == 4159347102540754944 {
            s.loc(
                string,
                4169929837569687552,
                "To jak? Gotowy?",
                Some("pl_pl"),
                1,
            );
        }
    }
}
fn countdown_localization(s: &mut Scene) {
    let rows: [(u64, [(u64, &str); 3]); 6] = [
        (
            1831399069043400704,
            [
                (
                    2241921996463149096,
                    "If someone wants to withdraw, it's too late.",
                ),
                (
                    1855808770493517876,
                    "Jeśli ktoś chce się wycofać, to już za późno.",
                ),
                (
                    4118865403789045792,
                    "Not like it matters – too late to back out anyway!",
                ),
            ],
        ),
        (
            1855979224088842240,
            [
                (
                    2241921996463149100,
                    "It's time to fasten your seat belts, clench your teeth and get ready to kick.",
                ),
                (
                    1855979224088842248,
                    "Czas zapiąć pasy, zacisnąć zęby i przygotować się na kopa.",
                ),
                (
                    4118865403789045796,
                    "Strap in, pray to your gods and get ready to go for pay dirt!",
                ),
            ],
        ),
        (
            1856145996930748416,
            [
                (2241921996496703504, "Start in three ..."),
                (1856145996930748424, "Start za trzy..."),
                (4118865403805822992, "Three!"),
            ],
        ),
        (
            1856146462783705088,
            [
                (2241921996496703508, "Two..."),
                (1856146462783705096, "Dwa..."),
                (4118865403805822996, "Two!"),
            ],
        ),
        (
            1856146646410334208,
            [
                (2241921996496703512, "One!"),
                (1856146646410334216, "Jeden!"),
                (4118865403805823000, "One!"),
            ],
        ),
        (
            2008388904462454784,
            [
                (
                    2241921996463149088,
                    "We accept the last bets! Don't be afraid to bet!",
                ),
                (
                    2008389536946720768,
                    "Przyjmujemy ostatnie zakłady! Nie bać się, obstawiać!",
                ),
                (
                    4118865403789045784,
                    "Last bets, last bets! Don't miss your chance to win!",
                ),
            ],
        ),
    ];
    for (i, locale) in ["db_db", "pl_pl", "en_us"].iter().enumerate() {
        for (string, variants) in rows {
            s.loc(string, variants[i].0, variants[i].1, Some(locale), 3);
        }
    }
}

use anyhow::{Context, Result};
use ghostline_quest_builder::nodes as n;
use serde_json::{Value, json};
pub const PACKAGE: &str = "WolvenKit.RED4.Archive.Buffer.RedPackage, WolvenKit.RED4, Version=8.17.4.0, Culture=neutral, PublicKeyToken=null";
#[derive(Default)]
pub struct Handles {
    next: u64,
}
impl Handles {
    pub fn wrap(&mut self, data: Value) -> Value {
        self.next += 1;
        json!({"HandleId":self.next.to_string(),"Data":data})
    }
}
pub fn document(root: Value, depot: &str) -> Value {
    json!({"Header":{"WolvenKitVersion":"8.17.4","WKitJsonVersion":"0.0.9","GameVersion":2310,"ExportedDateTime":"1970-01-01T00:00:00Z","DataType":"CR2W","ArchiveFileName":depot},"Data":{"Version":195,"BuildVersion":0,"RootChunk":root,"EmbeddedFiles":[]}})
}
pub fn hash(value: &str) -> u64 {
    value.as_bytes().iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
pub fn transform(z: f64, backwards: bool) -> Value {
    json!({"$type":"WorldTransform","Orientation":{"$type":"Quaternion","i":0,"j":0,"k":i32::from(backwards),"r":i32::from(!backwards)},"Position":{"$type":"WorldPosition","x":{"$type":"FixedPoint","Bits":0},"y":{"$type":"FixedPoint","Bits":0},"z":{"$type":"FixedPoint","Bits":(z*131072.).round()as i32}}})
}
pub fn tweak64(value: &str) -> Value {
    json!({"$type":"TweakDBID","$storage":"uint64","$value":value})
}
pub fn node64(value: &str) -> Value {
    json!({"$type":"NodeRef","$storage":"uint64","$value":value})
}
pub fn entity(reference: &str, names: &[&str]) -> Value {
    if names.len() == 1 {
        if let Some(reference) = super::actors::population_reference(reference, names[0]) {
            return n::entity(Some(&reference), &[]);
        }
        if let Some(name) = super::actors::dynamic(reference,names[0]) {
            let mut entity = n::entity(None,&[]);
            entity["dynamicEntityUniqueName"] = n::cname(&name);
            return entity;
        }
    }
    n::entity(Some(reference), names)
}

pub fn checkpoint(config: &Value, depot: &str, direction: &str) -> Result<Value> {
    let ns = crate::route::text(config, "id")?;
    let mut h = Handles::default();
    let mut chunks = vec![
        json!({"$type":"GenericDevice","controllerTypeName":n::cname("GenericDeviceController"),"durabilityType":"INVULNERABLE","deviceState":"OFF","isUIdirty":1,"visibilityCheckDistance":16000}),
    ];
    fn component(chunks: &mut Vec<Value>, ns: &str, kind: &str, name: &str, mut fields: Value) {
        let object = fields
            .as_object_mut()
            .expect("component fields are static objects");
        object.insert("$type".into(), json!(kind));
        object.insert("name".into(), n::cname(name));
        object.insert(
            "id".into(),
            json!(hash(&format!("{ns}:component:{name}")).to_string()),
        );
        object.insert("isReplicable".into(), json!(0));
        chunks.push(fields);
    }
    component(
        &mut chunks,
        ns,
        "entSlotComponent",
        "main_slot",
        json!({"localTransform":transform(0.,false),"slots":[],"fallbackSlots":[]}),
    );
    component(
        &mut chunks,
        ns,
        "workWorkspotResourceComponent",
        "personalLinkPlayerWorkspot",
        json!({"localTransform":transform(0.,false)}),
    );
    let visible=h.wrap(json!({"$type":"senseVisibleObject","visibilityDistance":100,"visibleObjectType":"Undefined"}));
    component(
        &mut chunks,
        ns,
        "senseVisibleObjectComponent",
        "senseVisibleObject",
        json!({"localTransform":transform(0.,false),"visibleObject":visible}),
    );
    let stats = h.wrap(json!({"$type":"gameStatsComponentPS"}));
    component(
        &mut chunks,
        ns,
        "gameStatsComponent",
        "StatsComponent",
        json!({"persistentState":stats}),
    );
    component(
        &mut chunks,
        ns,
        "gameaudioSoundComponent",
        "audio",
        json!({"localTransform":transform(0.,false),"maxPlayDistance":40,"obstructionChangeTime":0.2,"audioName":n::cname("None"),"voEventOverride":n::cname("None"),"subSystems":[]}),
    );
    let mut operations = Vec::new();
    for (kind, name, fields) in [
        (
            "FactsDeviceOperation",
            "addFact",
            json!({"facts":[{"$type":"SFactOperationData","factName":n::cname("sq024_current_race_checkpoint_fact_add"),"factValue":1,"operationType":"Add"}]}),
        ),
        (
            "ToggleComponentsDeviceOperation",
            "disableComponents",
            json!({"components":(["ui_back","ui_front","trigger"].iter().map(|name|json!({"$type":"SComponentOperationData","componentName":n::cname(name),"operationType":"Disable"})).collect::<Vec<_>>())}),
        ),
        (
            "PlaySoundDeviceOperation",
            "playSound",
            json!({"SFXs":[{"$type":"SSFXOperationData","operationType":"START","sfxName":n::cname("sq024_race_countdown")}]}),
        ),
    ] {
        let mut data = json!({"$type":kind,"operationName":n::cname(name),"disableDevice":i32::from(name=="disableComponents"),"executeOnce":0,"isEnabled":1,"toggleOperations":[]});
        data.as_object_mut()
            .context("operation")?
            .extend(fields.as_object().context("operation fields")?.clone());
        operations.push(h.wrap(data));
    }
    let executions=[("addFact",0.),("disableComponents",0.1),("playSound",0.)].iter().map(|(name,delay)|h.wrap(json!({"$type":"OperationExecutionData","delay":delay,"delayID":{"$type":"gameDelayID"},"isDelayActive":0,"operationName":n::cname(name),"resetDelay":0}))).collect::<Vec<_>>();
    let trigger_data=h.wrap(json!({"$type":"TriggerVolumeOperationTriggerData","canNPCBeDead":1,"componentName":n::cname("trigger"),"isActivatorNPC":0,"isActivatorPlayer":1,"operationsToExecute":executions,"operationType":"ENTER"}));
    let trigger =
        h.wrap(json!({"$type":"TriggerVolumeOperationsTrigger","triggerData":trigger_data}));
    let setup=h.wrap(json!({"$type":"DeviceOperationsContainer","operations":operations,"triggers":if direction=="Finish"{vec![]}else{vec![trigger]}}));
    let state=h.wrap(json!({"$type":"GenericDeviceControllerPS","deviceState":"ON","deviceUIStyle":"LightBlue","isInteractive":1,"hasAuthorizationModule":1,"tweakDBRecord":n::tweak_id("Devices.GenericDevice"),"deviceOperationsSetup":setup}));
    component(
        &mut chunks,
        ns,
        "GenericDeviceController",
        "controller",
        json!({"persistentState":state}),
    );
    component(
        &mut chunks,
        ns,
        "DisassemblableComponent",
        "disassemblableComponent",
        json!({}),
    );
    component(
        &mut chunks,
        ns,
        "gameinteractionsComponent",
        "interaction",
        json!({"localTransform":transform(0.,false),"isEnabled":1,"interactionRootOffset":{"$type":"Vector3","X":0,"Y":0,"Z":0},"layerOverrides":[],"layerOverridesTemp":[]}),
    );
    component(
        &mut chunks,
        ns,
        "entLocalizationStringComponent",
        "localization",
        json!({"Strings":[]}),
    );
    component(
        &mut chunks,
        ns,
        "gameTargetingComponent",
        "targeting",
        json!({"localTransform":transform(0.,false),"isEnabled":1,"isPrimary":1,"aimAssistData":[]}),
    );
    let vision = h.wrap(json!({"$type":"gameVisionModeComponentPS"}));
    component(
        &mut chunks,
        ns,
        "gameVisionModeComponent",
        "vision",
        json!({"persistentState":vision}),
    );
    component(
        &mut chunks,
        ns,
        "WorkspotMapperComponent",
        "workspotMapper",
        json!({"workspotsMap":[]}),
    );
    component(
        &mut chunks,
        ns,
        "FxResourceMapperComponent",
        "FxResourceMapper",
        json!({"investigationSlotOffsetMultiplier":1,"areaEffectData":[],"areaEffectInFocusMode":[],"optionalAreaEffectData":[]}),
    );
    component(
        &mut chunks,
        ns,
        "GameplayRoleComponent",
        "GameplayRole",
        json!({"alwaysCreateMappinAsDynamic":1,"autoDeterminGameplayRole":1,"currentGameplayRole":"UnAssigned","gameplayRole":"UnAssigned","mappinsDisplayMode":"MINIMALISTIC","offsetValue":0.04}),
    );
    component(
        &mut chunks,
        ns,
        "entSlotComponent",
        "IKslots",
        json!({"localTransform":transform(0.,false),"slots":[],"fallbackSlots":[]}),
    );
    component(
        &mut chunks,
        ns,
        "StimBroadcasterComponent",
        "StimBroadcaster",
        json!({"fallbackInterval":1}),
    );
    let outline=h.wrap(json!({"$type":"AreaShapeOutline","buffer":"","height":30,"points":([[-25,-20],[25,-20],[25,60],[-25,60]].iter().map(|p|json!({"$type":"Vector3","X":p[0],"Y":p[1],"Z":0})).collect::<Vec<_>>())}));
    component(
        &mut chunks,
        ns,
        "gameStaticTriggerAreaComponent",
        "trigger",
        json!({"isEnabled":i32::from(direction!="Finish"),"includeMask":1,"localTransform":transform(-582373./131072.,false),"outline":outline}),
    );
    let mesh = "base\\environment\\decoration\\advertising\\holograms\\common\\common_holograms_transparent_a.mesh";
    for side in ["front", "back"] {
        let name = format!("screen_{side}");
        component(
            &mut chunks,
            ns,
            "entMeshComponent",
            &name,
            json!({"isEnabled":1,"localTransform":transform(368312./131072.,side=="front"),"mesh":n::resource(mesh),"meshAppearance":n::cname("screen_ui"),"visualScale":{"$type":"Vector3","X":10,"Y":1,"Z":5.63000011},"LODMode":"AlwaysVisible","chunkMask":"9223372036854775807","forceLODLevel":-1,"objectTypeID":"ROT_Static","renderingPlane":"RPl_Scene","renderSceneLayerMask":"Default","motionBlurScale":1,"castShadows":"Always","castLocalShadows":"Always","castRayTracedGlobalShadows":"Always","overrideMeshNavigationImpact":1,"navigationImpact":{"$type":"NavGenNavigationSetting","navmeshImpact":"Ignored"}}),
        );
        let binding=h.wrap(json!({"$type":"worlduiMeshTargetBinding","bindName":n::cname(&name),"enabled":1,"enableMask":{"$type":"entTagMask","excludedTags":{"$type":"redTagList","tags":[n::cname("NoBinding")]},"hardTags":{"$type":"redTagList","tags":[]},"softTags":{"$type":"redTagList","tags":[]}}}));
        let record = if direction == "Finish" {
            "LCDScreens.RaceFinish".to_owned()
        } else {
            format!("LCDScreens.RaceCheckpoint{direction}")
        };
        let selector=h.wrap(json!({"$type":"RaceCheckpointSelector","baseTweakID":tweak64("0"),"recordID":n::tweak_id(&record)}));
        component(
            &mut chunks,
            ns,
            "StreetSignWidgetComponent",
            &format!("ui_{side}"),
            json!({"isEnabled":1,"localTransform":transform(0.,false),"isAStreetName":1,"screenAreaMultiplier":1,"meshTargetBinding":binding,"signSelector":selector,"streetSignTDBID":n::tweak_id(&record)}),
        );
    }
    let cruids = chunks
        .iter()
        .enumerate()
        .map(|(i, c)| (i.to_string(), c.get("id").cloned().unwrap_or(json!("0"))))
        .collect::<serde_json::Map<_, _>>();
    Ok(document(
        json!({"$type":"entEntityTemplate","cookingPlatform":"PLATFORM_PC","defaultAppearance":n::cname("default"),"compiledData":{"BufferId":"0","Flags":4063232,"Type":PACKAGE,"Data":{"Version":4,"Sections":7,"CruidIndex":0,"CruidDict":cruids,"Chunks":chunks}},"appearances":[],"backendDataOverrides":[],"bindingOverrides":[],"compiledEntityLODFlags":0,"componentResolveSettings":[],"includes":[],"inplaceResources":[],"resolvedDependencies":[n::resource(mesh)]}),
        depot,
    ))
}

fn entry(h: &mut Handles, kind: &str, id: &str, children: Vec<Value>, fields: Value) -> Value {
    let mut value =
        json!({"$type":kind,"id":id,"entries":children,"journalEntryOverrideDataList":[]});
    if let Some(fields) = fields.as_object() {
        value
            .as_object_mut()
            .expect("static journal object")
            .extend(fields.clone());
    }
    h.wrap(value)
}
fn loc(s: &str) -> Value {
    ghostline_quest_builder::localization::reference(s)
}
pub fn journal(config: &Value, b: &Value, depot: &str) -> Result<Value> {
    let ns = crate::route::text(config, "id")?;
    let mut h = Handles::default();
    let mut description = entry(
        &mut h,
        "gameJournalQuestDescription",
        "description",
        vec![],
        json!({"description":loc(&format!("{ns}_description"))}),
    );
    description["Data"]
        .as_object_mut()
        .context("description")?
        .remove("entries");
    let objective = entry(
        &mut h,
        "gameJournalQuestObjective",
        "finish",
        vec![description],
        json!({"description":loc(&format!("{ns}_finish")),"counter":0,"optional":0,"districtID":"","itemID":tweak64("0"),"locationPrefabRef":node64("0")}),
    );
    let objective_pin = |id: &str, reference: &str, h: &mut Handles| {
        entry(
            h,
            "gameJournalQuestMapPin",
            id,
            vec![],
            json!({"enableGPS":1,"reference":entity(reference,&[]),"slotName":n::cname("UI_Interaction"),"offset":{"$type":"Vector3","X":0,"Y":0,"Z":0.5},"mappinData":{"$type":"gamemappinsMappinData","active":1,"debugCaption":"","localizedCaption":loc(""),"mappinType":n::tweak_id("Mappins.QuestStaticMappinDefinition"),"variant":"DefaultQuestVariant","visibleThroughWalls":1}}),
        )
    };
    let pin = objective_pin(
        "waiting_spot",
        crate::route::text(b, "activation_map_ref")?,
        &mut h,
    );
    let start = entry(
        &mut h,
        "gameJournalQuestObjective",
        "01a_wait_for_race",
        vec![pin],
        json!({"description":loc(&format!("{ns}_go_to_start")),"counter":0,"optional":0,"locationPrefabRef":node64("0")}),
    );
    let mount = entry(
        &mut h,
        "gameJournalQuestObjective",
        "01_get_in",
        vec![],
        json!({"description":loc(&format!("{ns}_get_in")),"optional":0}),
    );
    let countdown = entry(
        &mut h,
        "gameJournalQuestObjective",
        "01a_wait_for_race1",
        vec![],
        json!({"description":loc(&format!("{ns}_wait_countdown")),"optional":0}),
    );
    let pin = objective_pin(
        "reward_crate",
        crate::route::text(b, "reward_crate")?,
        &mut h,
    );
    let reward = entry(
        &mut h,
        "gameJournalQuestObjective",
        "04_get_reward",
        vec![pin],
        json!({"description":loc(&format!("{ns}_get_reward")),"optional":0}),
    );
    let phase = entry(
        &mut h,
        "gameJournalQuestPhase",
        "race",
        vec![start, mount, countdown, objective, reward],
        json!({"locationPrefabRef":node64("0")}),
    );
    let quest = entry(
        &mut h,
        "gameJournalQuest",
        ns,
        vec![phase],
        json!({"title":loc(&format!("{ns}_title")),"type":"MinorQuest","districtID":"","recommendedLevelID":tweak64("0")}),
    );
    let folder = entry(
        &mut h,
        "gameJournalFolderEntry",
        "minor_quest",
        vec![quest],
        json!({}),
    );
    let primary = entry(
        &mut h,
        "gameJournalPrimaryFolderEntry",
        "quests",
        vec![folder],
        json!({}),
    );
    let variant =
        h.wrap(json!({"$type":"gamemappinsCommonVariant","variant":"Zzz18_RacingVariant"}));
    let poi=h.wrap(json!({"$type":"gameJournalPointOfInterestMappin","id":format!("{ns}_start"),"journalEntryOverrideDataList":[],"staticNodeRef":node64("0"),"dynamicEntityRef":entity(crate::route::text(b,"activation_map_ref")?,&[]),"offset":{"$type":"Vector3","X":b["activation_map_position"]["x"],"Y":b["activation_map_position"]["y"],"Z":b["activation_map_position"]["z"]},"questPath":null,"mappinData":{"$type":"gamemappinsPointOfInterestMappinData","active":1,"dynamicMappinRadius":30,"dynamicMappinDef":n::tweak_id("Mappins.DynamicPointOfInterestMappinDefinition"),"staticMappinDef":n::tweak_id("Mappins.StaticPointOfInterestMappinDefinition"),"slotName":n::cname("UI_Interaction"),"slotOffset":{"$type":"Vector3","X":0,"Y":0,"Z":0.5},"typedVariant":variant}}));
    let group = entry(
        &mut h,
        "gameJournalPointOfInterestGroup",
        "street_race",
        vec![poi],
        json!({}),
    );
    let pois = entry(
        &mut h,
        "gameJournalPrimaryFolderEntry",
        "points_of_interest",
        vec![group],
        json!({}),
    );
    let root = entry(
        &mut h,
        "gameJournalRootFolderEntry",
        "",
        vec![primary, pois],
        json!({"descriptor":n::resource("base\\journal\\descriptor.journaldesc")}),
    );
    Ok(document(
        json!({"$type":"gameJournalResource","cookingPlatform":"PLATFORM_PC","entry":root}),
        depot,
    ))
}
pub fn localization(config: &Value, depot: &str) -> Value {
    let ns = config["id"].as_str().unwrap_or("race");
    let descriptions = [
        ("title", config["title"].as_str().unwrap_or("Race")),
        ("finish", "Finish the race."),
        ("go_to_start", "Wait for the race."),
        ("get_in", "Get in your vehicle."),
        ("wait_countdown", "Wait for the race to start."),
        ("get_reward", "Collect your reward."),
        (
            "description",
            config["description"]
                .as_str()
                .unwrap_or("Complete the checkpoints in order and beat the other racers."),
        ),
    ];
    document(
        json!({"$type":"JsonResource","cookingPlatform":"PLATFORM_PC","root":{"HandleId":"0","Data":{"$type":"localizationPersistenceOnScreenEntries","entries":descriptions.iter().map(|(key,text)|ghostline_quest_builder::localization::onscreen_entry(&format!("{ns}_{key}"),text)).collect::<Vec<_>>()}}}),
        depot,
    )
}

use ghostline_race_builder::{
    route,
    series::{self, RaceSeries, SeriesRace},
};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};

fn fixture() -> (RaceSeries, Vec<Value>) {
    let mut projects = Vec::new();
    let mut races = Vec::new();
    for index in 1..=9 {
        let id = format!("championship_race_{index}");
        let mut config: Value =
            serde_json::from_str(include_str!("../examples/race.json")).unwrap();
        config["id"] = json!(id);
        config["title"] = json!(format!("Race {index}"));
        let mut route = route::import(
            serde_json::from_str(include_str!("../examples/game-scout-export.json")).unwrap(),
        )
        .unwrap();
        route["id"] = json!(id);
        route["prefix"] = json!("championship_test");
        route.as_object_mut().unwrap().remove("world");
        for mark in route["marks"].as_array_mut().unwrap() {
            mark.as_object_mut().unwrap().remove("native_ref");
            mark.as_object_mut().unwrap().remove("native_trigger_ref");
        }
        races.push(SeriesRace {
            title: format!("Race {index}"),
            project: Some(id),
            invitation: String::new(),
        });
        projects.push(json!({"config":config,"route":route}));
    }
    (
        RaceSeries {
            format: "ghostline-race-series".into(),
            schema_version: 1,
            id: "championship_test".into(),
            title: "Championship".into(),
            contact_name: "Organiser".into(),
            intro_message: "Win all nine races.".into(),
            completion_message: "Your bonus has been transferred.".into(),
            reward_eddies: 100000,
            quest_core_slot: None,
            races,
        },
        projects,
    )
}
fn objects<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(fields) => {
            out.push(v);
            for child in fields.values() {
                objects(child, out);
            }
        }
        Value::Array(rows) => {
            for child in rows {
                objects(child, out);
            }
        }
        _ => {}
    }
}
fn definitions(document: &Value) -> HashMap<String, Value> {
    let mut all = Vec::new();
    objects(document, &mut all);
    all.into_iter()
        .filter_map(|v| Some((v["HandleId"].as_str()?.to_owned(), v.get("Data")?.clone())))
        .collect()
}
fn handle(v: &Value) -> &str {
    v.get("HandleId")
        .or_else(|| v.get("HandleRefId"))
        .and_then(Value::as_str)
        .unwrap()
}

/// Execute the emitted controller's native connections and fact conditions.
struct Controller {
    defs: HashMap<String, Value>,
    nodes: HashMap<String, Value>,
    owners: HashMap<String, String>,
    facts: HashMap<String, i64>,
    paused: HashSet<String>,
    queue: VecDeque<String>,
    cash: i64,
    locked: bool,
    messages: Vec<String>,
    children: usize,
}
impl Controller {
    fn new(document: &Value, facts: HashMap<String, i64>) -> Self {
        let defs = definitions(document);
        let graph = &defs[handle(&document["Data"]["RootChunk"]["graph"])];
        let nodes = graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| (handle(n).to_owned(), defs[handle(n)].clone()))
            .collect::<HashMap<_, _>>();
        let mut owners = HashMap::new();
        let mut queue = VecDeque::new();
        for (id, node) in &nodes {
            if node["$type"] == "questInputNodeDefinition" {
                queue.push_back(id.clone());
            }
            for socket in node["sockets"].as_array().unwrap() {
                owners.insert(handle(socket).to_owned(), id.clone());
            }
        }
        Self {
            defs,
            nodes,
            owners,
            facts,
            paused: HashSet::new(),
            queue,
            cash: 0,
            locked: false,
            messages: Vec::new(),
            children: 0,
        }
    }
    fn resolve<'a>(&'a self, value: &'a Value) -> &'a Value {
        if value.get("HandleId").is_some() || value.get("HandleRefId").is_some() {
            &self.defs[handle(value)]
        } else {
            value
        }
    }
    fn pump(&mut self) {
        for id in std::mem::take(&mut self.paused) {
            self.queue.push_back(id);
        }
        while let Some(id) = self.queue.pop_front() {
            let node = self.nodes[&id].clone();
            let mut out = "Out";
            match node["$type"].as_str().unwrap() {
                "questConditionNodeDefinition" | "questPauseConditionNodeDefinition" => {
                    let condition = self.resolve(&node["condition"]);
                    let kind = self.resolve(&condition["type"]);
                    let pass = if condition["$type"] == "questContentCondition" {
                        true
                    } else {
                        let actual = self
                            .facts
                            .get(kind["factName"].as_str().unwrap())
                            .copied()
                            .unwrap_or(0);
                        let value = kind["value"].as_i64().unwrap();
                        match kind["comparisonType"].as_str().unwrap() {
                            "Equal" => actual == value,
                            "Greater" => actual > value,
                            other => panic!("Unhandled comparison {other}"),
                        }
                    };
                    if node["$type"] == "questPauseConditionNodeDefinition" {
                        if !pass {
                            self.paused.insert(id);
                            continue;
                        }
                    } else {
                        out = if pass { "True" } else { "False" };
                    }
                }
                "questFactsDBManagerNodeDefinition" => {
                    let kind = self.resolve(&node["type"]).clone();
                    assert_eq!(kind["setExactValue"], true);
                    self.facts.insert(
                        kind["factName"].as_str().unwrap().into(),
                        kind["value"].as_i64().unwrap(),
                    );
                }
                "questJournalNodeDefinition" => {
                    let kind = self.resolve(&node["type"]);
                    let path = self.resolve(&kind["path"]);
                    if path["className"]["$value"] == "gameJournalPhoneMessage" {
                        self.messages
                            .push(path["realPath"].as_str().unwrap().into());
                    }
                }
                "questCheckpointNodeDefinition" => {
                    self.locked = node["saveLock"] == 1;
                }
                "questItemManagerNodeDefinition" => {
                    assert!(
                        self.locked,
                        "Payout must not be saveable before its durable receipt"
                    );
                    let kind = self.resolve(&node["type"]);
                    let params = self.resolve(&kind["params"][0]);
                    self.cash += params["quantity"].as_i64().unwrap();
                }
                "questPhaseNodeDefinition" => {
                    self.children += 1;
                    continue;
                }
                "questInputNodeDefinition" => {}
                other => panic!("Unhandled controller node {other}"),
            }
            let mut next = Vec::new();
            for socket in node["sockets"].as_array().unwrap() {
                let socket = self.resolve(socket);
                if socket["type"] != "Output" || socket["name"]["$value"] != out {
                    continue;
                }
                for edge in socket["connections"].as_array().unwrap() {
                    let edge = self.resolve(edge);
                    next.push(self.owners[handle(&edge["destination"])].clone());
                }
            }
            self.queue.extend(next);
        }
    }
}

#[test]
fn nine_unique_wins_unlock_in_order_pay_once_and_keep_replays_alive() {
    let (series, projects) = fixture();
    let (build, bindings, _) = series::documents(&series, &projects).unwrap();
    let phase = build
        .artifacts
        .iter()
        .find(|(p, _)| p.ends_with("_series.questphase"))
        .unwrap()
        .1;
    let registration: Value =
        serde_json::from_slice(&build.resources["championship_test.archive.xl"]).unwrap();
    assert!(registration.get("quest").is_none());
    let bootstrap = build.bootstrap.as_ref().unwrap();
    assert_eq!(bootstrap.phases.len(), 1);
    assert_eq!(bootstrap.phases[0].input, "In1");
    assert!(bootstrap.phases[0].output.is_none());
    assert_eq!(bootstrap.workspot_count(), 0);
    let mut all_nodes = Vec::new();
    for artifact in build.artifacts.values() {
        objects(artifact, &mut all_nodes);
    }
    for registry in all_nodes
        .iter()
        .filter(|node| node["$type"] == "worldCommunityRegistryNode")
    {
        assert!(
            registry["workspotsPersistentData"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    assert!(bootstrap.phases[0].resource.ends_with("_series.questphase"));
    let scene = &build.artifacts["mod\\quest_bootstrap\\championship_test\\start.scene"];
    let scene_definitions = definitions(scene);
    let calls = scene_definitions
        .values()
        .filter(|node| node["$type"] == "questPhaseNodeDefinition")
        .collect::<Vec<_>>();
    assert!(calls.is_empty());
    assert!(
        scene
            .to_string()
            .contains("championship_test_bootstrap_requested")
    );
    assert_eq!(
        scene["Data"]["RootChunk"]["entryPoints"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        registration["streaming"]["blocks"]
            .as_array()
            .unwrap()
            .len(),
        10
    );
    let mut controller = Controller::new(phase, HashMap::new());
    controller.pump();
    assert_eq!(controller.children, 9);
    for index in 0..9 {
        let id = bindings["races"][index]["id"].as_str().unwrap();
        let membership = series.membership(index, id);
        assert_eq!(controller.facts.get(&membership.unlock_fact()), Some(&1));
        assert_eq!(controller.cash, 0);
        if index + 1 < 9 {
            let next = series.membership(
                index + 1,
                bindings["races"][index + 1]["id"].as_str().unwrap(),
            );
            assert!(!controller.facts.contains_key(&next.unlock_fact()));
        }
        // Model the child race arming its start trigger and finishing cleanup.
        controller.facts.insert(format!("{id}_waiting_armed"), 1);
        controller.pump();
        controller.facts.insert(membership.win_fact(), 1);
        controller.pump();
        assert_eq!(controller.cash, 0);
        controller.facts.insert(format!("{id}_settled"), 1);
        controller.pump();
        // Replaying an earlier event cannot replace the next distinct win.
        controller.facts.insert(
            series
                .membership(0, bindings["races"][0]["id"].as_str().unwrap())
                .win_fact(),
            1,
        );
        controller.pump();
        if index < 8 {
            assert_eq!(controller.cash, 0);
        }
    }
    assert_eq!(controller.cash, 100000);
    assert!(!controller.locked);
    assert_eq!(controller.messages.len(), 11);
    assert_eq!(controller.facts["series_championship_test_reward_paid"], 1);
    // Re-entering the controller with saved facts neither pays nor resends messages.
    let mut loaded = Controller::new(phase, controller.facts);
    loaded.pump();
    assert_eq!(loaded.cash, 0);
    assert!(loaded.messages.is_empty());
    assert_eq!(loaded.children, 9);
    for race in bindings["races"].as_array().unwrap() {
        let membership: series::Membership =
            serde_json::from_value(race["series"].clone()).unwrap();
        let root = &build.artifacts[race["root_phase"].as_str().unwrap()];
        let life = &build.artifacts[race["lifecycle_phase"].as_str().unwrap()];
        assert!(root.to_string().contains(&membership.unlock_fact()));
        let defs = definitions(life);
        let win = defs
            .values()
            .filter(|v| {
                v["$type"] == "questSetVar_NodeType" && v["factName"] == membership.win_fact()
            })
            .collect::<Vec<_>>();
        assert_eq!(win.len(), 1);
        assert_eq!(win[0]["value"], 1);
        assert_eq!(win[0]["setExactValue"], true);
    }
}

#[test]
fn drafts_reject_missing_or_repeated_courses_before_publication() {
    let (mut series, _) = fixture();
    series.races[8].project = None;
    assert!(series.validate(false).is_ok());
    assert!(
        series
            .validate(true)
            .unwrap_err()
            .to_string()
            .contains("race 9")
    );
    series.races[8].project = series.races[0].project.clone();
    assert!(series.validate(false).is_err());
}

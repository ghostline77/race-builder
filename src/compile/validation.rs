//! Audits generated graph topology, world bindings and scene reference tables.
use anyhow::{Context, Result, ensure};
use ghostline_quest_builder::{audit, compiler::Artifacts};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

fn objects<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(fields) => {
            out.push(value);
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
fn handle(value: &Value) -> Result<&str> {
    value
        .get("HandleId")
        .or_else(|| value.get("HandleRefId"))
        .and_then(Value::as_str)
        .context("Handle identity")
}
fn resolve<'a>(value: &'a Value, defs: &HashMap<&str, &'a Value>) -> Result<&'a Value> {
    defs.get(handle(value)?)
        .copied()
        .context("Undefined native handle")
}
fn name(value: &Value) -> Option<&str> {
    value["$value"].as_str()
}

fn phase(document: &Value, defs: &HashMap<&str, &Value>) -> Result<()> {
    audit::Phase::read(document)?;
    let graph = resolve(&document["Data"]["RootChunk"]["graph"], defs)?;
    let wrappers = graph["nodes"].as_array().context("Phase nodes")?;
    let mut owners = HashMap::new();
    let mut nodes = HashMap::new();
    let mut starts = Vec::new();
    for wrapper in wrappers {
        let id = handle(wrapper)?;
        let node = resolve(wrapper, defs)?;
        nodes.insert(id, node);
        if node["$type"] == "questInputNodeDefinition" {
            starts.push(id);
        }
        for socket in node["sockets"].as_array().context("Node sockets")? {
            owners.insert(handle(socket)?, id);
        }
    }
    let mut edges: HashMap<&str, Vec<&str>> = HashMap::new();
    for (socket, owner) in &owners {
        let socket = defs[socket];
        if socket["type"] != "Output" || name(&socket["name"]) == Some("CutSource") {
            continue;
        }
        for connection in socket["connections"].as_array().context("Connections")? {
            let connection = resolve(connection, defs)?;
            edges
                .entry(owner)
                .or_default()
                .push(owners[handle(&connection["destination"])?]);
        }
    }
    let mut seen = HashSet::new();
    while let Some(node) = starts.pop() {
        if seen.insert(node) {
            starts.extend(edges.get(node).into_iter().flatten().copied());
        }
    }
    let missing = nodes
        .iter()
        .filter(|(id, _)| !seen.contains(**id))
        .map(|(_, node)| node["id"].to_string())
        .collect::<Vec<_>>();
    ensure!(
        missing.is_empty(),
        "Unreachable quest nodes: {}",
        missing.join(", ")
    );
    Ok(())
}

fn scene(root: &Value, defs: &HashMap<&str, &Value>) -> Result<()> {
    let graph = resolve(&root["sceneGraph"], defs)?;
    let rows = graph["graph"].as_array().context("Scene graph")?;
    let mut nodes = HashMap::new();
    for row in rows {
        let node = resolve(row, defs)?;
        let id = node["nodeId"]["id"].as_u64().context("Scene node ID")?;
        ensure!(
            nodes.insert(id, node).is_none(),
            "Duplicate scene node ID {id}"
        );
    }
    let actors = root["actors"]
        .as_array()
        .context("Actors")?
        .iter()
        .chain(root["playerActors"].as_array().context("Player actors")?)
        .collect::<Vec<_>>();
    let lipsync_count = root["resouresReferences"]["lipsyncAnimSets"]
        .as_array()
        .context("Lipsync references")?
        .len() as u64;
    for (index, actor) in actors.iter().enumerate() {
        ensure!(
            actor["actorId"]["id"].as_u64() == Some(index as u64),
            "Actor IDs must match table slots"
        );
        let lip = actor["lipsyncAnimSet"]["id"]
            .as_u64()
            .context("Lipsync ID")?;
        ensure!(
            lip == u32::MAX as u64 || lip < lipsync_count,
            "Missing actor lipsync reference"
        );
    }
    let options = root["screenplayStore"]["options"]
        .as_array()
        .context("Screenplay options")?
        .iter()
        .filter_map(|o| o["itemId"]["id"].as_u64())
        .collect::<HashSet<_>>();
    let mut events = HashSet::new();
    for (number, node) in &nodes {
        let sockets = node["outputSockets"]
            .as_array()
            .context("Scene output sockets")?;
        let mut stamps = HashSet::new();
        for socket in sockets {
            ensure!(
                stamps.insert((
                    socket["stamp"]["name"].as_u64(),
                    socket["stamp"]["ordinal"].as_u64()
                )),
                "Duplicate scene socket stamp"
            );
            for destination in socket["destinations"]
                .as_array()
                .context("Scene destinations")?
            {
                let target = nodes
                    .get(
                        &destination["nodeId"]["id"]
                            .as_u64()
                            .context("Target node ID")?,
                    )
                    .context("Missing scene target")?;
                if target["$type"] == "scnQuestNode" {
                    ensure!(
                        destination["isockStamp"]["name"] == 0
                            && destination["isockStamp"]["ordinal"]
                                .as_u64()
                                .is_some_and(|v| v < target["isockMappings"]
                                    .as_array()
                                    .map_or(0, Vec::len)
                                    as u64),
                        "Scene {number} targets invalid quest input"
                    );
                }
            }
        }
        if node["$type"] == "scnChoiceNode" {
            let rows = node["options"].as_array().context("Choice options")?;
            ensure!(
                sockets.len() == rows.len() + 6,
                "Choice socket padding is invalid"
            );
            for option in rows {
                ensure!(
                    options.contains(
                        &option["screenplayOptionId"]["id"]
                            .as_u64()
                            .context("Choice screenplay ID")?
                    ),
                    "Missing screenplay option"
                );
            }
        }
        if node["$type"] == "scnSectionNode" {
            let duration = node["sectionDuration"]["stu"]
                .as_u64()
                .context("Section duration")?;
            for wrapper in node["events"].as_array().context("Scene events")? {
                let event = resolve(wrapper, defs)?;
                ensure!(
                    events.insert(event["id"]["id"].to_string()),
                    "Duplicate scene event ID"
                );
                let start = event["startTime"].as_u64().context("Event start")?;
                let length = event["duration"].as_u64().context("Event duration")?;
                ensure!(
                    start < duration && length <= duration - start,
                    "Event outside section {number}"
                );
                if event["$type"] == "scneventsSocket" {
                    ensure!(
                        stamps.contains(&(
                            event["osockStamp"]["name"].as_u64(),
                            event["osockStamp"]["ordinal"].as_u64()
                        )),
                        "Event emits undeclared socket"
                    );
                }
            }
        }
    }
    for (registry, kind, points) in [
        ("startNodes", "scnStartNode", "entryPoints"),
        ("endNodes", "scnEndNode", "exitPoints"),
    ] {
        let declared = if root["sceneCategoryTag"] == "otherOpenWorld" {
            root[points]
                .as_array()
                .context("Conversation public points")?
                .iter()
                .filter_map(|point| point["nodeId"]["id"].as_u64())
                .collect::<HashSet<_>>()
        } else {
            graph[registry]
                .as_array()
                .context("Scene registry")?
                .iter()
                .filter_map(|value| value["id"].as_u64())
                .collect::<HashSet<_>>()
        };
        let actual = nodes
            .iter()
            .filter(|(_, v)| v["$type"] == kind)
            .map(|(k, _)| *k)
            .collect::<HashSet<_>>();
        ensure!(declared == actual, "Scene boundary registry mismatch");
        for point in root[points].as_array().context("Public scene points")? {
            ensure!(
                declared.contains(&point["nodeId"]["id"].as_u64().context("Public point ID")?),
                "Public point missing from scene boundaries"
            );
        }
    }
    let mut values = Vec::new();
    objects(root, &mut values);
    for value in values {
        match value["$type"].as_str() {
            Some("scnActorId") => {
                let id = value["id"].as_u64().context("Actor reference")?;
                ensure!(
                    id == u32::MAX as u64 || id < actors.len() as u64,
                    "Missing scene actor {id}"
                );
            }
            Some("scnPerformerId") => {
                let id = value["id"].as_u64().context("Performer reference")?;
                ensure!(
                    id == u64::from(ghostline_quest_builder::scene::INVALID_PERFORMER_ID)
                        || root["props"].as_array().into_iter().flatten().any(|p|p["propId"]["id"].as_u64().map(|v|v*256+2)==Some(id))
                        || actors
                            .iter()
                            .any(|a| a["actorId"]["id"].as_u64().map(|v| v * 256 + 1) == Some(id)),
                    "Missing scene performer {id}"
                );
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn resources(artifacts: &Artifacts, bindings: &Value) -> Result<()> {
    audit::bound_interfaces(artifacts)?;
    let mut all = Vec::new();
    for document in artifacts.values() {
        objects(document, &mut all);
    }
    let mut refs = HashSet::<String>::new();
    let mut hashes = HashSet::new();
    let mut entries = HashSet::new();
    for value in &all {
        match value["$type"].as_str() {
            Some("worldStreamingSector") => {
                for reference in value["nodeRefs"]
                    .as_array()
                    .context("Sector node references")?
                {
                    if let Some(text) = name(reference) {
                        refs.insert(text.to_owned());
                        refs.insert(text.rsplit('/').next().unwrap_or(text).to_owned());
                    }
                }
                for placement in value["nodeData"]["Data"]
                    .as_array()
                    .context("Sector placements")?
                {
                    if let Some(hash) = name(&placement["QuestPrefabRefHash"]) {
                        hashes.insert(hash.to_owned());
                    }
                }
            }
            Some("communitySpawnEntry") => {
                if let Some(entry) = name(&value["entryName"]) {
                    entries.insert(entry.to_owned());
                }
            }
            _ => {}
        }
    }
    refs.insert(crate::route::text(bindings, "prefab")?.to_owned());
    for reference in bindings["native_refs"].as_array().into_iter().flatten() {
        refs.insert(
            reference
                .as_str()
                .context("Native NodeRef binding")?
                .to_owned(),
        );
    }
    for prefab in bindings["prefabs"].as_array().into_iter().flatten() {
        refs.insert(prefab.as_str().context("Series prefab binding")?.to_owned());
    }
    hashes.extend(
        refs.iter()
            .map(|s| ghostline_quest_builder::world::node_ref_hash(s).to_string()),
    );
    fn journal(
        value: &Value,
        parent: &str,
        defs: &HashMap<&str, &Value>,
        paths: &mut HashSet<String>,
    ) -> Result<()> {
        let entry = resolve(value, defs)?;
        let path = if parent.is_empty() {
            entry["id"].as_str().context("Journal ID")?.to_owned()
        } else {
            format!("{parent}/{}", entry["id"].as_str().context("Journal ID")?)
        };
        paths.insert(path.clone());
        for child in entry["entries"].as_array().into_iter().flatten() {
            journal(child, &path, defs, paths)?;
        }
        Ok(())
    }
    let mut paths = HashSet::new();
    for document in artifacts.values() {
        let mut values = Vec::new();
        objects(document, &mut values);
        let defs = values
            .iter()
            .filter_map(|v| Some((v["HandleId"].as_str()?, &v["Data"])))
            .collect::<HashMap<_, _>>();
        if document["Data"]["RootChunk"]["$type"] == "gameJournalResource" {
            journal(
                &document["Data"]["RootChunk"]["entry"],
                "",
                &defs,
                &mut paths,
            )?;
        }
    }
    for (depot, document) in artifacts {
        let mut values = Vec::new();
        objects(document, &mut values);
        let defs = values
            .iter()
            .filter_map(|v| Some((v["HandleId"].as_str()?, &v["Data"])))
            .collect::<HashMap<_, _>>();
        for value in &values {
            match value["$type"].as_str() {
                Some("NodeRef") => {
                    let text = name(value).context("NodeRef")?;
                    ensure!(
                        ["0", "", "#player", "#mounted_vehicle", "#owned_vehicle"].contains(&text)
                            || refs.contains(text)
                            || hashes.contains(text),
                        "{depot}: unresolved NodeRef {text}"
                    );
                }
                Some("gameEntityReference")
                    if [bindings["community"].as_str(), bindings["crowd"].as_str()]
                        .contains(&name(&value["reference"])) =>
                {
                    for entry in value["names"].as_array().into_iter().flatten() {
                        ensure!(
                            entries.contains(name(entry).context("Community entry name")?),
                            "{depot}: missing community entry"
                        );
                    }
                }
                Some("gameJournalPath") => ensure!(
                    paths.contains(value["realPath"].as_str().context("Journal path")?),
                    "{depot}: missing journal path {}",
                    value["realPath"]
                ),
                _ => {}
            }
        }
        let root = &document["Data"]["RootChunk"];
        match root["$type"].as_str() {
            Some("worldStreamingBlock") => {
                for descriptor in root["descriptors"].as_array().context("Streaming descriptors")? {
                    let sector_path = name(&descriptor["data"]["DepotPath"])
                        .context("Streaming sector path")?;
                    let sector = &artifacts.get(sector_path)
                        .with_context(|| format!("Missing streaming sector {sector_path}"))?["Data"]["RootChunk"];
                    let ranges = sector["variantIndices"].as_array()
                        .context("Streaming sector node ranges")?;
                    let declared = descriptor["numNodeRanges"].as_u64()
                        .context("Declared streaming node range count")?;
                    ensure!(declared == ranges.len() as u64,
                        "{depot}: {sector_path} declares {declared} node ranges but contains {}", ranges.len());
                    for variant in descriptor["variants"].as_array().into_iter().flatten() {
                        let range = variant["rangeIndex"].as_u64().context("Variant range index")?;
                        ensure!(range < declared,
                            "{depot}: {sector_path} variant range {range} exceeds {declared} node ranges");
                    }
                }
            }
            Some("questQuestPhaseResource") => {
                phase(document, &defs).with_context(|| depot.clone())?
            }
            Some("scnSceneResource") => scene(root, &defs).with_context(|| depot.clone())?,
            _ => {}
        }
    }
    Ok(())
}

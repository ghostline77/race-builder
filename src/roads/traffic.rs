//! Native traffic lanes supply both continuous geometry and explicit junctions.
use super::{RoadGraph, number};
use crate::route::{distance, point, position};
use anyhow::{Context, Result, ensure};
use ghostline_red::{material::ArchiveSet, schema::RedSchema, traffic};
use serde_json::{Value, json};
use std::{collections::HashMap, path::Path};

// worldTrafficLanePersistentFlags. GPSOnly road curves still describe real
// junction turns used by the generated race spline, independent of ambient
// traffic restrictions. Pavements and NoAIDriving paths are not race roads.
pub(super) fn drivable(flags: u64) -> bool {
    const ROAD: u64 = 16;
    const NO_AI_DRIVING: u64 = 8192;
    flags & ROAD != 0 && flags & NO_AI_DRIVING == 0
}

#[test]
fn only_ai_drivable_roads_are_routable() {
    assert!(drivable(17));
    assert!(drivable(16401));
    assert!(!drivable(40970));
    assert!(drivable(689));
    assert!(!drivable(16 | 8192));
}

fn vertices(road: &Value) -> Result<Vec<[f64; 3]>> {
    road["points"]
        .as_array()
        .context("Lane points")?
        .iter()
        .map(|p| {
            if let Some(p) = p.as_array() {
                ensure!(p.len() == 3, "Lane point requires XYZ");
                Ok([number(&p[0])?, number(&p[1])?, number(&p[2])?])
            } else {
                position(p)
            }
        })
        .collect()
}
fn stations(points: &[[f64; 3]]) -> Vec<f64> {
    let mut length = 0.;
    let mut result = vec![0.];
    for pair in points.windows(2) {
        length += distance(pair[0], pair[1]);
        result.push(length);
    }
    result
}
fn sample(points: &[[f64; 3]], stations: &[f64], s: f64) -> Result<[f64; 3]> {
    let length = *stations.last().context("Lane stations")?;
    ensure!(
        s.is_finite() && s >= 0. && s <= length + 1.,
        "Transition station {s} outside lane length {length}"
    );
    let s = s.min(length);
    for (i, pair) in stations.windows(2).enumerate() {
        if s <= pair[1] && pair[1] > pair[0] {
            let t = (s - pair[0]) / (pair[1] - pair[0]);
            return Ok(std::array::from_fn(|axis| {
                points[i][axis] + (points[i + 1][axis] - points[i][axis]) * t
            }));
        }
    }
    points.last().copied().context("Empty lane")
}

/// Reads the game's complete persistent lane geometry and native connection table.
pub fn extract_traffic(game: &Path, schema_path: Option<&Path>, root: &str) -> Result<Value> {
    let schema = RedSchema::load(Some(game), schema_path)?;
    let archives = ArchiveSet::open(&game.join("archive/pc"))?;
    let polygons = format!("{}\\all.lane_polygons", root.trim_end_matches(['\\', '/']));
    let connections = format!(
        "{}\\all.lane_connections",
        root.trim_end_matches(['\\', '/'])
    );
    let read = |path: &str| -> Result<Value> {
        ghostline_quest_builder::inspect::decode(&archives.read_resource(path)?, Some(&schema))
    };
    let geometry = traffic::read_polygons(&read(&polygons)?)?;
    let topology = traffic::read_connections(&read(&connections)?)?;
    let persistent = format!(
        "{}\\all.traffic_persistent",
        root.trim_end_matches(['\\', '/'])
    );
    let metadata = read(&persistent)?;
    let lane_metadata = metadata["Data"]["RootChunk"]["data"]["lanes"]
        .as_array()
        .context("Native traffic lane metadata")?;
    let mut roads = Vec::new();
    let mut lookup = HashMap::new();
    let mut failures = Vec::new();
    for lane in geometry {
        if lane.outline.len() < 2 {
            failures.push(json!({"lane":lane.index,"error":"Native lane has fewer than two centreline points"}));
            continue;
        }
        let points = lane
            .outline
            .iter()
            .map(|p| p.map(f64::from))
            .collect::<Vec<_>>();
        let station = stations(&points);
        lookup.insert(lane.index, (points, station));
        let metadata = lane_metadata
            .get(usize::from(lane.index))
            .context("Missing native lane metadata")?;
        let flags = metadata["flags"].as_u64().unwrap_or(0);
        roads.push(json!({"id":format!("lane_{}",lane.index),"corridor":metadata["nodeRefHash"],"drivable":drivable(flags),"lane_flags":flags,"source":{"resource":polygons,"lane_index":lane.index},"points":lane.outline,"polygon":lane.polygon}));
    }
    let mut links = Vec::new();
    for lane in topology {
        for exit in lane.outgoing {
            let result = (|| -> Result<Value> {
                let from = lookup
                    .get(&lane.index)
                    .context("Missing source lane geometry")?;
                let to = lookup
                    .get(&exit.lane)
                    .context("Missing destination lane geometry")?;
                let a = sample(&from.0, &from.1, f64::from(exit.exit_m))?;
                let b = sample(&to.0, &to.1, f64::from(exit.entry_m))?;
                Ok(
                    json!({"from":format!("lane_{}",lane.index),"to":format!("lane_{}",exit.lane),"from_m":exit.exit_m,"to_m":exit.entry_m,"from_position":point(a),"to_position":point(b),"sharp_angle":exit.sharp_angle}),
                )
            })();
            match result {
                Ok(link) => links.push(link),
                Err(error) => failures.push(
                    json!({"lane":lane.index,"target_lane":exit.lane,"error":format!("{error:#}")}),
                ),
            }
        }
    }
    ensure!(
        !roads.is_empty() && !links.is_empty(),
        "Native traffic network contains no connected lanes"
    );
    Ok(
        json!({"format":"ghostline-road-network","schema_version":1,"topology":"native-traffic","coordinate_system":"cyberpunk-world-metres-z-up","coverage":"Native persistent traffic lanes and explicit lane connections","resources":[polygons,connections,persistent],"roads":roads,"connections":links,"failures":failures}),
    )
}

pub(super) fn graph(network: Value) -> Result<RoadGraph> {
    let mut anchors: HashMap<&str, Vec<f64>> = HashMap::new();
    let connections = network["connections"]
        .as_array()
        .context("Native lane connections")?;
    for connection in connections {
        for (road, station) in [("from", "from_m"), ("to", "to_m")] {
            anchors
                .entry(connection[road].as_str().context("Connection lane ID")?)
                .or_default()
                .push(number(&connection[station])?);
        }
    }
    let mut graph = RoadGraph::default();
    graph.directed = true;
    let mut lanes: HashMap<&str, Vec<(f64, usize)>> = HashMap::new();
    for road in network["roads"].as_array().context("Native lanes")? {
        if road["drivable"] == false {
            continue;
        }
        let id = road["id"].as_str().context("Lane ID")?;
        if let Some(corridor) = road["corridor"].as_str().filter(|value| *value != "0") {
            graph.corridors.insert(id.to_owned(), corridor.to_owned());
        }
        let points = vertices(road)?;
        ensure!(points.len() >= 2, "Lane needs two points");
        let station = stations(&points);
        let length = *station.last().context("Lane length")?;
        let mut entries = station
            .iter()
            .copied()
            .zip(points.iter().copied())
            .collect::<Vec<_>>();
        for &s in anchors.get(id).into_iter().flatten() {
            entries.push((s.min(length), sample(&points, &station, s)?));
        }
        entries.sort_by(|a, b| a.0.total_cmp(&b.0));
        entries.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-5);
        let mut indices = Vec::new();
        for (s, p) in entries {
            let index = graph.points.len();
            graph.points.push(p);
            graph.links.push(Vec::new());
            indices.push((s, index));
        }
        for pair in indices.windows(2) {
            let (a, b) = (pair[0].1, pair[1].1);
            let d = distance(graph.points[a], graph.points[b]);
            if d > 0. {
                graph.links[a].push((b, d));
                graph.segments.push((a, b, id.to_owned()));
            }
        }
        ensure!(
            lanes.insert(id, indices).is_none(),
            "Duplicate native lane ID"
        );
    }
    let anchor = |id: &str, s: f64| -> Result<usize> {
        let lane = lanes
            .get(id)
            .context("Connection references missing lane")?;
        let requested = s.min(lane.last().context("Lane anchors")?.0);
        lane.iter()
            .min_by(|a, b| (a.0 - requested).abs().total_cmp(&(b.0 - requested).abs()))
            .filter(|(station, _)| (*station - requested).abs() < 1e-4)
            .map(|(_, index)| *index)
            .context("Missing transition station")
    };
    for connection in connections {
        if !lanes.contains_key(connection["from"].as_str().context("Source lane")?)
            || !lanes.contains_key(connection["to"].as_str().context("Target lane")?)
        {
            continue;
        }
        let a = anchor(
            connection["from"].as_str().context("Source lane")?,
            number(&connection["from_m"])?,
        )?;
        let b = anchor(
            connection["to"].as_str().context("Target lane")?,
            number(&connection["to_m"])?,
        )?;
        let d = distance(graph.points[a], graph.points[b]);
        // Native turn curves are directed. Reversing these links lets a route
        // enter one turn backwards and switch to another at its shared start,
        // creating the short T-shaped spurs seen at junctions.
        graph.links[a].push((b, d));
        if d > 0.05 {
            graph
                .segments
                .push((a, b, format!("{}→{}", connection["from"], connection["to"])));
        }
    }
    graph.network = network;
    Ok(graph)
}

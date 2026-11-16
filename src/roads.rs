//! Road centreline extraction preserves native deformation frames and height.
use anyhow::{Context, Result, ensure};
use ghostline_red::{material::ArchiveSet, schema::RedSchema};
use glam::DVec3;
use serde_json::{Value, json};
use std::path::Path;
mod display;
mod glyph;
pub mod guidance;
pub(crate) mod joins;
mod native;
mod surface;
mod traffic;
pub use native::sector_roads;
pub use traffic::extract_traffic;

/// Prepare native height-separated crossings for map consumers.
pub fn prepare_display(network: &mut Value) -> Result<()> {
    display::prepare(network)
}

fn unbox(v: &Value) -> &Value {
    v.get("$value").unwrap_or(v)
}
fn number(v: &Value) -> Result<f64> {
    let n = unbox(v).as_f64().context("missing road coordinate")?;
    ensure!(n.is_finite(), "nonfinite road coordinate");
    Ok(n)
}
fn vector(v: &Value) -> Result<DVec3> {
    Ok(DVec3::new(
        number(&v["X"])?,
        number(&v["Y"])?,
        number(&v["Z"])?,
    ))
}
pub fn extract(input: &Value) -> Result<Value> {
    let game = Path::new(input["game"].as_str().context("game directory required")?);
    let schema = RedSchema::load(
        Some(game),
        input["schema"]
            .as_str()
            .filter(|p| !p.is_empty())
            .map(Path::new),
    )?;
    let archives = ArchiveSet::open(&game.join("archive/pc"))?;
    let sectors = input["sectors"]
        .as_array()
        .context("sectors must list exact .streamingsector depot paths")?;
    let mut roads = Vec::new();
    let mut failures = Vec::new();
    let mut decoded = 0;
    for source in sectors {
        let depot = source.as_str().context("sector path")?;
        let result = (|| -> Result<()> {
            let bytes = archives.read_resource(depot)?;
            let document = ghostline_quest_builder::inspect::decode(&bytes, Some(&schema))?;
            let extracted = sector_roads(&document, depot)?;
            roads.extend(
                extracted["roads"]
                    .as_array()
                    .context("Roads")?
                    .iter()
                    .cloned(),
            );
            failures.extend(
                extracted["failures"]
                    .as_array()
                    .context("Failures")?
                    .iter()
                    .cloned(),
            );
            decoded += 1;
            Ok(())
        })();
        if let Err(e) = result {
            failures.push(json!({"source":depot,"error":format!("{e:#}")}));
        }
    }
    Ok(
        json!({"format":"ghostline-road-network","schema_version":1,"coordinate_system":"cyberpunk-world-metres-z-up","roads":roads,"decoded_sectors":decoded,"failures":failures}),
    )
}

use crate::route::{distance, point, position};
use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap},
};

#[derive(Debug, Default)]
pub struct RoadGraph {
    pub network: Value,
    points: Vec<[f64; 3]>,
    links: Vec<Vec<(usize, f64)>>,
    segments: Vec<(usize, usize, String)>,
    corridors: HashMap<String, String>,
    directed: bool,
}
#[derive(Debug, Clone)]
struct Candidate {
    a: usize,
    b: usize,
    position: [f64; 3],
    distance: f64,
    road: String,
}
#[derive(Clone, Copy, Debug)]
struct Step {
    cost: f64,
    node: usize,
}
impl PartialEq for Step {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.node == other.node
    }
}
impl Eq for Step {}
impl PartialOrd for Step {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Step {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| self.node.cmp(&other.node))
    }
}
impl RoadGraph {
    pub fn new(mut network: Value) -> Result<Self> {
        ensure!(
            network["format"] == "ghostline-road-network" && network["schema_version"] == 1,
            "Unsupported road network"
        );
        if network["topology"] == "native-traffic" {
            for road in network["roads"].as_array_mut().context("Native lanes")? {
                if let Some(flags) = road["lane_flags"].as_u64() {
                    road["drivable"] = traffic::drivable(flags).into();
                }
            }
            display::prepare(&mut network)?;
            return traffic::graph(network);
        }
        display::prepare(&mut network)?;
        let mut graph = Self::default();
        let mut buckets: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
        for road in network["roads"].as_array().context("roads array")? {
            let mut indices = Vec::new();
            for value in road["points"].as_array().context("road points")? {
                let p = if let Some(a) = value.as_array() {
                    ensure!(a.len() == 3, "road point requires xyz");
                    [number(&a[0])?, number(&a[1])?, number(&a[2])?]
                } else {
                    position(value)?
                };
                let index = graph.points.len();
                graph.points.push(p);
                graph.links.push(Vec::new());
                indices.push(index);
                let cell = p.map(|v| (v / 4.).floor() as i64);
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        for dz in -1..=1 {
                            if let Some(others) =
                                buckets.get(&[cell[0] + dx, cell[1] + dy, cell[2] + dz])
                            {
                                for &other in others {
                                    let q = graph.points[other];
                                    let d = distance(p, q);
                                    if d <= 2. && (p[2] - q[2]).abs() <= 1.5 {
                                        graph.connect(index, other, d);
                                    }
                                }
                            }
                        }
                    }
                }
                buckets.entry(cell).or_default().push(index);
            }
            for pair in indices.windows(2) {
                let d = distance(graph.points[pair[0]], graph.points[pair[1]]);
                if d > 0. {
                    graph.connect(pair[0], pair[1], d);
                    graph.segments.push((
                        pair[0],
                        pair[1],
                        road["id"].as_str().unwrap_or("road").to_owned(),
                    ));
                }
            }
        }
        graph.network = network;
        Ok(graph)
    }
    fn connect(&mut self, a: usize, b: usize, d: f64) {
        self.links[a].push((b, d));
        self.links[b].push((a, d));
    }
    fn nearest(&self, p: [f64; 3], height: Option<f64>) -> Result<Candidate> {
        self.candidates(p, height)?
            .into_iter()
            .next()
            .context("No road nearby")
    }
    fn candidates(&self, p: [f64; 3], height: Option<f64>) -> Result<Vec<Candidate>> {
        let mut candidates = Vec::new();
        for (a, b, road) in &self.segments {
            let u = self.points[*a];
            let v = self.points[*b];
            let dx = v[0] - u[0];
            let dy = v[1] - u[1];
            let denominator = dx * dx + dy * dy;
            if denominator < 1e-8 {
                continue;
            }
            let t = (((p[0] - u[0]) * dx + (p[1] - u[1]) * dy) / denominator).clamp(0., 1.);
            let q = std::array::from_fn(|i| u[i] + (v[i] - u[i]) * t);
            if height.is_some_and(|h| (q[2] - h).abs() > 2.) {
                continue;
            }
            let d = (p[0] - q[0]).hypot(p[1] - q[1]);
            if d <= 100. {
                candidates.push(Candidate {
                    a: *a,
                    b: *b,
                    position: q,
                    distance: d,
                    road: road.clone(),
                });
            }
        }
        let score = |c: &Candidate| c.distance.hypot(height.map_or(0., |h| c.position[2] - h));
        candidates.sort_by(|a, b| score(a).total_cmp(&score(b)));
        let best=candidates.first().map(score).context("No road nearby at this elevation; adjust the height filter or use surveyed free placement")?;
        let mut seen = std::collections::HashSet::new();
        candidates.retain(|c| score(c) <= best + 20. && seen.insert(c.road.clone()));
        candidates.truncate(96);
        Ok(candidates)
    }
    pub fn snap(&self, p: &Value, height: Option<f64>) -> Result<Value> {
        let nearest = self.nearest(position(p)?, height)?;
        Ok(
            json!({"position":point(nearest.position),"distance":nearest.distance,"road":nearest.road}),
        )
    }
    pub fn path(&self, start: &Value, end: &Value, height: Option<f64>) -> Result<Value> {
        let start_position = position(start)?;
        let mut starts = self.candidates(start_position, start["z"].as_f64().or(height))?;
        let closest = distance(starts[0].position, start_position);
        let corridor = self.corridors.get(&starts[0].road);
        starts.retain(|c| {
            distance(c.position, start_position) <= closest + 0.25
                || corridor.is_some_and(|group| self.corridors.get(&c.road) == Some(group))
                    && (c.position[2] - start_position[2]).abs() < 0.5
        });
        let heading_matches = |candidate: &Candidate, yaw: Option<f64>| {
            yaw.is_none_or(|yaw| {
                let a = self.points[candidate.a];
                let b = self.points[candidate.b];
                let r = yaw.to_radians();
                -(b[0] - a[0]) * r.sin() + (b[1] - a[1]) * r.cos() > 0.
            })
        };
        starts.retain(|c| heading_matches(c, start["yaw"].as_f64()));
        ensure!(
            !starts.is_empty(),
            "No forward road at the requested start heading"
        );
        let a = &starts[0];
        let end_position = position(end)?;
        let mut ends = self.candidates(end_position, end["z"].as_f64().or(height))?;
        let closest_end = distance(ends[0].position, end_position);
        let end_corridor = self.corridors.get(&ends[0].road);
        ends.retain(|c| {
            distance(c.position, end_position) <= closest_end + 0.25
                || end_corridor.is_some_and(|group| self.corridors.get(&c.road) == Some(group))
                    && (c.position[2] - end_position[2]).abs() < 0.5
        });
        ends.retain(|c| heading_matches(c, end["yaw"].as_f64()));
        ensure!(
            !ends.is_empty(),
            "No forward road at the requested end heading"
        );
        let b = &ends[0];
        let forward = (0..3)
            .map(|i| (b.position[i] - a.position[i]) * (self.points[a.b][i] - self.points[a.a][i]))
            .sum::<f64>()
            >= 0.;
        if (a.a, a.b) == (b.a, b.b) && (!self.directed || forward) {
            return Ok(json!({"points":[point(a.position),point(b.position)]}));
        }
        let mut costs = vec![f64::INFINITY; self.points.len()];
        let mut previous = vec![None; self.points.len()];
        let mut roots = vec![0usize; self.points.len()];
        let mut queue = BinaryHeap::new();
        for (index, a) in starts.iter().enumerate() {
            for node in [a.a, a.b] {
                if self.directed && node == a.a {
                    continue;
                }
                let cost = distance(a.position, self.points[node])
                    + distance(a.position, start_position) * 8.;
                if cost < costs[node] {
                    costs[node] = cost;
                    roots[node] = index;
                    queue.push(Step { node, cost });
                }
            }
        }
        let mut best = f64::INFINITY;
        let mut goal = None;
        while let Some(Step { node, cost }) = queue.pop() {
            if cost != costs[node] || cost > best {
                continue;
            }
            for (index, b) in ends
                .iter()
                .enumerate()
                .filter(|(_, b)| node == b.a || !self.directed && node == b.b)
            {
                // A click identifies a road corridor, rather than an arbitrary
                // overlapping lane. Prefer a reachable nearby lane and avoid
                // unnecessary elevation changes at bridges and junctions.
                let total = cost
                    + distance(self.points[node], b.position)
                    + b.distance * 8.
                    + (b.position[2] - a.position[2]).abs();
                if total < best {
                    best = total;
                    goal = Some((node, index));
                }
            }
            for &(other, d) in &self.links[node] {
                let next = cost + d;
                if next < costs[other] {
                    costs[other] = next;
                    previous[other] = Some(node);
                    roots[other] = roots[node];
                    queue.push(Step {
                        node: other,
                        cost: next,
                    });
                }
            }
        }
        let (node, end_index) = goal.context(
            "Roads are disconnected; choose another junction or use a surveyed free segment",
        )?;
        let b = &ends[end_index];
        let a = &starts[roots[node]];
        let mut nodes = vec![node];
        while let Some(other) = previous[*nodes.last().context("route node")?] {
            nodes.push(other);
        }
        nodes.reverse();
        let mut points = vec![start_position, a.position];
        points.extend(nodes.iter().map(|&n| self.points[n]));
        points.push(b.position);
        points.dedup_by(|a, b| distance(*a, *b) < 0.05);
        Ok(json!({"points":joins::path(points).into_iter().map(point).collect::<Vec<_>>()}))
    }
}

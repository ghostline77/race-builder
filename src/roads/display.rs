//! Height-separated road crossings prepared once with the road network.
use super::number;
use crate::route::position;
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Serialize)]
struct Point {
    x: f64,
    y: f64,
    z: f64,
}
#[derive(Clone, Serialize)]
struct Road {
    id: String,
    corridor: String,
    points: [Point; 0],
    bounds: [f64; 0],
}
#[derive(Clone, Serialize)]
struct Segment {
    a: Point,
    b: Point,
    road: Road,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Hit {
    x: f64,
    y: f64,
    upper: Segment,
    lower: Segment,
    upper_z: f64,
    lower_z: f64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Bridge {
    #[serde(flatten)]
    hit: Hit,
    upper_group: String,
    lower_group: String,
    hits: Vec<Hit>,
    direction: Direction,
    half_length: f64,
    half_width: f64,
}
#[derive(Serialize)]
struct Direction {
    x: f64,
    y: f64,
}
fn tangent(segment: &Segment) -> Direction {
    let x = segment.b.x - segment.a.x;
    let y = segment.b.y - segment.a.y;
    let length = x.hypot(y);
    Direction {
        x: x / length,
        y: y / length,
    }
}
fn crossing(a: &Segment, b: &Segment) -> Option<Hit> {
    let rx = a.b.x - a.a.x;
    let ry = a.b.y - a.a.y;
    let sx = b.b.x - b.a.x;
    let sy = b.b.y - b.a.y;
    let denominator = rx * sy - ry * sx;
    if denominator.abs() < 1e-8 {
        return None;
    }
    let dx = b.a.x - a.a.x;
    let dy = b.a.y - a.a.y;
    let t = (dx * sy - dy * sx) / denominator;
    let u = (dx * ry - dy * rx) / denominator;
    if !(0. ..=1.).contains(&t) || !(0. ..=1.).contains(&u) {
        return None;
    }
    let az = a.a.z + t * (a.b.z - a.a.z);
    let bz = b.a.z + u * (b.b.z - b.a.z);
    if (az - bz).abs() < 4. {
        return None;
    }
    Some(Hit {
        x: a.a.x + t * rx,
        y: a.a.y + t * ry,
        upper: if az > bz { a } else { b }.clone(),
        lower: if az > bz { b } else { a }.clone(),
        upper_z: az.max(bz),
        lower_z: az.min(bz),
    })
}

/// Attach reproducible display geometry; authored roads and routing stay intact.
pub(crate) fn prepare(network: &mut Value) -> Result<()> {
    if network["map_display"]["version"] == 1 {
        return Ok(());
    }
    let mut segments = Vec::new();
    for road in network["roads"].as_array().context("roads array")? {
        if road["drivable"] == false
            || road["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("connection_"))
        {
            continue;
        }
        let id = road["id"].as_str().context("road id")?;
        let descriptor = Road {
            id: id.into(),
            corridor: road["corridor"].as_str().unwrap_or(id).into(),
            points: [],
            bounds: [],
        };
        let points = road["points"]
            .as_array()
            .context("road points")?
            .iter()
            .map(|p| {
                let [x, y, z] = if let Some(a) = p.as_array() {
                    anyhow::ensure!(a.len() == 3, "road point requires xyz");
                    [number(&a[0])?, number(&a[1])?, number(&a[2])?]
                } else {
                    position(p)?
                };
                Ok(Point { x, y, z })
            })
            .collect::<Result<Vec<_>>>()?;
        for pair in points.windows(2) {
            if (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y) < 1e-5 {
                continue;
            }
            segments.push(Segment {
                a: pair[0],
                b: pair[1],
                road: descriptor.clone(),
            });
        }
    }
    let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    let mut bridges: Vec<Bridge> = Vec::new();
    let mut seen = vec![usize::MAX; segments.len()];
    for (index, a) in segments.iter().enumerate() {
        let dx = a.b.x - a.a.x;
        let dy = a.b.y - a.a.y;
        let steps = (dx.abs().max(dy.abs()) / 32.).ceil().max(1.) as usize;
        let mut visited = HashSet::new();
        let mut search = HashSet::new();
        for step in 0..=steps {
            let x = ((a.a.x + dx * step as f64 / steps as f64) / 32.).floor() as i64;
            let y = ((a.a.y + dy * step as f64 / steps as f64) / 32.).floor() as i64;
            visited.insert((x, y));
            for ox in -1..=1 {
                for oy in -1..=1 {
                    search.insert((x + ox, y + oy));
                }
            }
        }
        // Stable segment order makes grouping independent of hash iteration.
        let mut candidates = search
            .iter()
            .filter_map(|cell| cells.get(cell))
            .flatten()
            .copied()
            .filter(|&other| {
                if seen[other] == index {
                    false
                } else {
                    seen[other] = index;
                    true
                }
            })
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        for other in candidates {
            let b = &segments[other];
            if a.a.x.max(a.b.x) < b.a.x.min(b.b.x)
                || a.a.x.min(a.b.x) > b.a.x.max(b.b.x)
                || a.a.y.max(a.b.y) < b.a.y.min(b.b.y)
                || a.a.y.min(a.b.y) > b.a.y.max(b.b.y)
            {
                continue;
            }
            let Some(hit) = crossing(a, b) else {
                continue;
            };
            let direction = tangent(&hit.lower);
            let group = hit.upper.road.corridor.clone();
            let existing = groups
                .get(&group)
                .into_iter()
                .flatten()
                .copied()
                .find(|&i| {
                    let bridge = &bridges[i];
                    let other = tangent(&bridge.hit.lower);
                    (bridge.hit.upper_z - hit.upper_z).abs() < 2.
                        && (bridge.hit.lower_z - hit.lower_z).abs() < 3.
                        && (other.x * direction.x + other.y * direction.y).abs() > 0.85
                        && (bridge.hit.x - hit.x).hypot(bridge.hit.y - hit.y) < 45.
                });
            if let Some(i) = existing {
                bridges[i].hits.push(hit);
            } else {
                groups.entry(group.clone()).or_default().push(bridges.len());
                bridges.push(Bridge {
                    upper_group: group,
                    lower_group: hit.lower.road.corridor.clone(),
                    direction: tangent(&hit.upper),
                    half_length: 10.,
                    half_width: 3.,
                    hits: vec![hit.clone()],
                    hit,
                });
            }
        }
        for cell in visited {
            cells.entry(cell).or_default().push(index);
        }
    }
    for bridge in &mut bridges {
        let count = bridge.hits.len() as f64;
        bridge.hit.x = bridge.hits.iter().map(|h| h.x).sum::<f64>() / count;
        bridge.hit.y = bridge.hits.iter().map(|h| h.y).sum::<f64>() / count;
        bridge.hit.upper_z = bridge.hits.iter().map(|h| h.upper_z).sum::<f64>() / count;
        bridge.hit.lower_z = bridge.hits.iter().map(|h| h.lower_z).sum::<f64>() / count;
        for hit in &bridge.hits {
            let dx = hit.x - bridge.hit.x;
            let dy = hit.y - bridge.hit.y;
            bridge.half_length = bridge
                .half_length
                .max(10. + (dx * bridge.direction.x + dy * bridge.direction.y).abs());
            bridge.half_width = bridge
                .half_width
                .max(3. + (-dx * bridge.direction.y + dy * bridge.direction.x).abs());
        }
    }
    network["map_display"] = json!({"version":1,"bridges":bridges});
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interpolates_heights_groups_lanes_and_ignores_ground_junctions() {
        let mut network = json!({"roads":[
            {"id":"lower","points":[[-1000,-1000,-6],[1000,1000,-2]]},
            {"id":"upper1","corridor":"upper","points":[[-1000,1000,8],[1000,-1000,8]]},
            {"id":"upper2","corridor":"upper","points":[[-1000,1005,8],[1000,-995,8]]},
            {"id":"ground","points":[[-10,10,-4],[10,-10,-4]]}
        ]});
        prepare(&mut network).unwrap();
        let bridges = network["map_display"]["bridges"].as_array().unwrap();
        assert_eq!(bridges.len(), 1);
        assert_eq!(bridges[0]["hits"].as_array().unwrap().len(), 2);
        assert_eq!(bridges[0]["upperZ"], 8.);
        assert!((bridges[0]["lowerZ"].as_f64().unwrap() + 4.).abs() < 0.01);
        assert!(bridges[0]["halfWidth"].as_f64().unwrap() > 3.);
    }
}

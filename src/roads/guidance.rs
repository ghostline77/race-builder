//! Visible race guidance follows the middle of contiguous drivable lanes.
use super::*;
use std::collections::{BTreeMap, HashMap};

struct Segment {
    a: DVec3,
    b: DVec3,
    lane: usize,
    corridor: String,
    half_width: f64,
}

struct Lanes {
    segments: Vec<Segment>,
    cells: HashMap<(i32, i32), Vec<usize>>,
    corridors: HashMap<String, Vec<usize>>,
}

fn cell(p: DVec3) -> (i32, i32) {
    ((p.x / 32.).floor() as i32, (p.y / 32.).floor() as i32)
}

fn project(p: DVec3, segment: &Segment) -> DVec3 {
    let delta = segment.b - segment.a;
    segment.a + delta * ((p - segment.a).dot(delta) / delta.length_squared()).clamp(0., 1.)
}

impl Lanes {
    fn new(network: &Value) -> Result<Self> {
        let mut lanes = Self {
            segments: Vec::new(),
            cells: HashMap::new(),
            corridors: HashMap::new(),
        };
        for (lane, road) in network["roads"]
            .as_array()
            .context("Traffic roads")?
            .iter()
            .enumerate()
        {
            if road["drivable"] == false {
                continue;
            }
            let points = road["points"]
                .as_array()
                .context("Lane points")?
                .iter()
                .map(|p| {
                    if let Some(a) = p.as_array() {
                        ensure!(a.len() == 3, "Lane point needs three coordinates");
                        Ok(DVec3::new(
                            a[0].as_f64().context("Lane x")?,
                            a[1].as_f64().context("Lane y")?,
                            a[2].as_f64().context("Lane z")?,
                        ))
                    } else {
                        position(p).map(DVec3::from_array)
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            let corridor = road["corridor"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("lane_{lane}"));
            let polygon = road["polygon"].as_array();
            for (i, pair) in points.windows(2).enumerate() {
                if (pair[1] - pair[0]).length_squared() < 0.0001 {
                    continue;
                }
                let half_width = polygon
                    .filter(|p| p.len() == points.len() * 2)
                    .and_then(|p| {
                        let a = p[i].as_array()?;
                        let b = p[p.len() - 1 - i].as_array()?;
                        Some(
                            (a[0].as_f64()? - b[0].as_f64()?)
                                .hypot(a[1].as_f64()? - b[1].as_f64()?)
                                / 2.,
                        )
                    })
                    .unwrap_or(1.75)
                    .clamp(0.5, 7.);
                let index = lanes.segments.len();
                let lo = cell(pair[0].min(pair[1]));
                let hi = cell(pair[0].max(pair[1]));
                ensure!(
                    (i64::from(hi.0) - i64::from(lo.0) + 1)
                        * (i64::from(hi.1) - i64::from(lo.1) + 1)
                        < 10000,
                    "Excessive traffic lane extent"
                );
                for x in lo.0..=hi.0 {
                    for y in lo.1..=hi.1 {
                        lanes.cells.entry((x, y)).or_default().push(index);
                    }
                }
                lanes
                    .corridors
                    .entry(corridor.clone())
                    .or_default()
                    .push(index);
                lanes.segments.push(Segment {
                    a: pair[0],
                    b: pair[1],
                    lane,
                    corridor: corridor.clone(),
                    half_width,
                });
            }
        }
        Ok(lanes)
    }

    fn offset(&self, p: DVec3, forward: DVec3) -> f64 {
        let right = DVec3::new(forward.y, -forward.x, 0.);
        let (x, y) = cell(p);
        let mut nearest = None;
        let mut best = f64::INFINITY;
        for cx in x - 1..=x + 1 {
            for cy in y - 1..=y + 1 {
                for &index in self.cells.get(&(cx, cy)).into_iter().flatten() {
                    let s = &self.segments[index];
                    let q = project(p, s);
                    let direction = (s.b - s.a).with_z(0.).normalize_or_zero();
                    if (q.z - p.z).abs() > 1.25 || forward.dot(direction).abs() < 0.8 {
                        continue;
                    }
                    let distance = q.distance_squared(p);
                    if distance < best {
                        best = distance;
                        nearest = Some(index);
                    }
                }
            }
        }
        let Some(index) = nearest.filter(|_| best < 16.) else {
            return 0.;
        };
        let reference = &self.segments[index];
        let mut per_lane = BTreeMap::<usize, (f64, f64, f64)>::new();
        for &index in &self.corridors[&reference.corridor] {
            let s = &self.segments[index];
            let q = project(p, s);
            let direction = (s.b - s.a).with_z(0.).normalize_or_zero();
            if forward.dot(direction).abs() < 0.94
                || (q.z - p.z).abs() > 1.25
                || q.distance(p) > 32.
            {
                continue;
            }
            let distance = q.distance_squared(p);
            let side = (q - p).dot(right);
            let row = per_lane.entry(s.lane).or_insert((f64::INFINITY, 0., 0.));
            if distance < row.0 {
                *row = (distance, side - s.half_width, side + s.half_width);
            }
        }
        let Some(&(_, mut left, mut right_edge)) = per_lane.get(&reference.lane) else {
            return 0.;
        };
        // Do not bridge a median or jump to a neighbouring parallel carriageway.
        loop {
            let old = (left, right_edge);
            for &(_, a, b) in per_lane.values() {
                if a <= right_edge + 0.5 && b >= left - 0.5 {
                    left = left.min(a);
                    right_edge = right_edge.max(b);
                }
            }
            if old == (left, right_edge) {
                break;
            }
        }
        let offset = (left + right_edge) / 2.;
        if offset.abs() <= 12. { offset } else { 0. }
    }
}

/// Derive the visible line from native lane boundaries without moving the race spline.
pub fn centered(network: &Value, route: &Value) -> Result<Value> {
    let lanes = Lanes::new(network)?;
    let points = crate::route::group(route, "spline_point")
        .iter()
        .map(|p| position(&p["position"]).map(DVec3::from_array))
        .collect::<Result<Vec<_>>>()?;
    ensure!(points.len() >= 2, "Guidance requires a course");
    let forward = |i: usize| {
        let delta = if i + 1 < points.len() {
            points[i + 1] - points[i]
        } else {
            points[i] - points[i - 1]
        };
        delta.with_z(0.).normalize_or_zero()
    };
    let offsets = points
        .iter()
        .enumerate()
        .map(|(i, p)| lanes.offset(*p, forward(i)))
        .collect::<Vec<_>>();
    let stations = points.windows(2).fold(vec![0.], |mut s, p| {
        s.push(s.last().copied().unwrap_or(0.) + p[0].distance(p[1]));
        s
    });
    let centers = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mut total = 0.;
            let mut weights = 0.;
            for j in 0..points.len() {
                let distance = (stations[i] - stations[j]).abs();
                if distance < 10. && forward(i).dot(forward(j)) > 0.85 {
                    let weight = 1. - distance / 10.;
                    total += offsets[j] * weight;
                    weights += weight;
                }
            }
            let offset = total / weights;
            let f = forward(i);
            *p + DVec3::new(f.y, -f.x, 0.) * offset
        })
        .collect::<Vec<_>>();
    Ok(json!(
        centers
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let delta = if i + 1 < centers.len() {
                    centers[i + 1] - *p
                } else {
                    *p - centers[i - 1]
                };
                json!({"position":point(p.to_array()),"yaw":(-delta.x).atan2(delta.y).to_degrees()})
            })
            .collect::<Vec<_>>()
    ))
}

/// Native builds prepare road-centred guidance for the selected race projects.
pub fn prepare(
    projects: &[Value],
    game: Option<&Path>,
    schema: Option<&Path>,
) -> Result<Vec<Value>> {
    let projects = projects
        .iter()
        .map(|p| {
            let mut p = p.clone();
            p["route"] = super::joins::route(&p["route"])?;
            Ok(p)
        })
        .collect::<Result<Vec<_>>>()?;
    if !projects
        .iter()
        .any(|p| p["config"]["guidance_alignment"] == "road_center")
    {
        return super::surface::prepare(&projects, game, schema);
    }
    let network = extract_traffic(
        game.context("Road-centred guidance requires --game")?,
        schema,
        "base\\worlds\\03_night_city\\sectors\\_generated\\traffic",
    )?;
    let projects = projects
        .iter()
        .map(|project| {
            let mut project = project.clone();
            if project["config"]["guidance_alignment"] == "road_center" {
                let line = centered(&network, &project["route"])?;
                if !project["route"]["world"].is_object() {
                    project["route"]["world"] = json!({});
                }
                project["route"]["world"]["race_guidance"] = line;
            }
            Ok(project)
        })
        .collect::<Result<Vec<_>>>()?;
    super::surface::prepare(&projects, game, schema)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centers_contiguous_lanes_without_crossing_medians_or_road_levels() {
        let lane = |x: f64, z: f64| {
            json!({"corridor":"road","drivable":true,
            "points":[[x,0.,z],[x,100.,z]],
            "polygon":[[x-2.,0.],[x-2.,100.],[x+2.,100.],[x+2.,0.]]})
        };
        let route = json!({"marks":[
            {"role":"spline_point","order":0,"position":{"x":0.,"y":10.,"z":0.}},
            {"role":"spline_point","order":1,"position":{"x":0.,"y":50.,"z":0.}},
            {"role":"spline_point","order":2,"position":{"x":0.,"y":90.,"z":0.}}
        ]});
        for (x, z, expected) in [(4., 0., 2.), (8., 0., 0.), (4., 8., 0.)] {
            let network = json!({"roads":[lane(0.,0.),lane(x,z)]});
            let guidance = centered(&network, &route).unwrap();
            for p in guidance.as_array().unwrap() {
                assert!((p["position"]["x"].as_f64().unwrap() - expected).abs() < 0.001);
                assert_eq!(p["position"]["z"], 0.);
            }
            assert_eq!(route["marks"][0]["position"]["x"], 0.);
        }
    }
}

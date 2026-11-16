//! Gradual transitions across short lateral seams in generated traffic paths.
use crate::route::{group, point, position};
use anyhow::{Context, Result};
use glam::DVec3;
use serde_json::{Value, json};

/// Replace sideways lane connectors, preserving real turns and authored anchors.
fn transitions(points: &[DVec3], movable: &[bool]) -> Vec<(usize, usize, Vec<DVec3>)> {
    let mut stations = vec![0.];
    for pair in points.windows(2) {
        stations.push(stations.last().copied().unwrap_or(0.) + pair[0].distance(pair[1]));
    }
    let mut result = Vec::new();
    let mut consumed = 0;
    for i in 1..points.len().saturating_sub(2) {
        if i <= consumed {
            continue;
        }
        let before = (points[i] - points[i - 1]).normalize_or_zero();
        let seam = points[i + 1] - points[i];
        let after = (points[i + 2] - points[i + 1]).normalize_or_zero();
        if !(0.5..=8.).contains(&seam.length())
            || before.dot(after) < 0.95
            || seam.normalize_or_zero().dot(before).abs() > 0.3
            || seam.normalize_or_zero().dot(after).abs() > 0.3
            || seam.z.abs() > 0.5
        {
            continue;
        }
        let mut start = stations
            .partition_point(|s| *s < stations[i] - 15.)
            .saturating_sub(1);
        let mut end = stations
            .partition_point(|s| *s < stations[i + 1] + 15.)
            .min(points.len() - 1);
        // Keep authored anchors as transition endpoints rather than erasing
        // them or rejecting a seam merely because an anchor is nearby.
        if let Some(anchor) = (start + 1..=i).rfind(|j| !movable[*j]) {
            start = anchor;
        }
        if let Some(anchor) = (i + 1..end).find(|j| !movable[*j]) {
            end = anchor;
        }
        if (!result.is_empty() && start <= consumed)
            || end <= i + 1
            || !movable[start + 1..end].iter().all(|v| *v)
        {
            continue;
        }
        // A lane change must not consume an actual corner or cross a road level.
        if (start..end)
            .filter(|j| *j != i)
            .any(|j| (points[j + 1] - points[j]).normalize_or_zero().dot(before) < 0.85)
        {
            continue;
        }
        let a = points[start];
        let b = points[end];
        let length = a.distance(b);
        let incoming = if start == i {
            before
        } else {
            (points[start + 1] - a).normalize_or_zero()
        } * length;
        let outgoing = if end == i + 1 {
            after
        } else {
            (b - points[end - 1]).normalize_or_zero()
        } * length;
        let count = (length / 2.).ceil().max(2.) as usize;
        let curve = (1..count)
            .map(|n| {
                let t = n as f64 / count as f64;
                let t2 = t * t;
                let t3 = t2 * t;
                a * (2. * t3 - 3. * t2 + 1.)
                    + incoming * (t3 - 2. * t2 + t)
                    + b * (-2. * t3 + 3. * t2)
                    + outgoing * (t3 - t2)
            })
            .collect();
        result.push((start, end, curve));
        consumed = end;
    }
    result
}

pub(super) fn path(points: Vec<[f64; 3]>) -> Vec<[f64; 3]> {
    let native = points
        .iter()
        .copied()
        .map(DVec3::from_array)
        .collect::<Vec<_>>();
    let changes = transitions(&native, &vec![true; native.len()]);
    let mut output = Vec::new();
    let mut cursor = 0;
    for (start, end, curve) in changes {
        output.extend_from_slice(&points[cursor..=start]);
        output.extend(curve.into_iter().map(|p| p.to_array()));
        cursor = end;
    }
    output.extend_from_slice(&points[cursor..]);
    output
}

/// Bound native tracking spans after generated grid approaches have been added.
pub(crate) fn subdivide(points: &[[f64; 3]]) -> Vec<[f64; 3]> {
    let mut output = Vec::new();
    for pair in points.windows(2) {
        let a = DVec3::from_array(pair[0]);
        let b = DVec3::from_array(pair[1]);
        let count = ((a.distance(b) - 1e-7) / 10.).ceil().max(1.) as usize;
        for j in 0..count {
            output.push(a.lerp(b, j as f64 / count as f64).to_array());
        }
    }
    if let Some(last) = points.last() {
        output.push(*last);
    }
    output
}

/// Repair saved generated paths before guidance, checkpoints and native splines diverge.
pub(crate) fn route(route: &Value) -> Result<Value> {
    let marks = group(route, "spline_point");
    let points = marks
        .iter()
        .map(|m| position(&m["position"]).map(DVec3::from_array))
        .collect::<Result<Vec<_>>>()?;
    let movable = marks
        .iter()
        .map(|m| m["options"]["generated_road_sample"] == true)
        .collect::<Vec<_>>();
    let changes = transitions(&points, &movable);
    let mut rebuilt = Vec::new();
    let mut cursor = 0;
    for (start, end, curve) in changes {
        rebuilt.extend(marks[cursor..=start].iter().map(|m| (*m).clone()));
        for (j, p) in curve.into_iter().enumerate() {
            let mut mark = marks[start + 1].clone();
            mark["id"] = json!(format!(
                "{}_transition_{j}",
                marks[start + 1]["id"].as_str().context("route mark ID")?
            ));
            mark["position"] = point(p.to_array());
            rebuilt.push(mark);
        }
        cursor = end;
    }
    rebuilt.extend(marks[cursor..].iter().map(|m| (*m).clone()));
    // Native racing progression must not jump across a hundred-metre control
    // span. Subdivision changes neither the authored polyline nor its stations.
    let mut dense = Vec::new();
    for (i, mark) in rebuilt.iter().enumerate() {
        dense.push(mark.clone());
        if let Some(next) = rebuilt.get(i + 1) {
            let a = DVec3::from_array(position(&mark["position"])?);
            let b = DVec3::from_array(position(&next["position"])?);
            let count = ((a.distance(b) - 1e-7) / 10.).ceil().max(1.) as usize;
            for j in 1..count {
                let mut sample = mark.clone();
                sample["id"] = json!(format!(
                    "{}_span_{j}",
                    mark["id"].as_str().context("route mark ID")?
                ));
                sample["position"] = point(a.lerp(b, j as f64 / count as f64).to_array());
                sample["options"] = json!({"generated_road_sample":true});
                sample
                    .as_object_mut()
                    .context("route sample")?
                    .remove("native_ref");
                dense.push(sample);
            }
        }
    }
    for i in 0..dense.len() {
        dense[i]["order"] = json!(i);
        if i + 1 < dense.len() {
            let a = position(&dense[i]["position"])?;
            let b = position(&dense[i + 1]["position"])?;
            dense[i]["yaw"] = json!((-(b[0] - a[0])).atan2(b[1] - a[1]).to_degrees());
        }
    }
    let mut output = route.clone();
    let mut others = route["marks"]
        .as_array()
        .context("route marks")?
        .iter()
        .filter(|m| m["role"] != "spline_point")
        .cloned()
        .collect::<Vec<_>>();
    others.extend(dense);
    output["marks"] = json!(others);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lane_seams_become_gradual_without_rounding_real_corners() {
        let points = vec![
            [-30., 0., 0.],
            [-15., 0., 0.],
            [0., 0., 0.],
            [0., 5., 0.],
            [15., 5., 0.],
            [30., 5., 0.],
        ];
        let output = path(points.clone());
        assert!(output.len() > points.len());
        assert_eq!(output.first(), points.first());
        assert_eq!(output.last(), points.last());
        for p in output.windows(3) {
            let a = DVec3::from_array(p[1]) - DVec3::from_array(p[0]);
            let b = DVec3::from_array(p[2]) - DVec3::from_array(p[1]);
            assert!(a.normalize().dot(b.normalize()) > 0.9);
        }
        let corner = vec![[-30., 0., 0.], [0., 0., 0.], [0., 5., 0.], [0., 30., 0.]];
        assert_eq!(path(corner.clone()), corner);
        assert!(
            transitions(
                &points
                    .into_iter()
                    .map(DVec3::from_array)
                    .collect::<Vec<_>>(),
                &[true, true, false, false, true, true]
            )
            .is_empty()
        );
    }

    #[test]
    fn saved_routes_keep_anchors_and_are_stable_when_prepared_again() {
        let points = [
            [-30., 0., 0.],
            [-15., 0., 0.],
            [0., 0., 0.],
            [0., 5., 0.],
            [15., 5., 0.],
            [30., 5., 0.],
        ];
        let input = json!({"marks":points.iter().enumerate().map(|(i,p)|json!({"id":format!("p_{i}"),"role":"spline_point","order":i,"position":point(*p),"yaw":0,"options":{"generated_road_sample":i!=1}})).collect::<Vec<_>>()});
        let output = route(&input).unwrap();
        assert_eq!(route(&output).unwrap(), output);
        let marks = group(&output, "spline_point");
        assert!(
            marks
                .iter()
                .any(|m| m["id"] == "p_1" && m["position"] == point(points[1]))
        );
        for pair in marks.windows(2) {
            assert!(
                DVec3::from_array(position(&pair[0]["position"]).unwrap())
                    .distance(DVec3::from_array(position(&pair[1]["position"]).unwrap()))
                    <= 10.000001
            );
        }
    }
}

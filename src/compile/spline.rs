//! Fit generated racing paths with explicit, forward-facing native handles.
use anyhow::{Context, Result, ensure};
use glam::DVec3;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn vector(v: &Value) -> Result<DVec3> {
    let a = v.as_array().context("spline XYZ array")?;
    ensure!(a.len() == 3, "Spline point needs three coordinates");
    Ok(DVec3::new(
        crate::route::numeric(&a[0], "x")?,
        crate::route::numeric(&a[1], "y")?,
        crate::route::numeric(&a[2], "z")?,
    ))
}
fn nearest(stations: &[f64], value: f64) -> usize {
    stations
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - value).abs().total_cmp(&(b.1 - value).abs()))
        .map(|(i, _)| i)
        .unwrap_or(0)
}
fn handles(points: &[DVec3], looped: bool) -> (Vec<DVec3>, Vec<DVec3>) {
    let deltas = points.windows(2).map(|p| p[1] - p[0]).collect::<Vec<_>>();
    let units = deltas
        .iter()
        .map(|d| d.normalize_or_zero())
        .collect::<Vec<_>>();
    let mut directions = vec![units[0]];
    directions.extend(units.windows(2).map(|d| (d[0] + d[1]).normalize_or_zero()));
    directions.push(*units.last().unwrap());
    if looped {
        let d = (units[0] + units[units.len() - 1]).normalize_or_zero();
        directions[0] = d;
        *directions.last_mut().unwrap() = d;
    }
    let outgoing = deltas
        .iter()
        .enumerate()
        .map(|(i, d)| directions[i] * d.length() / 3.)
        .collect();
    let incoming = deltas
        .iter()
        .enumerate()
        .map(|(i, d)| -directions[i + 1] * d.length() / 3.)
        .collect();
    (outgoing, incoming)
}
fn evaluate(a: DVec3, b: DVec3, out: DVec3, inc: DVec3, t: f64) -> DVec3 {
    a * (1. - t).powi(3)
        + (a + out) * (3. * (1. - t).powi(2) * t)
        + (b + inc) * (3. * (1. - t) * t * t)
        + b * t.powi(3)
}
/// Preserve explicit handles; fit only compiler-generated linear control points.
pub(super) fn prepare(row: &mut Value) -> Result<()> {
    let raw = row["points"].as_array().context("race spline points")?;
    if raw.iter().any(|p| !p.is_array()) {
        return Ok(());
    }
    let looped = row["looped"] == true;
    let mut original = raw.iter().map(|p| vector(p)).collect::<Result<Vec<_>>>()?;
    original.dedup_by(|a, b| a.distance(*b) < 1e-6);
    ensure!(original.len() >= 2, "Race spline requires distinct points");
    if looped && original[0].distance(*original.last().unwrap()) > 1e-6 {
        original.push(original[0]);
    }
    original =
        crate::roads::joins::subdivide(&original.iter().map(|p| p.to_array()).collect::<Vec<_>>())
            .into_iter()
            .map(DVec3::from_array)
            .collect();
    let mut stations = vec![0.];
    for p in original.windows(2) {
        stations.push(stations.last().unwrap() + p[0].distance(p[1]));
    }
    let length = *stations.last().unwrap();
    let mut keep = BTreeSet::from([0, original.len() - 1]);
    let mut s = 48.;
    while s < length - 20. {
        keep.insert(nearest(&stations, s));
        s += 48.;
    }
    let (points, outgoing, incoming) = loop {
        let ids = keep.iter().copied().collect::<Vec<_>>();
        let points = ids.iter().map(|i| original[*i]).collect::<Vec<_>>();
        let (mut outgoing, mut incoming) = handles(&points, looped);
        let mut insert = Vec::new();
        for (i, pair) in ids.windows(2).enumerate() {
            let (a, b) = (pair[0], pair[1]);
            let mut samples = stations[a..=b].to_vec();
            let mut s = stations[a];
            while s < stations[b] {
                samples.push(s);
                s += 0.4;
            }
            let worst = loop {
                let mut worst = (0., stations[a]);
                for &s in &samples {
                    let j = stations
                        .partition_point(|v| *v <= s)
                        .saturating_sub(1)
                        .min(original.len() - 2);
                    let reference = original[j].lerp(
                        original[j + 1],
                        (s - stations[j]) / (stations[j + 1] - stations[j]),
                    );
                    let curve = evaluate(
                        points[i],
                        points[i + 1],
                        outgoing[i],
                        incoming[i],
                        (s - stations[a]) / (stations[b] - stations[a]),
                    );
                    let error = curve.distance(reference);
                    if error > worst.0 {
                        worst = (error, s);
                    }
                }
                if worst.0 <= 1.5 || b > a + 1 {
                    break worst;
                }
                // Shorten handles at tight joins while retaining their directions.
                outgoing[i] *= 0.5;
                incoming[i] *= 0.5;
            };
            if worst.0 > 1.5 {
                insert.push(nearest(&stations, worst.1).clamp(a + 1, b - 1));
            }
        }
        if insert.is_empty() {
            break (points, outgoing, incoming);
        }
        keep.extend(insert);
    };
    let mut curve_length = 0.;
    for i in 0..points.len() - 1 {
        let mut previous = points[i];
        for step in 1..=100 {
            let p = evaluate(
                points[i],
                points[i + 1],
                outgoing[i],
                incoming[i],
                step as f64 / 100.,
            );
            curve_length += p.distance(previous);
            previous = p;
        }
    }
    let count = points.len() - usize::from(looped);
    row["points"]=json!((0..count).map(|i| {
        let inc=if i>0 {incoming[i-1]} else if looped {*incoming.last().unwrap()} else {DVec3::ZERO};
        let out=outgoing.get(i).copied().unwrap_or(DVec3::ZERO);
        json!({"position":points[i].to_array(),"automatic_tangents":false,"tangents":[inc.to_array(),out.to_array()]})
    }).collect::<Vec<_>>());
    if row["width_sections"]
        .as_array()
        .is_none_or(|v| v.is_empty())
    {
        let width = row["width_default"].as_f64().unwrap_or(5.);
        row["width_sections"] =
            json!([{"from":0.,"to":curve_length.ceil(),"left":width,"right":width}]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unequal_spans_do_not_reverse_or_erase_turns() {
        let mut row = json!({"points":[[0.,0.,0.],[48.,0.,0.],[49.,0.,0.],[49.,40.,0.],[80.,40.,0.]],"width_default":7});
        prepare(&mut row).unwrap();
        let p = row["points"].as_array().unwrap();
        for pair in p.windows(2) {
            let v = |x: &Value| vector(x).unwrap();
            let a = v(&pair[0]["position"]);
            let b = v(&pair[1]["position"]);
            let out = v(&pair[0]["tangents"][1]);
            let inc = v(&pair[1]["tangents"][0]);
            let mut last = a;
            for n in 1..=100 {
                let next = evaluate(a, b, out, inc, n as f64 / 100.);
                assert!((next - last).dot(b - a) >= -1e-9);
                last = next;
            }
        }
        assert!(p.iter().all(|p| p["automatic_tangents"] == false));
    }
    #[test]
    fn explicit_handles_are_preserved() {
        let mut row = json!({"points":[{"position":[0,0,0],"tangents":[[0,0,0],[1,0,0]]},{"position":[3,0,0],"tangents":[[-1,0,0],[0,0,0]]}]});
        let original = row.clone();
        prepare(&mut row).unwrap();
        assert_eq!(row, original);
    }
}

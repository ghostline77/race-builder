//! Native deformed-road selection and placement validation.
use super::{number, unbox, vector};
use anyhow::{Context, Result, ensure};
use glam::{DQuat, DVec3};
use serde_json::{Value, json};

fn centreline(node: &Value, row: &Value) -> Result<(Vec<Value>, Vec<Value>)> {
    let frames = node["deformationData"]
        .as_array()
        .context("Road deformation frames")?;
    ensure!(
        frames.len() >= 2,
        "Road needs at least two deformation frames"
    );
    let origin = vector(&row["Position"])?;
    let scale = if row.get("Scale").is_some() {
        vector(&row["Scale"])?
    } else {
        DVec3::ONE
    };
    let orientation = &row["Orientation"];
    let q = DQuat::from_xyzw(
        number(&orientation["i"])?,
        number(&orientation["j"])?,
        number(&orientation["k"])?,
        number(&orientation["r"])?,
    );
    ensure!(
        q.length().is_finite() && q.length() > 1e-9 && scale.abs().min_element() > 1e-9,
        "Singular road transform"
    );
    let low = vector(&node["deformedBox"]["Min"])?;
    let high = vector(&node["deformedBox"]["Max"])?;
    let mut points: Vec<DVec3> = Vec::new();
    let mut tangents: Vec<DVec3> = Vec::new();
    for frame in frames {
        let p = vector(&frame["W"])?;
        let tangent = q.normalize() * (vector(&frame["Y"])? * scale);
        ensure!(
            (number(&frame["W"]["W"])? - 1.).abs() <= 1e-4,
            "Invalid homogeneous road station"
        );
        ensure!(
            p.cmpge(low - DVec3::splat(0.25)).all() && p.cmple(high + DVec3::splat(0.25)).all(),
            "Road station outside deformedBox"
        );
        ensure!(
            tangent.x.hypot(tangent.y) > 1e-6,
            "Road has no horizontal tangent"
        );
        let p = origin + q.normalize() * (p * scale);
        ensure!(
            points.last().is_none_or(|last| p.distance(*last) >= 1e-5),
            "Duplicate road stations"
        );
        points.push(p);
        tangents.push(tangent.normalize());
    }
    for (i, pair) in points.windows(2).enumerate() {
        let chord = (pair[1] - pair[0]).normalize();
        ensure!(
            chord.dot(tangents[i]).min(chord.dot(tangents[i + 1])) >= 0.2,
            "Road tangent disagrees with station order"
        );
    }
    let json_points = |points: Vec<DVec3>| {
        points
            .into_iter()
            .map(|p| json!({"x":p.x,"y":p.y,"z":p.z}))
            .collect()
    };
    Ok((json_points(points), json_points(tangents)))
}

/// Extracts validated native bended-road instances from a decoded sector.
pub fn sector_roads(document: &Value, depot: &str) -> Result<Value> {
    let root = &document["Data"]["RootChunk"];
    let nodes = root["nodes"].as_array().context("Sector nodes")?;
    let rows = root["nodeData"]["Data"]
        .as_array()
        .context("Sector placements")?;
    let mut roads = Vec::new();
    let mut failures = Vec::new();
    for (instance, row) in rows.iter().enumerate() {
        let index = unbox(&row["NodeIndex"])
            .as_u64()
            .context("Placement node index")?;
        let node = nodes
            .get(index as usize)
            .context("Placement references missing node")?;
        let node = node.get("Data").unwrap_or(node);
        let mesh = node["mesh"]["DepotPath"]["$value"]
            .as_str()
            .unwrap_or("")
            .replace('/', "\\")
            .to_ascii_lowercase();
        let road = unbox(&node["isBendedRoad"]);
        if node["$type"] != "worldBendedMeshNode"
            || !(road == true || road == 1)
            || !mesh.contains("road_meshes\\bended\\")
            || ["metro", "rail", "tram", "monorail"]
                .iter()
                .any(|word| mesh.contains(word))
        {
            continue;
        }
        match centreline(node,row){
            Ok((points,tangents))=>roads.push(json!({"id":format!("{depot}:{index}:{instance}"),"source":{"sector":depot,"node_index":index,"instance_index":instance,"mesh":mesh},"points":points,"native_tangents":tangents})),
            Err(error)=>failures.push(json!({"source":depot,"node_index":index,"instance_index":instance,"error":format!("{error:#}")})),
        }
    }
    Ok(json!({"roads":roads,"failures":failures}))
}

//! Shared, versioned authoring contract for the map, CET and Game Scout.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::HashSet;

pub const ROLES: &[&str] = &[
    "spline_point",
    "checkpoint",
    "start_grid",
    "player_start",
    "staging",
    "activation",
    "finish",
    "opponent_exit",
    "recovery",
    "reward",
    "spectator",
    "hazard",
    "boundary",
    "gps",
    "prop",
    "speed_change",
    "width_change",
];
pub fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .with_context(|| format!("{key} must be a string"))
}
pub fn identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
pub fn position(v: &Value) -> Result<[f64; 3]> {
    let p = [
        numeric(&v["x"], "x")?,
        numeric(&v["y"], "y")?,
        numeric(&v["z"], "z")?,
    ];
    Ok(p)
}
pub fn numeric(v: &Value, label: &str) -> Result<f64> {
    let n = v
        .as_f64()
        .with_context(|| format!("{label} must be a number"))?;
    ensure!(n.is_finite(), "{label} must be finite");
    Ok(n)
}
pub fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
pub fn point(p: [f64; 3]) -> Value {
    json!({"x":p[0],"y":p[1],"z":p[2]})
}

/// Accepts the common contract and both existing Game Scout export forms.
pub fn import(source: Value) -> Result<Value> {
    let mut route = if source["format"] == "ghostline-race-route" {
        source.clone()
    } else {
        let record = if source.get("race").is_some() {
            &source["race"]
        } else {
            &source
        };
        ensure!(
            record["data"].is_object(),
            "Import a shared route, Game Scout race-plan.json or saved race record"
        );
        let mut route = record["data"].clone();
        route["format"] = json!("ghostline-race-route");
        route["schema_version"] = json!(1);
        route["id"] = record["id"].clone();
        route["title"] = record["name"].clone();
        if route["prefix"].as_str().unwrap_or("").is_empty() {
            route["prefix"] = json!("races");
        }
        if let Some(world) = source.get("world_spec") {
            route["world"] = world.clone();
        }
        if let Some(marks) = route["marks"].as_array_mut() {
            for mark in marks {
                if let Some(binding) = source["bindings"]
                    .as_array()
                    .and_then(|a| a.iter().find(|b| b["mark_id"] == mark["id"]))
                {
                    if binding["ref"].is_string() {
                        mark["native_ref"] = binding["ref"].clone();
                    }
                    if binding["trigger_ref"].is_string() {
                        mark["native_trigger_ref"] = binding["trigger_ref"].clone();
                    }
                }
            }
        }
        route
    };
    // CET's JSON implementation can encode an empty Lua array as an object.
    if route["marks"].as_object().is_some_and(|m| m.is_empty()) {
        route["marks"] = json!([]);
    }
    validate(&route, false)?;
    Ok(route)
}

pub fn validate(route: &Value, complete: bool) -> Result<Value> {
    ensure!(
        route["format"] == "ghostline-race-route" && route["schema_version"] == 1,
        "Expected ghostline-race-route schema_version 1"
    );
    for key in ["id", "prefix"] {
        ensure!(
            identifier(text(route, key)?),
            "{key} must be a lowercase authoring identifier"
        );
    }
    ensure!(
        !text(route, "title")?.trim().is_empty(),
        "Route title is required"
    );
    ensure!(route["looped"].is_boolean(), "looped must be boolean");
    for key in ["width_default", "speed_mps"] {
        ensure!(numeric(&route[key], key)? >= 0.1, "{key} must be >= 0.1");
    }
    let marks = route["marks"]
        .as_array()
        .context("marks must be an array")?;
    let mut ids = HashSet::new();
    let mut orders = HashSet::new();
    for mark in marks {
        let id = text(mark, "id")?;
        let role = text(mark, "role")?;
        let order = mark["order"]
            .as_u64()
            .context("mark order must be a nonnegative integer")?;
        ensure!(
            identifier(id) && ids.insert(id),
            "Invalid or duplicate mark ID {id}"
        );
        ensure!(
            ROLES.contains(&role) && orders.insert((role, order)),
            "Unknown role or duplicate role order: {role} {order}"
        );
        position(&mark["position"])?;
        numeric(&mark["yaw"], "mark yaw")?;
        for key in [
            "width_m",
            "depth_m",
            "height_m",
            "left_m",
            "right_m",
            "speed_mps",
        ] {
            if let Some(v) = mark["options"].get(key) {
                ensure!(numeric(v, key)? > 0., "{key} must be positive");
            }
        }
    }
    let points = group(route, "spline_point");
    for pair in points.windows(2) {
        ensure!(
            distance(
                position(&pair[0]["position"])?,
                position(&pair[1]["position"])?
            ) >= 0.05,
            "Adjacent course points must be at least 0.05 metres apart"
        );
    }
    let mut missing = Vec::new();
    if points.len() < if route["looped"] == true { 3 } else { 2 } {
        missing.push("course points");
    }
    for role in [
        "checkpoint",
        "start_grid",
        "player_start",
        "staging",
        "activation",
        "finish",
    ] {
        if group(route, role).is_empty() {
            missing.push(role);
        }
    }
    for role in ["player_start", "staging", "activation", "finish"] {
        ensure!(
            group(route, role).len() <= 1,
            "Only one {role} is supported"
        );
    }
    ensure!(
        !complete || missing.is_empty(),
        "Missing route roles: {}",
        missing.join(", ")
    );
    Ok(json!({"complete":missing.is_empty(),"missing":missing,"marks":marks.len()}))
}
pub fn group<'a>(route: &'a Value, role: &str) -> Vec<&'a Value> {
    let mut rows = route["marks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["role"] == role)
        .collect::<Vec<_>>();
    rows.sort_by_key(|m| m["order"].as_u64().unwrap_or(0));
    rows
}
pub fn station(p: [f64; 3], points: &[[f64; 3]]) -> (f64, f64) {
    station_near(p, points, None)
}

/// Projects onto the course, using a prior station to distinguish overlapping passes.
/// The hint distinguishes passes within half a metre of the nearest projection,
/// allowing a road-centred gate to sit between slightly different lane paths.
pub fn station_near(p: [f64; 3], points: &[[f64; 3]], hint: Option<f64>) -> (f64, f64) {
    let mut best = (0., f64::INFINITY);
    let mut candidates = Vec::new();
    let mut traveled = 0.;
    for pair in points.windows(2) {
        let a = pair[0];
        let b = pair[1];
        let len = distance(a, b);
        if len < 1e-9 {
            continue;
        }
        let t = (((p[0] - a[0]) * (b[0] - a[0])
            + (p[1] - a[1]) * (b[1] - a[1])
            + (p[2] - a[2]) * (b[2] - a[2]))
            / len.powi(2))
        .clamp(0., 1.);
        let q = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
        let d = distance(p, q);
        let candidate = traveled + t * len;
        if hint.is_some() {
            candidates.push((candidate, d));
        }
        let nearer_pass = hint.is_some_and(|s| (candidate - s).abs() < (best.0 - s).abs());
        if d < best.1 - 0.0001 || ((d - best.1).abs() <= 0.0001 && nearer_pass) {
            best = (candidate, d);
        }
        traveled += len;
    }
    if let Some(hint) = hint {
        candidates
            .into_iter()
            .filter(|c| c.1 <= best.1 + 0.5)
            .min_by(|a, b| (a.0 - hint).abs().total_cmp(&(b.0 - hint).abs()))
            .unwrap_or(best)
    } else {
        best
    }
}

/// Generates world authoring and explicit bindings; native names survive import.
pub fn survey(route: &Value) -> Result<Value> {
    validate(route, true)?;
    let points = group(route, "spline_point")
        .iter()
        .map(|m| position(&m["position"]))
        .collect::<Result<Vec<_>>>()?;
    let mut course = points.clone();
    if route["looped"] == true {
        course.push(points[0]);
    }
    let length = course.windows(2).map(|p| distance(p[0], p[1])).sum::<f64>();
    let ns = format!("{}_{}", text(route, "prefix")?, text(route, "id")?);
    let root = format!(
        "mod\\{}\\races\\{}",
        text(route, "prefix")?,
        text(route, "id")?
    );
    let mut world = route
        .get("world")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or(json!({}));
    world["name"] = json!(ns);
    world["origin"] = point(points[0]);
    world["origin"]["yaw"] = json!(0);
    for (key, default) in [
        (
            "prefab_root",
            format!("$/{}/#{ns}_course", root.replace('\\', "/")),
        ),
        ("block_path", format!("{root}\\world\\all.streamingblock")),
        (
            "quest_sector_path",
            format!("{root}\\world\\course.streamingsector"),
        ),
    ] {
        if !world[key].is_string() {
            world[key] = json!(default);
        }
    }
    if world.get("streaming_box").is_none() {
        world["streaming_box"] = json!({"padding":100});
    }
    let spline = group(route, "spline_point")
        .iter()
        .find_map(|m| m["native_ref"].as_str())
        .or_else(|| world["race_splines"][0]["ref"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("#{ns}_route"));
    let mut speeds = vec![json!({"speed_mps":route["speed_mps"]})];
    let mut widths = Vec::new();
    for role in ["speed_change", "width_change"] {
        let mut marks = group(route, role)
            .iter()
            .map(|m| Ok((station(position(&m["position"])?, &course).0, *m)))
            .collect::<Result<Vec<_>>>()?;
        marks.sort_by(|a, b| a.0.total_cmp(&b.0));
        for pair in marks.windows(2) {
            ensure!(
                pair[1].0 - pair[0].0 >= 0.01,
                "{role} marks share a route station"
            );
        }
        for (i, (s, m)) in marks.iter().enumerate() {
            if role == "speed_change" {
                let speed = numeric(&m["options"]["speed_mps"], "speed change")?;
                if *s < 0.01 {
                    speeds[0]["speed_mps"] = json!(speed);
                } else {
                    speeds.push(json!({"start":s,"end":s,"speed_mps":speed}));
                }
            } else {
                let end = marks.get(i + 1).map_or(length, |m| m.0);
                ensure!(end > *s, "Width change must precede course end");
                widths.push(json!({"from":s,"to":end,"left":numeric(&m["options"]["left_m"],"left width")?,"right":numeric(&m["options"]["right_m"],"right width")?}));
            }
        }
    }
    world["race_splines"] = json!([{"ref":spline,"position":point(points[0]),"yaw":0,"points":points.iter().map(|p|std::array::from_fn::<_,3,_>(|i|p[i]-points[0][i])).collect::<Vec<_>>(),"looped":route["looped"],"width_default":route["width_default"],"speed_sections":speeds,"width_sections":widths}]);
    let marks = route["marks"].as_array().context("route marks")?;
    let old = marks
        .iter()
        .flat_map(|m| [m["native_ref"].as_str(), m["native_trigger_ref"].as_str()])
        .flatten()
        .collect::<HashSet<_>>();
    for key in ["markers", "triggers", "entities"] {
        let rows = world[key]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|r| !r["ref"].as_str().is_some_and(|s| old.contains(s)))
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        world[key] = json!(rows);
    }
    let mut bindings = Vec::new();
    for m in marks {
        let role = text(m, "role")?;
        let reference = m["native_ref"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("#{ns}_{role}_{}", m["id"].as_str().unwrap_or("mark")));
        let mut binding = json!({"mark_id":m["id"],"role":role,"order":m["order"],"ref":reference});
        if ["spline_point", "speed_change", "width_change"].contains(&role) {
            binding["ref"] = json!(spline);
            binding["station_m"] = json!(station(position(&m["position"])?, &course).0);
        } else {
            let mut marker = json!({"ref":reference,"position":m["position"],"yaw":m["yaw"]});
            if ["start_grid", "player_start", "recovery"].contains(&role) {
                marker["marker_type"] = json!("spawn_point");
            }
            world["markers"]
                .as_array_mut()
                .context("markers")?
                .push(marker.clone());
            if ["checkpoint", "staging", "finish", "hazard", "boundary"].contains(&role) {
                let trigger = m["native_trigger_ref"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or(format!("{reference}_trigger"));
                let mut v = marker.clone();
                v["ref"] = json!(trigger);
                v["outline"] = json!({"type":"box","width":m["options"]["width_m"].as_f64().unwrap_or(10.),"depth":m["options"]["depth_m"].as_f64().unwrap_or(2.),"height":m["options"]["height_m"].as_f64().unwrap_or(4.)});
                world["triggers"]
                    .as_array_mut()
                    .context("triggers")?
                    .push(v);
                binding["trigger_ref"] = json!(trigger);
            }
            if role == "prop" && m["options"]["entity_template"].is_string() {
                marker["ref"] = json!(format!("{reference}_entity"));
                marker["entity_template"] = m["options"]["entity_template"].clone();
                world["entities"]
                    .as_array_mut()
                    .context("entities")?
                    .push(marker);
            }
        }
        bindings.push(binding);
    }
    Ok(
        json!({"schema_version":1,"status":"geometry_complete","missing":[],"warnings":[],"world_spec":world,"bindings":bindings,"centreline_length_m":length,"route":route}),
    )
}

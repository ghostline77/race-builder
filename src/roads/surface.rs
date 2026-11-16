//! Native road-marker surface fitting, shared by individual and championship builds.
use super::glyph::Glyph;
use crate::route::position;
use anyhow::{Context, Result, ensure};
use ghostline_red::{
    material::ArchiveSet,
    schema::RedSchema,
    surface::{SurfaceHit, SurfaceQuery, SurfaceSurvey},
};
use glam::{DMat3, DQuat, DVec3};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufWriter, Write},
    path::Path,
};

const FORWARD: [f64; 9] = [-2.5, -1.875, -1.25, -0.625, 0., 0.625, 1.25, 1.875, 2.5];
const SIDE: [f64; 3] = [-1., 0., 1.];
const OFFSETS: [f64; 5] = [0., -0.75, 0.75, -1.5, 1.5];
const CLEARANCE: f64 = 0.08;
const SAMPLES: usize = FORWARD.len() * SIDE.len();

struct Marker {
    race: usize,
    reference: String,
    center: DVec3,
    forward: DVec3,
    right: DVec3,
    half_width: f64,
    forward_scale: f64,
}

struct Fit {
    position: DVec3,
    rotation: DQuat,
    maximum_clearance: f64,
    elevation: f64,
    length: f64,
    station_offset: f64,
}

#[derive(serde::Serialize)]
struct CacheKey<'a> {
    version: u32,
    archives: String,
    schema: String,
    queries: &'a [SurfaceQuery],
}

#[cfg(test)]
fn fit(m: &Marker, offset: f64, hits: &[Option<SurfaceHit>]) -> Option<Fit> {
    fit_span(m, offset, hits, 0, FORWARD.len() - 1)
}

fn fit_span(
    m: &Marker,
    offset: f64,
    hits: &[Option<SurfaceHit>],
    start: usize,
    end: usize,
) -> Option<Fit> {
    let hits = &hits[start * 3..(end + 1) * 3];
    if hits.iter().any(Option::is_none) {
        return None;
    }
    let heights = hits
        .iter()
        .flatten()
        .map(|h| h.position[2])
        .collect::<Vec<_>>();
    let mut along = 0.;
    let mut across = 0.;
    let forward = &FORWARD[start..=end];
    let middle = (FORWARD[start] + FORWARD[end]) / 2.;
    for (i, f) in forward.iter().enumerate() {
        for (j, r) in SIDE.iter().enumerate() {
            along += (f - middle) * m.forward_scale * heights[i * 3 + j];
            across += r * m.half_width * heights[i * 3 + j];
        }
    }
    along /= forward
        .iter()
        .map(|f| ((f - middle) * m.forward_scale).powi(2))
        .sum::<f64>()
        * 3.;
    across /= SIDE.iter().map(|r| (r * m.half_width).powi(2)).sum::<f64>() * forward.len() as f64;
    let elevation = forward
        .iter()
        .enumerate()
        .flat_map(|(i, f)| {
            SIDE.iter().enumerate().map(move |(j, r)| {
                (
                    i * 3 + j,
                    (*f - middle) * m.forward_scale,
                    *r * m.half_width,
                )
            })
        })
        .map(|(i, f, r)| heights[i] - along * f - across * r)
        .fold(f64::NEG_INFINITY, f64::max)
        + CLEARANCE;
    let maximum_clearance = forward
        .iter()
        .enumerate()
        .flat_map(|(i, f)| {
            SIDE.iter().enumerate().map(move |(j, r)| {
                (
                    i * 3 + j,
                    (*f - middle) * m.forward_scale,
                    *r * m.half_width,
                )
            })
        })
        .map(|(i, f, r)| elevation + along * f + across * r - heights[i])
        .fold(0f64, f64::max);
    if maximum_clearance > 0.35 {
        return None;
    }
    let normal = (-m.forward * along - m.right * across + DVec3::Z).normalize();
    let forward = (m.forward + DVec3::Z * along).normalize();
    let right = forward.cross(normal).normalize();
    let rotation = DQuat::from_mat3(&DMat3::from_cols(right, forward, right.cross(forward)));
    let center = m.center + m.right * offset + m.forward * (middle * m.forward_scale);
    Some(Fit {
        position: DVec3::new(center.x, center.y, elevation),
        rotation,
        maximum_clearance,
        elevation,
        length: (FORWARD[end] - FORWARD[start]) * m.forward_scale * (1. + along * along).sqrt(),
        station_offset: middle,
    })
}

fn fit_pieces(
    m: &Marker,
    offset: f64,
    hits: &[Option<SurfaceHit>],
    start: usize,
    end: usize,
) -> Option<Vec<Fit>> {
    if let Some(fit) = fit_span(m, offset, hits, start, end) {
        return Some(vec![fit]);
    }
    if end - start <= 2 {
        return None;
    }
    let middle = (start + end) / 2;
    let mut pieces = fit_pieces(m, offset, hits, start, middle)?;
    pieces.extend(fit_pieces(m, offset, hits, middle, end)?);
    Some(pieces)
}

/// Samples physical geometry at the actual generated tile footprints, after grid coverage.
pub(super) fn prepare(
    projects: &[Value],
    game: Option<&Path>,
    schema: Option<&Path>,
) -> Result<Vec<Value>> {
    let Some(game) = game else {
        return Ok(projects.to_vec());
    };
    let schema = RedSchema::load(Some(game), schema)?;
    let archives = ArchiveSet::open(&game.join("archive/pc"))?;
    let glyph = Glyph::load(&archives, &schema)?;
    let mut projects = projects.to_vec();
    let mut markers = Vec::new();
    let mut queries = Vec::new();
    for (race, project) in projects.iter_mut().enumerate() {
        if !project["route"]["world"].is_object() {
            project["route"]["world"] = json!({});
        }
        let world = project["route"]["world"]
            .as_object_mut()
            .context("route world")?;
        // Discard derived placements before planning footprints after route edits.
        for key in ["race_guidance_placements", "race_guidance_surface_audit"] {
            world.remove(key);
        }
        let (world, _) = crate::compile::marker_world(&project["config"], &project["route"])?;
        for entity in world["entities"].as_array().into_iter().flatten() {
            if entity["entity_template"]
                != r"base\quest\side_quests\sq024\entities\sq024_gps_path.ent"
            {
                continue;
            }
            let mut center = DVec3::from_array(position(&entity["position"])?);
            let yaw = entity["yaw"]
                .as_f64()
                .context("marker heading")?
                .to_radians();
            let forward = DVec3::new(-yaw.sin(), yaw.cos(), 0.);
            let right = DVec3::new(yaw.cos(), yaw.sin(), 0.);
            let orientation = &entity["node_data"]["orientation"];
            let rotation = DQuat::from_xyzw(
                orientation["i"].as_f64().unwrap_or(0.),
                orientation["j"].as_f64().unwrap_or(0.),
                orientation["k"].as_f64().unwrap_or(0.),
                orientation["r"].as_f64().unwrap_or(1.),
            );
            let tangent = rotation * DVec3::Y;
            let forward_scale = tangent.truncate().length().max(1e-6);
            let slope = tangent.z / forward_scale;
            center += rotation * DVec3::new(glyph.center.x, glyph.center.y, 0.);
            for offset in OFFSETS {
                for f in FORWARD {
                    for r in SIDE {
                        let mut p = center
                            + forward * (f * forward_scale)
                            + right * (r * glyph.half_width + offset);
                        p.z += slope * f * forward_scale;
                        queries.push(SurfaceQuery {
                            position: p.to_array(),
                            above: 2.,
                            below: 4.,
                        });
                    }
                }
            }
            markers.push(Marker {
                race,
                reference: entity["ref"].as_str().context("marker ref")?.into(),
                center,
                forward,
                right,
                half_width: glyph.half_width,
                forward_scale,
            });
        }
    }
    ensure!(
        !markers.is_empty(),
        "No road markers generated for surface sampling"
    );
    eprintln!(
        "Sampling native road surfaces for {} markers ({} footprint points)",
        markers.len(),
        queries.len()
    );
    let identity = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&CacheKey {
            version: 6,
            archives: archives.content_identity(),
            schema: format!("{:x}", Sha256::digest(serde_json::to_vec(&schema)?)),
            queries: &queries,
        })?)
    );
    let cache = std::env::temp_dir()
        .join("ghostline-race-builder")
        .join("surfaces");
    fs::create_dir_all(&cache)?;
    let cached = cache.join(format!("{identity}.json"));
    let previous = fs::read(&cached)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<SurfaceSurvey>(&bytes).ok())
        .filter(|v| v.hits.len() == queries.len());
    let survey: SurfaceSurvey = if let Some(previous) = previous {
        previous
    } else {
        let survey = ghostline_red::surface::sample(&archives, &schema, &queries)?;
        let mut temporary = tempfile::NamedTempFile::new_in(&cache)?;
        {
            let mut writer = BufWriter::with_capacity(1024 * 1024, temporary.as_file_mut());
            serde_json::to_writer(&mut writer, &survey)?;
            writer.flush()?;
        }
        temporary.persist(&cached).map_err(|e| e.error)?;
        survey
    };
    ensure!(
        survey.hits.len() == queries.len(),
        "Surface sampler returned a different query count"
    );
    let mut failures = Vec::new();
    let mut audits = vec![(0usize, 0f64, 0f64, 0usize, 0usize); projects.len()];
    for (index, m) in markers.iter().enumerate() {
        let selection = OFFSETS.iter().enumerate().find_map(|(candidate, offset)| {
            let start = (index * OFFSETS.len() + candidate) * SAMPLES;
            fit_pieces(
                m,
                *offset,
                &survey.hits[start..start + SAMPLES],
                0,
                FORWARD.len() - 1,
            )
            .map(|fit| (*offset, fit))
        });
        let Some((offset, pieces)) = selection else {
            failures.push(format!("{} {} at {:.2}, {:.2}, {:.2}: no fully sampled road tile fits within 1.5 m of the guidance line",projects[m.race]["route"]["id"],m.reference,m.center.x,m.center.y,m.center.z));
            continue;
        };
        let world = &mut projects[m.race]["route"]["world"];
        if !world["race_guidance_placements"].is_object() {
            world["race_guidance_placements"] = json!({});
        }
        world["race_guidance_placements"][&m.reference] = json!(
            pieces
                .iter()
                .map(|fit| {
                    let mut placed = glyph.placement(fit.position, fit.rotation, fit.length);
                    placed["station_offset_m"] = json!(fit.station_offset + glyph.center.y);
                    placed
                })
                .collect::<Vec<_>>()
        );
        let audit = &mut audits[m.race];
        audit.0 += pieces.len();
        audit.1 = pieces
            .iter()
            .map(|fit| fit.maximum_clearance)
            .fold(audit.1, f64::max);
        audit.2 = pieces
            .iter()
            .map(|fit| (fit.elevation - m.center.z).abs())
            .fold(audit.2, f64::max);
        audit.3 += usize::from(offset != 0.);
        audit.4 += usize::from(pieces.len() > 1);
    }
    let failure_report = cache.join(format!("{identity}.failures.json"));
    if !failures.is_empty() {
        fs::write(
            &failure_report,
            serde_json::to_vec_pretty(
                &json!({"format":"ghostline-road-surface-failures","identity":identity,"failures":failures,"diagnostics":survey.diagnostics}),
            )?,
        )?;
    }
    ensure!(
        failures.is_empty(),
        "Native road surface fitting failed for {} markers:\n{}\nFull report: {}\nNative decoder diagnostics: {}",
        failures.len(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n"),
        failure_report.display(),
        survey.diagnostics.join("; ")
    );
    for (project, (count, max_clearance, max_correction, shifted, split)) in
        projects.iter_mut().zip(audits)
    {
        project["route"]["world"]["race_guidance_surface_audit"] = json!({"source":"native-collision","identity":identity,"measured_tiles":count,"unmeasured_tiles":0,"samples_per_full_tile":SAMPLES,"shifted_tiles":shifted,"split_tiles":split,"clearance_m":CLEARANCE,"maximum_clearance_m":max_clearance,"maximum_height_correction_m":max_correction,"sectors":survey.sectors,"triangles":survey.triangles,"diagnostics":survey.diagnostics});
    }
    Ok(projects)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marker() -> Marker {
        Marker {
            race: 0,
            reference: "#marker".into(),
            center: DVec3::new(20., 30., 10.),
            forward: DVec3::Y,
            right: DVec3::X,
            half_width: 0.75,
            forward_scale: 1.,
        }
    }
    fn plane(m: &Marker) -> Vec<Option<SurfaceHit>> {
        FORWARD
            .iter()
            .flat_map(|f| {
                SIDE.iter().map(move |r| {
                    let r = r * m.half_width;
                    let p = m.center + m.forward * *f + m.right * r;
                    Some(SurfaceHit {
                        position: [p.x, p.y, 10. + 0.2 * *f + 0.1 * r],
                        normal: DVec3::new(-0.1, -0.2, 1.).normalize().to_array(),
                        sector: "fixture".into(),
                        overhead_m: None,
                    })
                })
            })
            .collect()
    }
    #[test]
    fn tilted_markers_keep_the_visible_mesh_above_the_road() {
        let m = marker();
        let fit = fit(&m, 0., &plane(&m)).unwrap();
        for f in [-2.5, 0., 2.5] {
            for r in [-0.5, 0., 0.5] {
                let p = fit.position + fit.rotation * DVec3::new(r, f, 0.);
                let road = 10. + 0.2 * (p.y - 30.) + 0.1 * (p.x - 20.);
                assert!((p.z - road - CLEARANCE).abs() < 1e-8);
            }
        }
    }
    #[test]
    fn missing_ground_is_rejected_and_overhead_does_not_move_the_road() {
        let m = marker();
        let mut hits = plane(&m);
        hits[0] = None;
        assert!(fit(&m, 0., &hits).is_none());
        hits = plane(&m);
        let baseline = fit(&m, 0., &hits).unwrap();
        hits[7].as_mut().unwrap().overhead_m = Some(0.9);
        assert_eq!(fit(&m, 0., &hits).unwrap().position, baseline.position);
        hits[7].as_mut().unwrap().overhead_m = Some(4.);
        assert!(fit(&m, 0., &hits).is_some());
    }

    #[test]
    fn short_segments_cover_a_sharp_crown_without_gaps() {
        let m = marker();
        let mut hits = plane(&m);
        for (i, f) in FORWARD.iter().enumerate() {
            for j in 0..3 {
                hits[i * 3 + j].as_mut().unwrap().position[2] = 10. + (1. - f.abs() / 1.25).max(0.);
            }
        }
        assert!(fit(&m, 0., &hits).is_none());
        let pieces = fit_pieces(&m, 0., &hits, 0, FORWARD.len() - 1).unwrap();
        assert!(pieces.len() > 1);
        let horizontal = pieces
            .iter()
            .map(|p| p.length * (p.rotation * DVec3::Y).truncate().length())
            .sum::<f64>();
        assert!((horizontal - 5.).abs() < 1e-8);
        for pair in pieces.windows(2) {
            let a = pair[0].position + pair[0].rotation * DVec3::Y * pair[0].length / 2.;
            let b = pair[1].position - pair[1].rotation * DVec3::Y * pair[1].length / 2.;
            assert!((a - b).truncate().length() < 1e-8);
        }
    }
}

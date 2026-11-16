//! Atomic editor storage for series manifests, separate from race projects.
use super::RaceSeries;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{fs, path::Path};
pub fn list(root: &Path) -> Result<Value> {
    let dir = root.join("series");
    let mut entries = Vec::new();
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().and_then(|v| v.to_str()) != Some("json") {
                continue;
            }
            let series: RaceSeries = serde_json::from_value(crate::compile::read_json(&path)?)?;
            entries.push(json!({"id":series.id,"title":series.title}));
        }
    }
    entries.sort_by(|a, b| a["title"].as_str().cmp(&b["title"].as_str()));
    Ok(json!(entries))
}
pub fn load(root: &Path, id: &str) -> Result<RaceSeries> {
    ensure!(crate::route::identifier(id), "Invalid series ID");
    let series: RaceSeries = serde_json::from_value(crate::compile::read_json(
        &root.join("series").join(format!("{id}.json")),
    )?)?;
    ensure!(series.id == id, "Series identity does not match its file");
    series.validate(false)?;
    Ok(series)
}
pub fn save(root: &Path, series: &RaceSeries) -> Result<()> {
    series.validate(false)?;
    let dir = root.join("series");
    fs::create_dir_all(&dir)?;
    let file = tempfile::NamedTempFile::new_in(&dir)?;
    crate::compile::write_json(file.path(), &serde_json::to_value(series)?)?;
    file.persist(dir.join(format!("{}.json", series.id)))
        .map_err(|e| anyhow::anyhow!("Save series: {e}"))?;
    Ok(())
}

//! Ordered race championships, persisted through native quest facts.
use anyhow::{Context, Result, ensure};
use ghostline_quest_builder::compiler::Build;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::Path};
pub(crate) mod store;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RaceSeries {
    pub format: String,
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub contact_name: String,
    pub intro_message: String,
    pub completion_message: String,
    pub reward_eddies: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quest_core_slot: Option<u16>,
    pub races: Vec<SeriesRace>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeriesRace {
    pub title: String,
    pub project: Option<String>,
    pub invitation: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Membership {
    pub id: String,
    pub race: String,
    pub position: usize,
}
impl Membership {
    pub fn unlock_fact(&self) -> String {
        format!("series_{}_{}_unlocked", self.id, self.race)
    }
    pub fn win_fact(&self) -> String {
        format!("series_{}_{}_won", self.id, self.race)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            crate::route::identifier(&self.id)
                && crate::route::identifier(&self.race)
                && self.position > 0,
            "Invalid race series membership"
        );
        Ok(())
    }
}
impl RaceSeries {
    /// Drafts retain empty slots; only complete series can be built.
    pub fn validate(&self, complete: bool) -> Result<()> {
        ensure!(
            self.format == "ghostline-race-series" && self.schema_version == 1,
            "Unsupported race series format"
        );
        ensure!(crate::route::identifier(&self.id), "Invalid series ID");
        if let Some(slot) = self.quest_core_slot {
            ghostline_quest_builder::quest_core::slot_resource(slot)?;
        }
        for (label, value) in [
            ("title", &self.title),
            ("contact name", &self.contact_name),
            ("intro message", &self.intro_message),
            ("completion message", &self.completion_message),
        ] {
            ensure!(!value.trim().is_empty(), "Series {label} is required");
        }
        ensure!(
            (1..=32).contains(&self.races.len()),
            "A series needs 1–32 races"
        );
        ensure!(
            self.reward_eddies > 0 && self.reward_eddies <= i32::MAX as u32,
            "Series reward must be a positive native integer"
        );
        let mut projects = HashSet::new();
        for (index, race) in self.races.iter().enumerate() {
            ensure!(
                !race.title.trim().is_empty(),
                "Race {} needs a display title",
                index + 1
            );
            if let Some(project) = &race.project {
                ensure!(
                    crate::route::identifier(project) && projects.insert(project),
                    "Race {} has an invalid or repeated project",
                    index + 1
                );
            } else {
                ensure!(
                    !complete,
                    "Choose a project for race {} ({})",
                    index + 1,
                    race.title
                );
            }
        }
        Ok(())
    }
    pub fn membership(&self, index: usize, race: &str) -> Membership {
        Membership {
            id: self.id.clone(),
            race: race.into(),
            position: index + 1,
        }
    }
}

/// Load saved authoring projects in series order, preserving complete routes.
pub fn load_projects(series: &RaceSeries, root: &Path) -> Result<Vec<Value>> {
    series.validate(true)?;
    series
        .races
        .iter()
        .enumerate()
        .map(|(index, race)| {
            let id = race.project.as_ref().context("race project")?;
            let project = crate::compile::read_json(&root.join(format!("{id}.json")))
                .with_context(|| format!("Race {}: {id}", index + 1))?;
            ensure!(
                project["route"]["id"] == *id,
                "Race {} project identity does not match {id}",
                index + 1
            );
            crate::compile::validate(&project["config"], &project["route"])
                .with_context(|| format!("Race {} ({})", index + 1, race.title))?;
            Ok(project)
        })
        .collect()
}

/// Generate every race and the single series controller into one native build.
pub fn documents(series: &RaceSeries, projects: &[Value]) -> Result<(Build, Value, Value)> {
    series.validate(true)?;
    ensure!(
        projects.len() == series.races.len(),
        "Series project count does not match its ordered races"
    );
    let mut build: Option<Build> = None;
    let mut registrations = Vec::new();
    let mut bindings = Vec::new();
    let mut worlds = Vec::new();
    let mut namespaces = HashSet::new();
    let mut workspot_offset = 0;
    for (index, project) in projects.iter().enumerate() {
        ensure!(
            project["route"]["id"].as_str() == series.races[index].project.as_deref(),
            "Series race {} refers to the wrong project",
            index + 1
        );
        let mut config = project["config"].clone();
        config["title"] = json!(series.races[index].title);
        let id = crate::route::text(&config, "id")?.to_owned();
        ensure!(
            namespaces.insert(id.clone()),
            "Duplicate race resource namespace {id}"
        );
        config["series"] = serde_json::to_value(series.membership(index, &id))?;
        if let Some(slot) = series.quest_core_slot {
            config["quest_core_slot"] = json!(slot);
            config["quest_core_workspot_offset"] = json!(workspot_offset);
        }
        let (mut race, race_bindings, world) =
            crate::compile::documents(&config, &project["route"])?;
        workspot_offset += race_bindings["reserved_workspot_count"].as_u64().unwrap_or(0);
        let registration = race
            .resources
            .remove(&format!("{id}.archive.xl"))
            .context("Race registration")?;
        registrations.push(serde_json::from_slice::<Value>(&registration)?);
        if let Some(all) = &mut build {
            for (path, value) in race.artifacts {
                ensure!(
                    all.artifacts.insert(path.clone(), value).is_none(),
                    "Duplicate resource {path}"
                );
            }
            for (path, value) in race.resources {
                ensure!(
                    all.resources.insert(path.clone(), value).is_none(),
                    "Duplicate loose resource {path}"
                );
            }
        } else {
            build = Some(race);
        }
        bindings.push(race_bindings);
        worlds.push(world);
    }
    let mut build = build.context("Series has no races")?;
    let root = format!("mod\\races\\series\\{}", series.id);
    let phase = format!("{root}\\phases\\{}_series.questphase", series.id);
    let (journal, onscreens) = crate::compile::series_resources(series, &root)?;
    let journal_path = journal.0.clone();
    let onscreens_path = onscreens.0.clone();
    build.artifacts.insert(journal.0, journal.1);
    build.artifacts.insert(onscreens.0, onscreens.1);
    build.artifacts.insert(
        phase.clone(),
        crate::compile::series_phase(series, &bindings, &phase)?,
    );
    let mut blocks = Vec::new();
    let mut journals = vec![json!(journal_path)];
    let mut texts = vec![json!(onscreens_path)];
    for registration in registrations {
        blocks.extend(
            registration["streaming"]["blocks"]
                .as_array()
                .context("blocks")?
                .iter()
                .cloned(),
        );
        journals.extend(
            registration["journal"]
                .as_array()
                .context("journals")?
                .iter()
                .cloned(),
        );
        texts.extend(
            registration["localization"]["onscreens"]["en-us"]
                .as_array()
                .context("localization")?
                .iter()
                .cloned(),
        );
    }
    let registration = json!({"quest":{"phases":[{"parent":"base\\quest\\cyberpunk2077.quest","path":phase}]},"streaming":{"blocks":blocks},"journal":journals,"localization":{"onscreens":{"en-us":texts}}});
    ensure!(
        build
            .resources
            .insert(
                format!("{}.archive.xl", series.id),
                serde_json::to_vec_pretty(&registration)?
            )
            .is_none(),
        "Series ID collides with a race package"
    );
    let bootstrap_phase = ghostline_quest_builder::bootstrap::Phase {
        resource: phase,
        input: "In1".into(),
        output: None,
    };
    let core_registration = if let Some(slot) = series.quest_core_slot {
        Some(ghostline_quest_builder::quest_core::attach(
            &mut build,
            &ghostline_quest_builder::quest_core::Allocation {
                slot,
                owner: series.id.clone(),
            },
            &bootstrap_phase,
        )?)
    } else {
        ghostline_quest_builder::bootstrap::attach(
            &mut build,
            ghostline_quest_builder::bootstrap::Spec::new(series.id.clone(), vec![bootstrap_phase]),
        )?;
        None
    };
    build.authoring_sha256 = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(
            &json!({"series":series,"projects":projects})
        )?)
    );
    crate::compile::audit_series(&build.artifacts, &bindings)?;
    Ok((
        build,
        json!({"series":series,"races":bindings,"quest_core_registration":core_registration}),
        json!({"races":worlds}),
    ))
}

/// Publish one archive/ZIP for the complete championship.
pub fn generate(
    series: &RaceSeries,
    projects: &[Value],
    out: &Path,
    schema: Option<&Path>,
    game: Option<&Path>,
) -> Result<Value> {
    ensure!(!out.exists(), "Build output must be a new directory");
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("series-compiler".into())
            .stack_size(64 * 1024 * 1024)
            .spawn_scoped(scope, || {
                let projects = crate::roads::guidance::prepare(projects, game, schema)?;
                let docs = documents(series, &projects)?;
                crate::compile::publish(
                    &serde_json::to_value(series)?,
                    &json!(projects),
                    out,
                    schema,
                    game,
                    docs,
                )
            })?
            .join()
            .map_err(|_| anyhow::anyhow!("Series compiler worker panicked"))?
    })
}

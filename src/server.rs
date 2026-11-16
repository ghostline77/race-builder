//! Local browser editor; all authoring and generation run in Rust.
use crate::{
    compile::{self, read_json, write_json},
    roads::RoadGraph,
    route,
};
use anyhow::{Context, Result, ensure};
use axum::{
    Json, Router,
    extract::{Path as UrlPath, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
struct App {
    root: PathBuf,
    token: String,
    port: u16,
    roads: Arc<RwLock<RoadGraph>>,
    vehicles: Value,
    jobs: Arc<Mutex<HashMap<String, Value>>>,
}
struct ApiError(anyhow::Error);
impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.into())
    }
}
impl From<anyhow::Error> for ApiError {
    fn from(value: anyhow::Error) -> Self {
        Self(value)
    }
}
impl From<std::io::Error> for ApiError {
    fn from(value: std::io::Error) -> Self {
        Self(value.into())
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":format!("{:#}",self.0)})),
        )
            .into_response()
    }
}
type Answer = std::result::Result<Json<Value>, ApiError>;
fn key() -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}:{}:{:?}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                std::thread::current().id()
            )
            .as_bytes()
        )
    )
}
fn authorize(app: &App, headers: &HeaderMap) -> Result<()> {
    ensure!(
        headers.get("x-race-token").and_then(|v| v.to_str().ok()) == Some(app.token.as_str()),
        "Reload the editor to renew its session"
    );
    if let Some(origin) = headers.get(header::ORIGIN) {
        let origin = origin.to_str()?;
        ensure!(
            origin == format!("http://127.0.0.1:{}", app.port)
                || origin == format!("http://localhost:{}", app.port),
            "Invalid request origin"
        );
    }
    Ok(())
}
fn projects(app: &App) -> Result<Value> {
    let mut rows = Vec::new();
    for entry in fs::read_dir(&app.root)? {
        let path = entry?.path();
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.extension().is_some_and(|s| s == "json") && !name.starts_with('.') && path.is_file()
        {
            let value = read_json(&path)?;
            rows.push(json!({"id":name,"title":value["config"]["title"].as_str().unwrap_or(name)}));
        }
    }
    rows.sort_by(|a, b| a["title"].as_str().cmp(&b["title"].as_str()));
    Ok(json!(rows))
}
fn settings(app: &App) -> Result<Value> {
    let path = app.root.join(".settings.json");
    if path.is_file() {
        read_json(&path)
    } else {
        Ok(json!({"schema":"","game":""}))
    }
}
async fn bootstrap(State(app): State<App>) -> Answer {
    let roads = app
        .roads
        .read()
        .map_err(|_| anyhow::anyhow!("road graph lock"))?;
    Ok(Json(
        json!({"token":app.token,"projects":projects(&app)?,"settings":settings(&app)?,"road_count":roads.network["roads"].as_array().map_or(0,Vec::len),"road_failures":roads.network["failures"],"roles":route::ROLES}),
    ))
}
async fn network(State(app): State<App>) -> Answer {
    Ok(Json(
        app.roads
            .read()
            .map_err(|_| anyhow::anyhow!("road graph lock"))?
            .network
            .clone(),
    ))
}
async fn replace_roads(state: State<App>, headers: HeaderMap, data: Json<Value>) -> Answer {
    action(state, UrlPath("roads".to_owned()), headers, data).await
}
async fn vehicles(State(app): State<App>) -> Json<Value> {
    Json(app.vehicles)
}
async fn project(State(app): State<App>, UrlPath(key): UrlPath<String>) -> Answer {
    ensure_key(&key)?;
    let mut project = read_json(&app.root.join(format!("{key}.json")))?;
    project["route"] = crate::roads::joins::route(&project["route"])?;
    Ok(Json(project))
}
async fn series_list(State(app): State<App>) -> Answer {
    Ok(Json(crate::series::store::list(&app.root)?))
}
async fn series_project(State(app): State<App>, UrlPath(id): UrlPath<String>) -> Answer {
    Ok(Json(serde_json::to_value(crate::series::store::load(
        &app.root, &id,
    )?)?))
}
async fn job(State(app): State<App>, UrlPath(key): UrlPath<String>) -> Answer {
    let value = app
        .jobs
        .lock()
        .map_err(|_| anyhow::anyhow!("job lock"))?
        .get(&key)
        .cloned()
        .context("Build job not found")?;
    Ok(Json(value))
}
async fn download(
    State(app): State<App>,
    UrlPath(key): UrlPath<String>,
) -> std::result::Result<Response, ApiError> {
    let job = app
        .jobs
        .lock()
        .map_err(|_| anyhow::anyhow!("job lock"))?
        .get(&key)
        .cloned()
        .context("Build job not found")?;
    if job["status"] != "complete" || job["native"] != true {
        return Err(anyhow::anyhow!("Native package is not ready").into());
    }
    let output = Path::new(job["output"].as_str().context("Build output")?);
    let series = output.join("series-build.json");
    let report = read_json(&if series.is_file() {
        series
    } else {
        output.join("race-build.json")
    })?;
    let id = route::text(&report, "id")?;
    ensure_key(id)?;
    let directory = if report.get("modio").is_some() {
        "modio"
    } else {
        "package"
    };
    let bytes = fs::read(output.join(directory).join(format!("{id}.zip")))?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{id}.zip\""),
            ),
        ],
        bytes,
    )
        .into_response())
}
fn ensure_key(key: &str) -> Result<()> {
    ensure!(route::identifier(key), "Invalid project ID");
    Ok(())
}
async fn action(
    State(app): State<App>,
    UrlPath(action): UrlPath<String>,
    headers: HeaderMap,
    Json(data): Json<Value>,
) -> Answer {
    authorize(&app, &headers)?;
    let response = match action.as_str() {
        "series-save" => {
            let series: crate::series::RaceSeries = serde_json::from_value(data)?;
            crate::series::store::save(&app.root, &series)?;
            json!({"saved":true})
        }
        "series-check" => {
            let series: crate::series::RaceSeries = serde_json::from_value(data)?;
            let projects = crate::series::load_projects(&series, &app.root)?;
            json!({"valid":true,"races":projects.len(),"reward_eddies":series.reward_eddies})
        }
        "series-build" => {
            let series: crate::series::RaceSeries = serde_json::from_value(data["series"].clone())?;
            let projects = crate::series::load_projects(&series, &app.root)?;
            let native = data["native"].as_bool().unwrap_or(true);
            let settings = settings(&app)?;
            let schema = settings["schema"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from);
            let game = settings["game"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from);
            let job = key();
            let out = app
                .root
                .join("builds")
                .join(format!("{}-{}", series.id, &job[..16]));
            app.jobs
                .lock()
                .map_err(|_| anyhow::anyhow!("job lock"))?
                .insert(job.clone(), json!({"status":"running","output":out}));
            let jobs = app.jobs.clone();
            let job_id = job.clone();
            tokio::task::spawn_blocking(move || {
                let result = crate::series::generate(
                    &series,
                    &projects,
                    &out,
                    if native { schema.as_deref() } else { None },
                    game.as_deref(),
                );
                if let Ok(mut jobs) = jobs.lock() {
                    if let Some(job) = jobs.get_mut(&job_id) {
                        match result {
                            Ok(report) => {
                                job["status"] = json!("complete");
                                job["resources"] = report["resource_count"].clone();
                                job["native"] = report["native"].clone();
                            }
                            Err(error) => {
                                job["status"] = json!("failed");
                                job["error"] = json!(format!("{error:#}"));
                            }
                        }
                    }
                }
            });
            json!({"job":job})
        }
        "import" => route::import(data)?,
        "check" => compile::validate(&data["config"], &data["route"])?,
        "save" => {
            route::validate(&data["route"], false)?;
            let id = route::text(&data["route"], "id")?;
            ensure_key(id)?;
            ensure_key(route::text(&data["config"], "id")?)?;
            let path = app.root.join(format!("{id}.json"));
            let temporary = tempfile::NamedTempFile::new_in(&app.root)?;
            write_json(temporary.path(), &data)?;
            temporary
                .persist(&path)
                .map_err(|e| anyhow::anyhow!("Save project: {e}"))?;
            json!({"saved":true,"projects":projects(&app)?})
        }
        "settings" => {
            let value = json!({"game":data["game"].as_str().unwrap_or(""),"schema":data["schema"].as_str().unwrap_or("")});
            write_json(&app.root.join(".settings.json"), &value)?;
            json!({"saved":true})
        }
        "snap" => app
            .roads
            .read()
            .map_err(|_| anyhow::anyhow!("road graph lock"))?
            .snap(&data["position"], data["height"].as_f64())?,
        "path" => {
            let roads = app.roads.clone();
            tokio::task::spawn_blocking(move || {
                roads
                    .read()
                    .map_err(|_| anyhow::anyhow!("road graph lock"))?
                    .path(&data["start"], &data["end"], data["height"].as_f64())
            })
            .await
            .map_err(|e| anyhow::anyhow!(e))??
        }
        "guidance" => {
            let roads = app.roads.clone();
            tokio::task::spawn_blocking(move || -> Result<Value> {
                let roads = roads.read().map_err(|_| anyhow::anyhow!("road graph lock"))?;
                let guidance = crate::roads::guidance::centered(&roads.network, &data["route"])?;
                Ok(json!({"points":guidance.as_array().context("guidance points")?.iter().map(|p|p["position"].clone()).collect::<Vec<_>>()}))
            }).await.map_err(|e|anyhow::anyhow!(e))??
        }
        "roads" => {
            let graph = RoadGraph::new(data)?;
            let count = graph.network["roads"].as_array().map_or(0, Vec::len);
            *app.roads
                .write()
                .map_err(|_| anyhow::anyhow!("road graph lock"))? = graph;
            json!({"roads":count})
        }
        "build" => {
            compile::validate(&data["config"], &data["route"])?;
            let native = data["native"].as_bool().unwrap_or(true);
            let settings = settings(&app)?;
            let schema = settings["schema"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from);
            let game = settings["game"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from);
            let job = key();
            let out = app.root.join("builds").join(format!(
                "{}-{}",
                route::text(&data["config"], "id")?,
                &job[..16]
            ));
            app.jobs
                .lock()
                .map_err(|_| anyhow::anyhow!("job lock"))?
                .insert(job.clone(), json!({"status":"running","output":out}));
            let jobs = app.jobs.clone();
            let job_id = job.clone();
            tokio::task::spawn_blocking(move || {
                let result = compile::generate(
                    &data["config"],
                    &data["route"],
                    &out,
                    if native { schema.as_deref() } else { None },
                    game.as_deref(),
                );
                if let Ok(mut jobs) = jobs.lock() {
                    if let Some(job) = jobs.get_mut(&job_id) {
                        match result {
                            Ok(report) => {
                                job["status"] = json!("complete");
                                job["resources"] = report["resource_count"].clone();
                                job["native"] = report["native"].clone();
                            }
                            Err(error) => {
                                job["status"] = json!("failed");
                                job["error"] = json!(format!("{error:#}"));
                            }
                        }
                    }
                }
            });
            json!({"job":job})
        }
        _ => return Err(anyhow::anyhow!("Unknown editor action").into()),
    };
    Ok(Json(response))
}
async fn index() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        include_str!("../ui/dist/index.html"),
    )
}
async fn javascript() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        include_str!("../ui/dist/app.js"),
    )
}
async fn css() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        include_str!("../ui/dist/style.css"),
    )
}
fn load_vehicles(path: &Path) -> Result<Value> {
    let index = if path.is_dir() {
        path.join("data/catalogue.json")
    } else {
        path.to_owned()
    };
    let data = read_json(&index)?;
    if let Some(chunks) = data["collections"]["vehicles"].as_array() {
        let root = index
            .parent()
            .context("Catalogue directory")?
            .parent()
            .context("Catalogue root")?;
        let mut records = Vec::new();
        for chunk in chunks {
            let relative = chunk.as_str().context("Vehicle chunk path")?;
            ensure!(
                !relative.contains("..") && !Path::new(relative).is_absolute(),
                "Invalid vehicle chunk path"
            );
            let rows = read_json(&root.join(relative))?;
            for vehicle in rows
                .as_array()
                .context("Vehicle catalogue chunk must be an array")?
            {
                if let Some(record) = vehicle["native_name"]
                    .as_str()
                    .filter(|s| s.starts_with("Vehicle."))
                {
                    records.push(
                        json!({"record":record,"name":vehicle["label"].as_str().unwrap_or(record)}),
                    );
                }
            }
        }
        records.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        return Ok(json!(records));
    }
    let records = if data.is_array() {
        data
    } else {
        data.get("vehicles")
            .or_else(|| data.get("entries"))
            .cloned()
            .context("Expected vehicle catalogue or record array")?
    };
    ensure!(records.is_array(), "Vehicle records must be an array");
    Ok(records)
}

pub async fn serve(
    root: &Path,
    port: u16,
    roads: Option<&Path>,
    vehicle_path: Option<&Path>,
) -> Result<()> {
    fs::create_dir_all(root)?;
    let root = root.canonicalize()?;
    let road_bytes = roads.map(fs::read).transpose()?;
    let cache_path = road_bytes.as_ref().map(|bytes| {
        root.join("road-display-cache")
            .join(format!("v1-{:x}.json", Sha256::digest(bytes)))
    });
    let mut network_data: Value = road_bytes
        .as_ref()
        .map(|bytes| serde_json::from_slice(bytes))
        .transpose()?
        .unwrap_or(
            json!({"format":"ghostline-road-network","schema_version":1,"roads":[],"failures":[]}),
        );
    if network_data["map_display"]["version"] != 1 {
        if let Some(path) = cache_path.as_ref().filter(|path| path.is_file()) {
            let display = read_json(path)?;
            ensure!(
                display["version"] == 1 && display["bridges"].is_array(),
                "Invalid cached road display"
            );
            network_data["map_display"] = display;
        }
    }
    let road_graph = RoadGraph::new(network_data)?;
    if let Some(path) = cache_path.as_ref().filter(|path| !path.is_file()) {
        write_json(path, &road_graph.network["map_display"])?;
    }
    let vehicle_data = vehicle_path
        .map(load_vehicles)
        .transpose()?
        .unwrap_or(json!([]));
    let app = App {
        root,
        token: key(),
        port,
        roads: Arc::new(RwLock::new(road_graph)),
        vehicles: vehicle_data,
        jobs: Default::default(),
    };
    let router = Router::new()
        .route("/", get(index))
        .route("/app.js", get(javascript))
        .route("/style.css", get(css))
        .route("/api/bootstrap", get(bootstrap))
        .route("/api/roads", get(network).post(replace_roads))
        .route("/api/vehicles", get(vehicles))
        .route("/api/project/{key}", get(project))
        .route("/api/series-list", get(series_list))
        .route("/api/series/{key}", get(series_project))
        .route("/api/job/{key}", get(job))
        .route("/api/job/{key}/download", get(download))
        .route("/api/{action}", post(action))
        .layer(axum::extract::DefaultBodyLimit::max(256 * 1024 * 1024))
        .with_state(app);
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    println!("Race Builder: http://127.0.0.1:{port}/");
    axum::serve(listener, router).await?;
    Ok(())
}

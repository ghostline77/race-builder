use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use ghostline_race_builder::{compile, roads, route, server};
use serde_json::json;
use std::path::PathBuf;
#[derive(Parser)]
#[command(version, about = "Native Cyberpunk 2077 race authoring")]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Build the ordered championship and all its saved race projects together.
    BuildSeries {
        spec: PathBuf,
        #[arg(long, default_value = "work/projects")]
        projects: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        schema: Option<PathBuf>,
        #[arg(long)]
        game: Option<PathBuf>,
    },
    ValidateSeries {
        spec: PathBuf,
        #[arg(long, default_value = "work/projects")]
        projects: PathBuf,
    },
    Validate {
        spec: PathBuf,
    },
    Build {
        spec: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        schema: Option<PathBuf>,
        #[arg(long)]
        game: Option<PathBuf>,
    },
    ImportRoute {
        source: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    ExportSurvey {
        route: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Roads {
        #[arg(long)]
        game: PathBuf,
        #[arg(long)]
        schema: Option<PathBuf>,
        #[arg(long)]
        sectors: Option<PathBuf>,
        #[arg(
            long,
            default_value = r"base\worlds\03_night_city\sectors\_generated\traffic"
        )]
        traffic_root: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// Route explicit point pairs through the native road network.
    RoadPaths {
        #[arg(long)]
        roads: PathBuf,
        #[arg(long)]
        requests: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Serve {
        #[arg(long, default_value = "work/projects")]
        workspace: PathBuf,
        #[arg(long, default_value_t = 8795)]
        port: u16,
        #[arg(long)]
        roads: Option<PathBuf>,
        #[arg(long)]
        vehicles: Option<PathBuf>,
    },
}
fn authoring(spec: &std::path::Path) -> Result<(serde_json::Value, serde_json::Value)> {
    let config = compile::read_json(spec)?;
    let file = config["route"]
        .as_str()
        .context("Race settings need a route file")?;
    let route = route::import(compile::read_json(
        &spec
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join(file),
    )?)?;
    Ok((config, route))
}
#[tokio::main]
async fn main() -> Result<()> {
    let result = match Args::parse().command {
        Command::BuildSeries {
            spec,
            projects,
            out,
            schema,
            game,
        } => {
            let series: ghostline_race_builder::series::RaceSeries =
                serde_json::from_value(compile::read_json(&spec)?)?;
            let projects = ghostline_race_builder::series::load_projects(&series, &projects)?;
            ghostline_race_builder::series::generate(
                &series,
                &projects,
                &out,
                schema.as_deref(),
                game.as_deref(),
            )?
        }
        Command::ValidateSeries { spec, projects } => {
            let series: ghostline_race_builder::series::RaceSeries =
                serde_json::from_value(compile::read_json(&spec)?)?;
            let projects = ghostline_race_builder::series::load_projects(&series, &projects)?;
            json!({"valid":true,"races":projects.len(),"reward_eddies":series.reward_eddies})
        }
        Command::Validate { spec } => {
            let (config, route) = authoring(&spec)?;
            compile::validate(&config, &route)?
        }
        Command::Build {
            spec,
            out,
            schema,
            game,
        } => {
            let (config, route) = authoring(&spec)?;
            compile::generate(&config, &route, &out, schema.as_deref(), game.as_deref())?
        }
        Command::ImportRoute { source, out } => {
            let route = route::import(compile::read_json(&source)?)?;
            compile::write_json(&out, &route)?;
            route::validate(&route, false)?
        }
        Command::ExportSurvey { route: input, out } => {
            let route = route::import(compile::read_json(&input)?)?;
            let survey = route::survey(&route)?;
            compile::write_json(&out, &survey)?;
            json!({"output":out,"complete":true})
        }
        Command::Roads {
            game,
            schema,
            sectors,
            traffic_root,
            out,
        } => {
            let mut network = if let Some(sectors) = sectors {
                roads::extract(
                    &json!({"game":game,"schema":schema,"sectors":compile::read_json(&sectors)?}),
                )?
            } else {
                roads::extract_traffic(&game, schema.as_deref(), &traffic_root)?
            };
            roads::prepare_display(&mut network)?;
            compile::write_json(&out, &network)?;
            json!({"roads":network["roads"].as_array().map_or(0,Vec::len),"connections":network["connections"].as_array().map_or(0,Vec::len),"coverage":network["coverage"],"failures":network["failures"]})
        }
        Command::RoadPaths {
            roads,
            requests,
            out,
        } => {
            let graph = roads::RoadGraph::new(compile::read_json(&roads)?)?;
            let requests = compile::read_json(&requests)?;
            let routes = requests
                .as_array()
                .context("road path requests must be an array")?
                .iter()
                .map(|request| graph.path(&request["start"], &request["end"], None))
                .collect::<Result<Vec<_>>>()?;
            compile::write_json(&out, &json!(routes))?;
            json!({"output":out,"paths":routes.len()})
        }
        Command::Serve {
            workspace,
            port,
            roads,
            vehicles,
        } => {
            server::serve(&workspace, port, roads.as_deref(), vehicles.as_deref()).await?;
            return Ok(());
        }
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

//! CLI for sigmaos-ports.

use clap::{Parser, Subcommand, ValueEnum};
use sigmaos_ports::{default_db_path, Port, PortState, PortsDb};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "sigmaos-ports", about = "SigmaOS ports tree manager")]
struct Cli {
    #[arg(long, global = true)]
    db: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, ValueEnum)]
enum StateArg {
    Available,
    Building,
    Installed,
    Failed,
}

impl From<StateArg> for PortState {
    fn from(s: StateArg) -> Self {
        match s {
            StateArg::Available => PortState::Available,
            StateArg::Building => PortState::Building,
            StateArg::Installed => PortState::Installed,
            StateArg::Failed => PortState::Failed,
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// List categories.
    Categories,
    /// List ports in a category.
    List { category: Option<String> },
    /// Search ports.
    Search { query: String },
    /// Show build plan for a port.
    Plan { name: String },
    /// Add a port from a JSON file.
    Add { file: PathBuf },
    /// Set port state.
    State { name: String, state: StateArg },
    /// List ports by state.
    ByState { state: StateArg },
    /// Print database path.
    Path,
}

fn db_path(db: &Option<PathBuf>) -> Result<PathBuf, Box<dyn std::error::Error>> {
    db.clone()
        .or_else(default_db_path)
        .ok_or_else(|| "cannot determine db path".into())
}

fn load(db: &Option<PathBuf>) -> Result<PortsDb, Box<dyn std::error::Error>> {
    let p = db_path(db)?;
    if p.exists() {
        Ok(PortsDb::load(&p)?)
    } else {
        let d = PortsDb::default();
        d.save(&p)?;
        Ok(d)
    }
}

fn save(db: &PortsDb, path: &Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    db.save(&db_path(path)?)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let cli = Cli::parse();
    let mut db = load(&cli.db)?;
    match cli.command {
        Commands::Categories => {
            for c in db.list_categories() {
                println!("{c}");
            }
        }
        Commands::List { category } => match category {
            Some(cat) => {
                if !db.categories.contains(&cat) {
                    return Err(format!("unknown category: {cat}").into());
                }
                for p in db.in_category(&cat) {
                    println!("{} {} ({:?})", p.name, p.version, p.state);
                }
            }
            None => {
                for p in db.ports.values() {
                    println!("{} {} {} ({:?})", p.category, p.name, p.version, p.state);
                }
            }
        },
        Commands::Search { query } => {
            for p in db.search(&query) {
                println!("{} {} — {}", p.name, p.version, p.description);
            }
        }
        Commands::Plan { name } => {
            for n in db.build_plan(&name)? {
                println!("{n}");
            }
        }
        Commands::Add { file } => {
            let port: Port = serde_json::from_str(&std::fs::read_to_string(file)?)?;
            db.add(port)?;
            save(&db, &cli.db)?;
        }
        Commands::State { name, state } => {
            db.set_state(&name, state.into())?;
            save(&db, &cli.db)?;
        }
        Commands::ByState { state } => {
            for p in db.by_state(state.into()) {
                println!("{} {} ({})", p.name, p.version, p.category);
            }
        }
        Commands::Path => println!("{}", db_path(&cli.db)?.display()),
    }
    Ok(())
}

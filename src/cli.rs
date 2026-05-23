use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// TODO: add English documentation
#[derive(Debug, Parser)]
#[command(
    name = "yaml-seeder",
    about = "Load and validate YAML-based seed data for MySQL-compatible databases.",
    author,
    version,
    propagate_version = true
)]
pub struct Cli {
    /// TODO: add English documentation
    #[arg(long, global = true)]
    pub debug: bool,

    /// TODO: add English documentation
    #[arg(long, global = true, env = "DATABASE_URL")]
    pub database_url: Option<String>,

    /// TODO: add English documentation
    #[command(subcommand)]
    pub command: Command,
}

/// TODO: add English documentation
#[derive(Debug, Subcommand)]
pub enum Command {
    /// TODO: add English documentation
    Create(CreateArgs),
    /// TODO: add English documentation
    Apply(ApplyArgs),
    /// TODO: add English documentation
    Export(ExportArgs),
}

/// TODO: add English documentation
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum Environment {
    /// TODO: add English documentation
    Dev,
    /// TODO: add English documentation
    Prod,
    /// TODO: add English documentation
    TidbPlayground,
}

impl Environment {
    /// TODO: add English documentation
    pub fn database_env_var(self) -> &'static str {
        match self {
            Self::Dev => "DEV_DATABASE_URL",
            Self::Prod => "PROD_DATABASE_URL",
            Self::TidbPlayground => "TIDB_PLAYGROUND_DATABASE_URL",
        }
    }

    /// TODO: add English documentation
    pub fn parse_arg(value: &str) -> Option<Self> {
        let lower = value.to_ascii_lowercase();
        match lower.as_str() {
            "dev" | "development" => Some(Self::Dev),
            "prod" | "production" => Some(Self::Prod),
            "tidb" | "tidb-playground" | "tidb_playground" => Some(Self::TidbPlayground),
            _ => None,
        }
    }
}

/// TODO: add English documentation
#[derive(Debug, Args)]
pub struct CreateArgs {
    /// TODO: add English documentation
    #[arg(value_name = "NAME")]
    pub name: String,

    /// TODO: add English documentation
    #[arg(long, value_name = "DIRECTORY", default_value = "scripts/seeds")]
    pub directory: PathBuf,

    /// TODO: add English documentation
    #[arg(long, value_name = "NUMBER")]
    pub number: Option<u32>,

    /// TODO: add English documentation
    #[arg(long, value_name = "WIDTH", default_value_t = 3)]
    pub width: usize,
}

/// TODO: add English documentation
#[derive(Debug, Args)]
pub struct ApplyArgs {
    /// TODO: add English documentation
    /// TODO: add English documentation
    #[arg(value_name = "ENV_OR_PATH", num_args = 1..=2)]
    pub env_and_path: Vec<String>,

    /// TODO: add English documentation
    #[arg(long)]
    pub dry_run: bool,

    /// TODO: add English documentation
    #[arg(long)]
    pub validate_only: bool,

    /// Skip tables that do not exist in the database schema
    /// instead of returning an error.
    #[arg(long)]
    pub ignore_missing_tables: bool,
}

/// TODO: add English documentation
#[derive(Debug, Args)]
pub struct ExportArgs {
    /// TODO: add English documentation
    #[arg(value_name = "YAML_FILE")]
    pub file: PathBuf,
    /// TODO: add English documentation
    #[arg(long = "env", value_enum)]
    pub env: Option<Environment>,

    /// TODO: add English documentation
    #[arg(long = "table", value_name = "SCHEMA.TABLE", required = true)]
    pub tables: Vec<String>,

    /// TODO: add English documentation
    #[arg(long)]
    pub pretty: bool,
}

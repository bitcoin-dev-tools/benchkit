use anyhow::{bail, Result};
use benchkit::{benchmarks, database};
use clap::{Parser, Subcommand};
use std::io::{self};

#[derive(Parser, Debug)]
#[command(
    version,
    about,
    long_about = "Run benchmarks using hyperfine from a YAML config"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, env = "PGHOST", default_value = "localhost")]
    pg_host: String,

    #[arg(long, env = "PGPORT", default_value = "5432")]
    pg_port: u16,

    #[arg(long, env = "PGDATABASE", default_value = "benchmarks")]
    pg_database: String,

    #[arg(long, env = "PGUSER", default_value = "benchkit")]
    pg_user: String,

    #[arg(long, env = "PGPASSWORD", default_value = "benchcoin")]
    pg_password: String,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Database administration
    Db {
        #[command(subcommand)]
        command: DbCommands,
    },
    /// Build Bitcoin Core
    Build,
    /// Run benchmarks
    Run {
        #[command(subcommand)]
        command: RunCommands,
    },
}

#[derive(Subcommand, Debug)]
enum DbCommands {
    /// Initialise database if not exists
    Init,
    /// Test connection to postgres backend
    Test,
    /// [WARNING] Drop database and user
    Delete,
}

#[derive(Subcommand, Debug)]
enum RunCommands {
    /// Run all benchmarks found in config yml
    All {
        #[arg(short, long, default_value = "benchmark.yml")]
        config: String,

        #[arg(long)]
        pr_number: Option<i32>,

        #[arg(long)]
        run_id: Option<i32>,
    },
    /// Run a single benchmark from config yml
    Single {
        #[arg(short, long, default_value = "benchmark.yml")]
        config: String,

        #[arg(short, long)]
        name: String,

        #[arg(long)]
        pr_number: Option<i32>,

        #[arg(long)]
        run_id: Option<i32>,
    },
}

impl From<&Cli> for database::DatabaseConfig {
    fn from(cli: &Cli) -> Self {
        database::DatabaseConfig {
            host: cli.pg_host.clone(),
            port: cli.pg_port,
            database: cli.pg_database.clone(),
            user: cli.pg_user.clone(),
            password: cli.pg_password.clone(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let db_config = database::DatabaseConfig::from(&cli);

    match &cli.command {
        Commands::Db { command } => match command {
            DbCommands::Init => {
                println!("Initializing database...");
                database::initialize_database(&db_config).await?;
            }
            DbCommands::Test => {
                database::check_connection(&db_config.connection_string()).await?;
                println!("Successfully connected to database");
            }
            DbCommands::Delete => {
                println!("⚠️  WARNING: You are about to delete:");
                println!("  Database: {}", db_config.database);
                println!("  User: {}", db_config.user);
                println!("  Host: {}:{}", db_config.host, db_config.port);
                println!("\nAre you sure? Type 'yes' to confirm: ");

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                if input.trim().to_lowercase() == "yes" {
                    println!("Deleting database...");
                    database::delete_database(&db_config).await?;
                    println!("Database and user deleted successfully.");
                } else {
                    bail!("Database deletion cancelled.");
                }
            }
        },
        Commands::Build => {
            println!("Building project... Not implemented yet.");
        }
        Commands::Run { command } => match command {
            RunCommands::All {
                config,
                pr_number,
                run_id,
            } => {
                database::check_connection(&db_config.connection_string()).await?;
                let runner = benchmarks::Runner::new(
                    config,
                    &db_config.connection_string(),
                    *pr_number,
                    *run_id,
                )?;
                runner.run().await?;
                println!("All benchmarks completed successfully.");
            }
            #[allow(unused_variables)]
            RunCommands::Single {
                config,
                name,
                pr_number,
                run_id,
            } => {
                unimplemented!("Single benchmarks not yet supported")
            }
        },
    }

    Ok(())
}

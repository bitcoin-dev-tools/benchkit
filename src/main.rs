use anyhow::{bail, Result};
use benchkit::{
    benchmarks::{self, load_bench_config, BenchmarkConfig},
    config::{load_app_config, AppConfig},
    database::{self, DatabaseConfig},
    system::SystemChecker,
};
use clap::{Parser, Subcommand};
use std::{
    io::{self},
    path::PathBuf,
    process,
};

const DEFAULT_CONFIG: &str = "config.yml";
const DEFAULT_BENCH_CONFIG: &str = "benchmark.yml";

#[derive(Parser, Debug)]
#[command(
    version,
    about,
    long_about = "Run benchmarks using hyperfine from a YAML config"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Application config
    #[arg(short, long, default_value = DEFAULT_CONFIG)]
    app_config: PathBuf,

    /// Benchmark config
    #[arg(short, long, default_value = DEFAULT_BENCH_CONFIG)]
    bench_config: PathBuf,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Database administration
    Db {
        #[command(subcommand)]
        command: DbCommands,
    },
    /// Build bitcoin core binaries using guix
    Build {},
    /// Run benchmarks
    Run {
        #[command(subcommand)]
        command: RunCommands,
    },
    /// Check system performance settings
    System {
        #[command(subcommand)]
        command: SystemCommands,
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
        #[arg(long)]
        pr_number: Option<i32>,

        #[arg(long)]
        run_id: Option<i32>,
    },
    /// Run a single benchmark from config yml
    Single {
        #[arg(short, long)]
        name: String,

        #[arg(long)]
        pr_number: Option<i32>,

        #[arg(long)]
        run_id: Option<i32>,
    },
}

#[derive(Subcommand, Debug)]
enum SystemCommands {
    /// Check current system configuration
    Check,
    /// Tune the system for benchmarking
    Tune,
    /// Reset benchmarking tune
    Reset,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Run system commands without loading any configuration
    if let Commands::System { command } = &cli.command {
        let checker = SystemChecker::new()?;
        match command {
            SystemCommands::Check => checker.run_checks()?,
            SystemCommands::Tune => checker.tune()?,
            SystemCommands::Reset => checker.reset()?,
        }
        process::exit(0);
    }

    let app_config: AppConfig = load_app_config(&cli.app_config)?;
    let bench_config: BenchmarkConfig = load_bench_config(&cli.bench_config)?;
    let db_config: &DatabaseConfig = &app_config.database;

    match &cli.command {
        Commands::Db { command } => match command {
            DbCommands::Init => {
                println!("Initializing database...");
                database::initialize_database(db_config).await?;
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
                    database::delete_database(db_config).await?;
                    println!("Database and user deleted successfully.");
                } else {
                    bail!("Database deletion cancelled.");
                }
            }
        },
        Commands::Build {} => {
            let builder = benchmarks::Builder::new(&app_config, &bench_config)?;
            builder.build()?;
        }
        Commands::Run { command } => {
            database::check_connection(&db_config.connection_string()).await?;
            // First we will build the binaries
            // TODO: is there a way we can check the binaries_dir first to avoid rebuilding the
            // same commit binary twice?
            let builder = benchmarks::Builder::new(&app_config, &bench_config)?;
            builder.build()?;
            match command {
                RunCommands::All { pr_number, run_id } => {
                    let runner = benchmarks::Runner::new(
                        &bench_config,
                        &db_config.connection_string(),
                        *pr_number,
                        *run_id,
                    )?;
                    runner.run().await?;
                    println!("All benchmarks completed successfully.");
                }
                RunCommands::Single {
                    name,
                    pr_number,
                    run_id,
                } => {
                    let runner = benchmarks::Runner::new(
                        &bench_config,
                        &db_config.connection_string(),
                        *pr_number,
                        *run_id,
                    )?;
                    runner.run_single(name).await?;
                    println!("Benchmark completed successfully.");
                }
            }
        }
        _ => {}
    }

    Ok(())
}

use anyhow::{Context, Result};
use benchkit::{benchmarks, database};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "benchkit")]
#[command(
    version = "0.1.0",
    about = "Run benchmarks using hyperfine from a YAML config"
)]
struct Cli {
    #[arg(short, long, default_value = "benchmark.yml")]
    config: String,

    #[arg(long)]
    pr_number: Option<i32>,

    #[arg(long)]
    run_id: Option<i32>,

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

    #[arg(long)]
    init_db: bool,
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

    if cli.init_db {
        println!("Initializing database...");
        database::initialize_database(&db_config).await?;
        return Ok(());
    }

    database::check_connection(&db_config.connection_string())
        .await
        .with_context(|| "Failed to connect to database. If database does not exist, run with --init-db flag first")?;
    println!("Successfully connected to database");

    let runner = benchmarks::Runner::new(
        &cli.config,
        &db_config.connection_string(),
        cli.pr_number,
        cli.run_id,
    )?;

    runner.run().await?;

    println!("All benchmarks completed successfully.");
    Ok(())
}

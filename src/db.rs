use anyhow::{Context, Result};
use serde::Deserialize;
use std::time::Duration;
use tokio::time::timeout;
use tokio_postgres::{Client, NoTls};

#[derive(Deserialize, Debug)]
struct BenchmarkResult {
    command: String,
    mean: f64,
    stddev: f64,
    median: f64,
    user: f64,
    system: f64,
    min: f64,
    max: f64,
    times: Vec<f64>,
    exit_codes: Vec<i32>,
}

#[derive(Deserialize)]
struct Results {
    results: Vec<BenchmarkResult>,
}

pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub password: String,
}

impl DatabaseConfig {
    pub fn connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.user, self.password, self.host, self.port, self.database
        )
    }
}

pub async fn initialize_database(config: &DatabaseConfig) -> Result<()> {
    let user_exists = check_postgres_user(&config.user)
        .with_context(|| format!("Failed to check if user {} exists", config.user))?;
    let db_exists = check_postgres_database(&config.database)
        .with_context(|| format!("Failed to check if database {} exists", config.database))?;

    if !user_exists {
        create_postgres_user(&config.user, &config.password)
            .with_context(|| format!("Failed to create user {}", config.user))?;
    }

    if !db_exists {
        create_postgres_database(&config.database, &config.user)
            .with_context(|| format!("Failed to create database {}", config.database))?;
        grant_privileges(&config.database, &config.user).with_context(|| {
            format!(
                "Failed to grant privileges on {} to {}",
                config.database, config.user
            )
        })?;
    }

    let (client, connection) = tokio_postgres::connect(&config.connection_string(), NoTls)
        .await
        .with_context(|| "Failed to connect to database")?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    init_schema(&client).await?;
    Ok(())
}

async fn init_schema(client: &Client) -> Result<()> {
    client
        .batch_execute(include_str!("../src/database/schema.sql"))
        .await?;

    Ok(())
}

pub async fn check_connection(conn_string: &str) -> Result<()> {
    let (client, connection) = tokio_postgres::connect(conn_string, NoTls)
        .await
        .with_context(|| "Failed to establish database connection")?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    client
        .execute("SELECT 1", &[])
        .await
        .with_context(|| "Failed to execute test query")?;
    Ok(())
}

pub async fn store_results_in_db(
    db_url: &str,
    bench_name: &str,
    result_json: &str,
    pull_request_number: Option<i32>,
    run_id: Option<i32>,
) -> Result<()> {
    let (client, connection) = timeout(
        Duration::from_secs(5),
        tokio_postgres::connect(db_url, NoTls),
    )
    .await
    .with_context(|| "Database connection timeout")?
    .with_context(|| "Failed to connect to database")?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    let results: Results = serde_json::from_str(result_json)
        .with_context(|| "Failed to parse benchmark results JSON")?;

    for result in &results.results {
        let benchmark_id: i32 = timeout(
            Duration::from_secs(5),
            client.query_one(
                "INSERT INTO benchmarks (name, command, pull_request_number, run_id) 
                VALUES ($1, $2, $3, $4) RETURNING id",
                &[&bench_name, &result.command, &pull_request_number, &run_id],
            ),
        )
        .await
        .with_context(|| "Timeout inserting benchmark")?
        .with_context(|| "Failed to insert benchmark")?
        .get(0);

        let run_id: i32 = timeout(
            Duration::from_secs(5),
            client.query_one(
                "INSERT INTO benchmark_runs (
                    benchmark_id, mean, stddev, median, user_time,
                    system_time, min_time, max_time
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
                &[
                    &benchmark_id,
                    &result.mean,
                    &result.stddev,
                    &result.median,
                    &result.user,
                    &result.system,
                    &result.min,
                    &result.max,
                ],
            ),
        )
        .await
        .with_context(|| "Timeout inserting benchmark run")?
        .with_context(|| "Failed to insert benchmark run")?
        .get(0);

        for (idx, (time, exit_code)) in result
            .times
            .iter()
            .zip(result.exit_codes.iter())
            .enumerate()
        {
            timeout(
                Duration::from_secs(5),
                client.execute(
                    "INSERT INTO measurements (
                        benchmark_run_id, execution_time, exit_code, measurement_order
                    ) VALUES ($1, $2, $3, $4)",
                    &[&run_id, time, exit_code, &(idx as i32)],
                ),
            )
            .await
            .with_context(|| "Timeout inserting measurement")?
            .with_context(|| "Failed to insert measurement")?;
        }
    }

    Ok(())
}

fn check_postgres_user(user: &str) -> Result<bool> {
    let output = std::process::Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-tAc")
        .arg(format!("SELECT 1 FROM pg_roles WHERE rolname = '{}'", user))
        .output()
        .with_context(|| "Failed to execute postgres user check command")?;

    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

fn check_postgres_database(database: &str) -> Result<bool> {
    let output = std::process::Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-tAc")
        .arg(format!(
            "SELECT 1 FROM pg_database WHERE datname = '{}'",
            database
        ))
        .output()
        .with_context(|| "Failed to execute postgres database check command")?;

    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

fn create_postgres_user(user: &str, password: &str) -> Result<()> {
    let status = std::process::Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!(
            "CREATE USER {} WITH PASSWORD '{}';",
            user, password
        ))
        .status()
        .with_context(|| "Failed to execute create user command")?;

    if !status.success() {
        anyhow::bail!("Failed to create user {}", user);
    }
    Ok(())
}

fn create_postgres_database(database: &str, owner: &str) -> Result<()> {
    let status = std::process::Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-X")
        .arg("-c")
        .arg(format!(
            "CREATE DATABASE {} WITH OWNER = {};",
            database, owner
        ))
        .status()
        .with_context(|| "Failed to execute create database command")?;

    if !status.success() {
        anyhow::bail!("Failed to create database {}", database);
    }
    Ok(())
}

fn grant_privileges(database: &str, user: &str) -> Result<()> {
    let status = std::process::Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!(
            "GRANT ALL PRIVILEGES ON DATABASE {} TO {};",
            database, user
        ))
        .status()
        .with_context(|| "Failed to execute grant privileges command")?;

    if !status.success() {
        anyhow::bail!("Failed to grant privileges on {} to {}", database, user);
    }
    Ok(())
}

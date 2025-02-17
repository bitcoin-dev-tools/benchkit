use anyhow::{Context, Result};
use std::process::Command;
use tokio::time::{timeout, Duration};
use tokio_postgres::NoTls;

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

pub async fn check_connection(conn_string: &str) -> Result<()> {
    let (client, connection) = tokio_postgres::connect(conn_string, NoTls)
        .await
        .with_context(|| "Failed to establish database connection")?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    timeout(Duration::from_secs(5), client.execute("SELECT 1", &[]))
        .await
        .with_context(|| "Database query timeout")?
        .with_context(|| "Failed to execute test query")?;

    Ok(())
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

    client.batch_execute(include_str!("schema.sql")).await?;

    Ok(())
}

fn check_postgres_user(user: &str) -> Result<bool> {
    let output = Command::new("sudo")
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
    let output = Command::new("sudo")
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
    let status = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!("CREATE USER {} WITH PASSWORD '{}'", user, password))
        .status()
        .with_context(|| "Failed to execute create user command")?;

    if !status.success() {
        anyhow::bail!("Failed to create user {}", user);
    }
    Ok(())
}

fn create_postgres_database(database: &str, owner: &str) -> Result<()> {
    let status = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!(
            "CREATE DATABASE {} WITH OWNER = {}",
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
    let status = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!(
            "GRANT ALL PRIVILEGES ON DATABASE {} TO {}",
            database, user
        ))
        .status()
        .with_context(|| "Failed to execute grant privileges command")?;

    if !status.success() {
        anyhow::bail!("Failed to grant privileges on {} to {}", database, user);
    }
    Ok(())
}

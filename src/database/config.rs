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

pub async fn initialize_database(config: &DatabaseConfig) -> Result<()> {
    check_postgres_running()?;

    let user_exists = check_postgres_user(&config.user)?;
    let db_exists = check_postgres_database(&config.database)?;

    if !user_exists {
        create_postgres_user(&config.user, &config.password)?;
    }

    ensure_user_createdb(&config.user)?;

    if !db_exists {
        create_postgres_database(&config.database, &config.user)?;
        grant_privileges(&config.database, &config.user)?;
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

    println!("Database setup completed successfully");
    Ok(())
}

fn check_postgres_running() -> Result<()> {
    let status = Command::new("pg_isready").status()?;
    if !status.success() {
        anyhow::bail!("PostgreSQL is not running");
    }
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

fn ensure_user_createdb(user: &str) -> Result<()> {
    let has_createdb = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-tAc")
        .arg(format!(
            "SELECT 1 FROM pg_roles WHERE rolname = '{}' AND rolcreatedb",
            user
        ))
        .output()
        .with_context(|| "Failed to check CREATEDB permission")?;

    if String::from_utf8_lossy(&has_createdb.stdout)
        .trim()
        .is_empty()
    {
        println!("Granting CREATEDB permission to {}", user);
        let status = Command::new("sudo")
            .arg("-u")
            .arg("postgres")
            .arg("psql")
            .arg("-c")
            .arg(format!("ALTER USER \"{}\" CREATEDB", user))
            .status()
            .with_context(|| "Failed to grant CREATEDB permission")?;

        if !status.success() {
            anyhow::bail!("Failed to grant CREATEDB permission to {}", user);
        }
    }

    Ok(())
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
    println!("Creating user {}", user);
    let status = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!(
            "CREATE USER \"{}\" WITH PASSWORD '{}'",
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
    println!("Creating database {}", database);
    let status = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!(
            "CREATE DATABASE \"{}\" WITH OWNER = \"{}\"",
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
    println!("Granting privileges on {} to {}", database, user);
    let status = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!(
            "GRANT ALL PRIVILEGES ON DATABASE \"{}\" TO \"{}\"",
            database, user
        ))
        .status()
        .with_context(|| "Failed to execute grant privileges command")?;

    if !status.success() {
        anyhow::bail!("Failed to grant privileges on {} to {}", database, user);
    }
    Ok(())
}

pub async fn delete_database(config: &DatabaseConfig) -> Result<()> {
    check_postgres_running()?;

    if check_postgres_database(&config.database)? {
        println!("Dropping database {}", config.database);
        drop_database(&config.database)?;
    } else {
        println!("Database {} does not exist", config.database);
    }

    if check_postgres_user(&config.user)? {
        println!("Dropping user {}", config.user);
        drop_user(&config.user)?;
    } else {
        println!("User {} does not exist", config.user);
    }

    println!("Cleanup completed successfully");
    Ok(())
}

fn drop_database(database: &str) -> Result<()> {
    // First terminate all connections to the database
    let status = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}'",
            database
        ))
        .status()
        .with_context(|| "Failed to terminate database connections")?;

    if !status.success() {
        anyhow::bail!("Failed to terminate database connections");
    }

    let status = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!("DROP DATABASE \"{}\"", database))
        .status()
        .with_context(|| format!("Failed to drop database {}", database))?;

    if !status.success() {
        anyhow::bail!("Failed to drop database {}", database);
    }
    Ok(())
}

fn drop_user(user: &str) -> Result<()> {
    let status = Command::new("sudo")
        .arg("-u")
        .arg("postgres")
        .arg("psql")
        .arg("-c")
        .arg(format!("DROP USER \"{}\"", user))
        .status()
        .with_context(|| format!("Failed to drop user {}", user))?;

    if !status.success() {
        anyhow::bail!("Failed to drop user {}", user);
    }
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

    timeout(Duration::from_secs(5), client.execute("SELECT 1", &[]))
        .await
        .with_context(|| "Database query timeout")?
        .with_context(|| "Failed to execute test query")?;

    Ok(())
}

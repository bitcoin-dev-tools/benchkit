use anyhow::Result;
use rand::Rng;
use tokio_postgres::NoTls;

pub struct TestDb {
    db_name: String,
}

impl TestDb {
    pub async fn new() -> Result<Self> {
        let db_name = format!("benchkittest_{}", random_suffix());
        Self::create_database(&db_name).await?;
        Ok(Self { db_name })
    }

    pub fn connection_string(&self) -> String {
        format!(
            "postgres://benchkittest:benchkitpw@localhost/{}",
            self.db_name
        )
    }

    async fn create_database(db_name: &str) -> Result<()> {
        let mut create = std::process::Command::new("sudo");
        create.arg("-u").arg("postgres").args([
            "psql",
            "-c",
            &format!("CREATE DATABASE {} WITH OWNER = benchkittest;", db_name),
        ]);

        println!("Creating new test database with command:\n{:?}", &create);
        create.output()?;

        let (client, connection) = tokio_postgres::connect(
            &format!("postgres://benchkittest:benchkitpw@localhost/{}", db_name),
            NoTls,
        )
        .await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        client
            .batch_execute(include_str!("../src/database/schema.sql"))
            .await?;

        Ok(())
    }

    async fn drop_database(&self) -> Result<()> {
        // First terminate all connections to the database
        let (client, connection) = tokio_postgres::connect(
            "postgres://benchkittest:benchkitpw@localhost/postgres",
            NoTls,
        )
        .await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        client
            .execute(
                &format!(
                    "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}'",
                    self.db_name
                ),
                &[],
            )
            .await?;

        let mut drop = std::process::Command::new("sudo");
        drop.arg("-u").arg("postgres").args([
            "psql",
            "-c",
            &format!("DROP DATABASE IF EXISTS {} WITH (FORCE);", self.db_name),
        ]);

        println!("Dropping test database with command:\n{:?}", &drop);
        drop.output()?;

        Ok(())
    }
}

fn random_suffix() -> String {
    let mut rng = rand::rng();
    format!("{:06}", rng.random_range(0..999999))
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let mut drop = std::process::Command::new("sudo");
        drop.arg("-u").arg("postgres").args([
            "psql",
            "-c",
            &format!("DROP DATABASE IF EXISTS {} WITH (FORCE);", self.db_name),
        ]);

        println!("Dropping test database with command:\n{:?}", &drop);
        let status = drop.status();

        if let Err(e) = status {
            eprintln!("Failed to drop test database {}: {}", self.db_name, e);
        }
    }
}

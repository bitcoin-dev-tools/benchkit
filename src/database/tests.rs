use anyhow::Result;
use std::sync::Once;
use tokio::runtime::Runtime;
use tokio_postgres::NoTls;

const TEST_DB_URL: &str = "postgres://benchkittest:benchkitpw@localhost/benchcointests";
static INIT: Once = Once::new();

fn setup_test_db() -> Result<()> {
    INIT.get_or_init(|| {
        let output = std::process::Command::new("./tests/setup-test-db.sh")
            .output()
            .expect("Failed to execute setup script");

        if !output.status.success() {
            panic!(
                "Setup script failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    });
    Ok(())
}

async fn init_test_db() -> Result<()> {
    let (client, connection) = tokio_postgres::connect(TEST_DB_URL, NoTls).await?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    client
        .batch_execute(
            "
            DROP TABLE IF EXISTS measurements;
            DROP TABLE IF EXISTS benchmark_runs;
            DROP TABLE IF EXISTS benchmarks;
            DROP TABLE IF EXISTS schema_version;
            );",
        )
        .await?;

    client
        .batch_execute(include_str!("../../src/database/schema.sql"))
        .await?;

    Ok(())
}

#[test]
fn test_db_connection() -> Result<()> {
    setup_test_db()?;
    let rt = Runtime::new()?;

    rt.block_on(async {
        init_test_db().await?;

        let (client, connection) = tokio_postgres::connect(TEST_DB_URL, NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        let row = client.query_one("SELECT 1", &[]).await?;
        assert_eq!(row.get::<_, i32>(0), 1);
        Ok(())
    })
}

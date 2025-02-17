use anyhow::Result;
use benchkit::benchmarks::Runner;
use rand::Rng;
use serial_test::serial;
use tokio_postgres::NoTls;

mod test_utils;
use test_utils::TestDb;

#[tokio::test]
#[serial]
async fn test_database_connection() -> Result<()> {
    let db = TestDb::new().await?;
    let conn_string = db.connection_string();

    let (client, connection) = tokio_postgres::connect(&conn_string, NoTls).await?;
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    let row = client.query_one("SELECT 1", &[]).await?;
    assert_eq!(row.get::<_, i32>(0), 1);
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_example_benchmark() -> Result<()> {
    let db = TestDb::new().await?;
    let conn_string = db.connection_string();

    let mut rng = rand::rng();
    let pr_number = rng.random_range(10000..100000);
    let run_id = rng.random_range(100000000..1000000000);

    let runner = Runner::new(
        "example.benchmark.yml",
        &conn_string,
        Some(pr_number),
        Some(run_id),
    )?;

    runner.run().await?;

    let (client, connection) = tokio_postgres::connect(&conn_string, NoTls).await?;
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    let benchmark_count: i64 = client
        .query_one("SELECT COUNT(*) FROM benchmarks", &[])
        .await?
        .get(0);
    assert!(benchmark_count > 0, "No benchmarks were stored");

    let run_count: i64 = client
        .query_one("SELECT COUNT(*) FROM runs", &[])
        .await?
        .get(0);
    assert!(run_count > 0, "No benchmark runs were stored");

    let measurement_count: i64 = client
        .query_one("SELECT COUNT(*) FROM measurements", &[])
        .await?
        .get(0);
    assert!(measurement_count > 0, "No measurements were stored");

    Ok(())
}

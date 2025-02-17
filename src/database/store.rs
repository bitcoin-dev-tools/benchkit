use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::time::{timeout, Duration};
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

pub async fn store_results(
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
        store_benchmark_result(&client, bench_name, result, pull_request_number, run_id).await?;
    }

    Ok(())
}

async fn store_benchmark_result(
    client: &Client,
    bench_name: &str,
    result: &BenchmarkResult,
    pull_request_number: Option<i32>,
    run_id: Option<i32>,
) -> Result<()> {
    let benchmark_id =
        insert_benchmark(client, bench_name, result, pull_request_number, run_id).await?;
    let run_id = insert_benchmark_run(client, benchmark_id, result).await?;
    insert_measurements(client, run_id, result).await?;

    Ok(())
}

async fn insert_benchmark(
    client: &Client,
    bench_name: &str,
    result: &BenchmarkResult,
    pull_request_number: Option<i32>,
    run_id: Option<i32>,
) -> Result<i32> {
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

    Ok(benchmark_id)
}

async fn insert_benchmark_run(
    client: &Client,
    benchmark_id: i32,
    result: &BenchmarkResult,
) -> Result<i32> {
    let run_id: i32 = timeout(
        Duration::from_secs(5),
        client.query_one(
            "INSERT INTO runs (
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

    Ok(run_id)
}

async fn insert_measurements(client: &Client, run_id: i32, result: &BenchmarkResult) -> Result<()> {
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

    Ok(())
}

use crate::AppError;
use rusqlite::{params, Connection, Transaction};
use serde::Deserialize;

#[derive(Deserialize)]
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

fn init_schema_version(conn: &Connection) -> Result<(), AppError> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY
        )",
        [],
    )?;

    let version: Option<i32> = conn
        .query_row("SELECT version FROM schema_version", [], |row| row.get(0))
        .ok();

    if version.is_none() {
        conn.execute("INSERT INTO schema_version (version) VALUES (0)", [])?;
    }

    Ok(())
}

fn get_schema_version(conn: &Connection) -> Result<i32, AppError> {
    let version: i32 =
        conn.query_row("SELECT version FROM schema_version", [], |row| row.get(0))?;
    Ok(version)
}

fn upgrade_schema(conn: &mut Connection) -> Result<(), AppError> {
    let tx = conn.transaction()?;
    let version = get_schema_version(&tx)?;

    if version < 1 {
        tx.execute(
            "CREATE TABLE IF NOT EXISTS benchmarks (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                command TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        tx.execute(
            "CREATE TABLE IF NOT EXISTS benchmark_runs (
                id INTEGER PRIMARY KEY,
                benchmark_id INTEGER NOT NULL,
                mean REAL NOT NULL,
                stddev REAL NOT NULL,
                median REAL NOT NULL,
                user_time REAL NOT NULL,
                system_time REAL NOT NULL,
                min_time REAL NOT NULL,
                max_time REAL NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (benchmark_id) REFERENCES benchmarks(id)
            )",
            [],
        )?;

        tx.execute(
            "CREATE TABLE IF NOT EXISTS measurements (
                id INTEGER PRIMARY KEY,
                benchmark_run_id INTEGER NOT NULL,
                execution_time REAL NOT NULL,
                exit_code INTEGER NOT NULL,
                measurement_order INTEGER NOT NULL,
                FOREIGN KEY (benchmark_run_id) REFERENCES benchmark_runs(id)
            )",
            [],
        )?;

        tx.execute("UPDATE schema_version SET version = 1", [])?;
    }

    if version < 2 {
        tx.execute(
            "ALTER TABLE benchmarks ADD COLUMN pull_request_number INTEGER",
            [],
        )?;
        tx.execute("ALTER TABLE benchmarks ADD COLUMN run_id INTEGER", [])?;

        tx.execute("UPDATE schema_version SET version = 2", [])?;
    }

    tx.commit()?;
    Ok(())
}

fn init_db(conn: &mut Connection) -> Result<(), AppError> {
    init_schema_version(conn)?;
    upgrade_schema(conn)?;
    Ok(())
}

fn store_benchmark_result(
    tx: &Transaction,
    bench_name: &str,
    result: &BenchmarkResult,
    pull_request_number: Option<i32>,
    run_id: Option<i32>,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO benchmarks (name, command, pull_request_number, run_id) 
         VALUES (?1, ?2, ?3, ?4)",
        params![bench_name, result.command, pull_request_number, run_id],
    )?;

    let benchmark_id = tx.last_insert_rowid();

    tx.execute(
        "INSERT INTO benchmark_runs (
            benchmark_id, mean, stddev, median, user_time,
            system_time, min_time, max_time
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            benchmark_id,
            result.mean,
            result.stddev,
            result.median,
            result.user,
            result.system,
            result.min,
            result.max,
        ],
    )?;

    let run_id = tx.last_insert_rowid();

    for (idx, (time, exit_code)) in result
        .times
        .iter()
        .zip(result.exit_codes.iter())
        .enumerate()
    {
        tx.execute(
            "INSERT INTO measurements (
                benchmark_run_id, execution_time, exit_code, measurement_order
            ) VALUES (?1, ?2, ?3, ?4)",
            params![run_id, time, exit_code, idx as i32],
        )?;
    }

    Ok(())
}

pub fn store_results_in_db(
    db_path: &str,
    bench_name: &str,
    result_json: &str,
    pull_request_number: Option<i32>,
    run_id: Option<i32>,
) -> Result<(), AppError> {
    let results: Results = serde_json::from_str(result_json)?;
    let mut conn = Connection::open(db_path)?;

    init_db(&mut conn)?;

    let tx = conn.transaction()?;

    for result in &results.results {
        store_benchmark_result(&tx, bench_name, result, pull_request_number, run_id)?;
    }

    tx.commit()?;
    Ok(())
}

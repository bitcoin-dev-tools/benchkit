DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'schema_version') THEN
        CREATE TABLE schema_version (
            version INTEGER PRIMARY KEY
        );
    END IF;
END $$;

INSERT INTO schema_version (version)
SELECT 0
WHERE NOT EXISTS (SELECT 1 FROM schema_version);

CREATE TABLE IF NOT EXISTS benchmarks (
    id SERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    command TEXT NOT NULL,
    pull_request_number INTEGER,
    run_id INTEGER,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS runs (
    id SERIAL PRIMARY KEY,
    benchmark_id INTEGER NOT NULL REFERENCES benchmarks(id),
    mean DOUBLE PRECISION NOT NULL,
    stddev DOUBLE PRECISION NOT NULL,
    median DOUBLE PRECISION NOT NULL,
    user_time DOUBLE PRECISION NOT NULL,
    system_time DOUBLE PRECISION NOT NULL,
    min_time DOUBLE PRECISION NOT NULL,
    max_time DOUBLE PRECISION NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS measurements (
    id SERIAL PRIMARY KEY,
    benchmark_run_id INTEGER NOT NULL REFERENCES runs(id),
    execution_time DOUBLE PRECISION NOT NULL,
    exit_code INTEGER NOT NULL,
    measurement_order INTEGER NOT NULL
);

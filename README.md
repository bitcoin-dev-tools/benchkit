# Benchkit

A Rust-based benchmarking toolkit designed for benchmarking Bitcoin Core, using
[hyperfine](https://github.com/sharkdp/hyperfine) as the underlying
benchmarking engine.

## Features

- Run single or multiple benchmarks defined in YAML configuration files
- Store benchmark results in PostgreSQL for analysis
- Support for parameterized benchmarks with multiple variable combinations
- Configurable benchmark environment variables
- Integration with CI/PR workflows via PR number and run ID tracking
- Command wrapping support (e.g., `taskset` for CPU pinning)

## Prerequisites

- Rust 1.84.1 or later
- PostgreSQL
- hyperfine
- sudo access for database operations

## Installation

```bash
cargo install --path .
```

## Usage

### Database Setup

Test database connection:
```bash
benchkit db test
```

Initialize the database:
```bash
benchkit db init
```

Delete database and user (caution):
```bash
benchkit db delete
```

### Build Bitcoin Core

Build bitcoind binaries:
```bash
benchkit build --src-dir /path/to/benchcoin/src --commits 7fd2804faf,668c9bb609 --out-dir ./binaries
```

### Running Benchmarks

Run all benchmarks from config:
```bash
benchkit run all --config benchmark.yml
```

Run a specific benchmark:
```bash
benchkit run single --config benchmark.yml --name "benchmark-name"
```

### Configuration

Benchkit uses YAML configuration files to define benchmarks. Example configuration:

```yaml
global:
  hyperfine:
    warmup: 1
    runs: 5
    export_json: results.json
    shell: /bin/bash
    show_output: true
  wrapper: "taskset -c 1-14"

benchmarks:
  - name: "Example Benchmark"
    env:
      RUST_LOG: "debug"
    hyperfine:
      command: "sleep {duration}s"
      parameter_lists:
        - var: duration
          values: ["0.1", "0.2", "0.5"]
```

### Environment Variables

Database configuration can be set via environment variables:
- `PGHOST` (default: localhost)
- `PGPORT` (default: 5432)
- `PGDATABASE` (default: benchmarks)
- `PGUSER` (default: benchkit)
- `PGPASSWORD` (default: benchcoin)

## Contributing

Contributions are welcome! Please ensure your code:
- Updates documentation as needed
- Includes appropriate tests
- Follows the project's code style

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

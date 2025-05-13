# Benchkit

A benchmarking toolkit designed for benchmarking Bitcoin Core with an internal benchmarking system.

## Features

- Run single or multiple benchmarks defined in YAML configuration files
- Support for parameterized benchmarks with multiple variable combinations
- Configurable benchmark environment variables
- Integration with CI/PR workflows via run ID tracking
- CPU affinity control with hwloc for consistent benchmark results
- System performance tuning and monitoring
- Patch management and updating
- Nix flake for devShell
- Object storage integration for benchmark artifacts
- AssumeUTXO snapshot management
- Store benchmark results in PostgreSQL for analysis

## Prerequisites

- Rust 1.84.1 or later
- Nix package manager
- hwloc library (for CPU affinity control)

## Installation

```bash
cargo install --path .
```

## Environment Configuration

The project includes an `.envrc.example` file that shows all required environment variables. If you use `direnv`, you can copy this to `.envrc` and modify it. Otherwise, ensure these variables are set in your environment.

Key environment variables:

```bash
# Object Storage (optional)
export KEY_ID=<your_id>
export SECRET_ACCESS_KEY=<your_password>

# Logging
export RUST_LOG=info
```

## Command Reference

### Building Bitcoin Core

```bash
# Build bitcoind binaries for specified commits
benchkit build
```

### Running Benchmarks

```bash
# Run all benchmarks from config
benchkit run --out-dir ./out

# Run a specific benchmark
benchkit run --name "benchmark-name" --out-dir ./out
```

### System Performance Management

```bash
# Check current system performance settings
benchkit system check

# Tune system for benchmarking (requires sudo)
benchkit system tune

# Reset system settings to default
benchkit system reset
```

### AssumeUTXO Snapshot Management

```bash
# Download snapshot for specific network
benchkit snapshot download [mainnet|signet]
```

### Patch testing

```bash
# Test the benchcoin patches apply cleanly to all refs
benchkit patch test

# Fetch latest benchkit patches from github
benchkit patch update
```

## Configuration Files

### Application Configuration (config.yml)

```yaml
home_dir: $HOME/.local/state/benchkit
bin_dir: $HOME/.local/state/benchkit/binaries
patch_dir: $HOME/.local/state/benchkit/patches
snapshot_dir: $HOME/.local/state/benchkit/snapshots
```

### Benchmark Configuration (benchmark.yml)

```yaml
global:
  benchmark:
    warmup: 1
    runs: 5
    capture_output: false

  # CPU affinity control
  benchmark_cores: "1-7"    # Cores to run benchmark commands on
  runner_cores: "0"         # Core to bind the main benchkit process to

  source: $HOME/src/core/bitcoin
  commits: ["62bd1960fdf", "e932c6168b5"]
  tmp_data_dir: /tmp/benchkit
  host: x86_64-linux-gnu

benchmarks:
  - name: "assumeutxo signet test sync"
    network: signet
    connect: 127.0.0.1:39333
    benchmark:
      command: "bitcoind -dbcache={dbcache} -stopatheight=160001"
      parameter_lists:
        - var: dbcache
          values: ["450", "32000"]
```

See [Internal Benchmarking](docs/INTERNAL_BENCHMARKING.md) for details on the new configuration format.

## Benchmark Scripts

The following scripts can be customized in the `scripts/` directory:

- `setup.sh`: Initial setup before benchmarking
- `prepare.sh`: Preparation before each benchmark run
- `conclude.sh`: Cleanup after each benchmark run
- `cleanup.sh`: Final cleanup after all benchmarks

These scripts now use named arguments instead of positional arguments. See [Internal Benchmarking](docs/INTERNAL_BENCHMARKING.md) for details.

## Tips

- If running against a local Bitcoin Core, it's generally easier to configure
  the "seed" node with custom `-port` and `-rpcport` settings, and then connect
  to it from the benchcoin node using `-connect=<host>:<port>`.

## Process Profiling

Benchkit supports runtime profiling of applications, measuring CPU usage, memory consumption, disk I/O, and other metrics over time. This is particularly useful for benchmarking Bitcoin Core operations to identify performance bottlenecks.

### Enabling Profiling

Add the following to your benchmark configuration:

```yaml
benchmark:
  # Other benchmark settings
  profile: true               # Enable profiling
  profile_interval: 1         # Sample interval in seconds
```

Profiling will:
- Track all child processes (including forks)
- Record CPU, memory, disk I/O stats over time
- Generate both JSON and CSV output files
- Record data points at the specified interval

### Profiling Output

Profiling results are stored in the benchmark output directory:
- `profile_run_N/profile_data.json` - Complete profiling data
- `profile_run_N/profile_data.csv` - CSV format for easy visualization

The results include per-sample metrics for CPU usage (percentage), memory usage (bytes), virtual memory usage (bytes), disk read/write (bytes), and elapsed time.

## Contributing

Contributions are welcome! Please ensure your code:
- Updates documentation as needed
- Follows the project's code style

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

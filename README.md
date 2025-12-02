# Benchkit

A benchmarking toolkit designed for benchmarking Bitcoin Core.

## Features

- Run single or multiple benchmarks defined in YAML configuration files
- Support for parameterized benchmarks with multiple variable combinations
- Configurable benchmark environment variables
- CPU affinity control for more consistent benchmark results (Linux only)
- System performance tuning and monitoring
- Process profiling and perf instrumentation (Linux only)
- Nix flake for integrated build and run shell environment

## Prerequisites

- Rust 1.84.1 or later
- Nix package manager

If not using Nix package manager and project flake:

- Bitcoin Core build deps, e.g. [build.md](https://github.com/bitcoin/bitcoin/blob/master/doc/build-unix.md)
- Cargo/rustc
- `hwloc` library (only if using CPU affinity control, Linux only)

## Quickstart

```bash
git clone https://github.com/bitcoin-dev-tools/benchkit.git && cd benchkit

# Optional (Recommended)
nix develop


# Build bitcoind binaries from config
cargo run -- build

# Ensure you have a signet node accepting connections on 127.0.0.1:39333 e.g.:
# `bitcoind -signet -daemon=1 -port=39333 -rpcport=39334`

# Run demo benchmarks
cargo run -- run --out-dir ./out
```

Modify `benchmark.yml` to benchmark your desired commits and parameters.

## Installation

```bash
cargo install --path .

# Now call the binary as above using "benchkit <options>"
```

## Environment Configuration

The project includes an `.envrc.example` file that shows all required
environment variables. If you use `direnv`, you can copy this to `.envrc` and
modify it. Otherwise, ensure these variables are set in your environment.

Key environment variables:

```bash
# Logging
export RUST_LOG=info
```

## Command Reference

### Building Bitcoin Core

```bash
# Build bitcoind binaries from commits specified in benchmark.yml
benchkit build
```

### Running Benchmarks

```bash
# Run all benchmarks from config
benchkit run --out-dir ./out

# Run a specific benchmark
benchkit run --name "benchmark-name" --out-dir ./out
```

### System Performance Management (Linux only)

```bash
# Check current system performance settings
benchkit system check

# Tune system for benchmarking (requires sudo)
benchkit system tune

# Reset system settings to default
benchkit system reset
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
  - name: "signet test sync"
    network: signet
    connect: 127.0.0.1:39333
    benchmark:
      command: "bitcoind -dbcache={dbcache} -stopatheight=160001"
      parameter_lists:
        - var: dbcache
          values: ["450", "32000"]

  # Example using stop_on_log_pattern (regex)
  - name: "stop on new block"
    network: signet
    connect: 127.0.0.1:39333
    benchmark:
      command: "bitcoind -dbcache={dbcache}"
      stop_on_log_pattern: "UpdateTip: new best="  # Stop when this regex matches
      runs: 3
      parameter_lists:
        - var: dbcache
          values: ["450"]

  # More regex examples:
  # stop_on_log_pattern: "UpdateTip: new best=.* height=200000"  # Stop at specific height
  # stop_on_log_pattern: "progress=0\\.11[6-9]"  # Stop at progress threshold
  # stop_on_log_pattern: "date='2024-04-18"  # Stop on specific date
```

See [Internal Benchmarking](docs/INTERNAL_BENCHMARKING.md) for details on the new configuration format.

## Benchmark Lifecycle Hooks

Benchkit uses built-in lifecycle hooks to manage the benchmark environment:

- **Setup**: Initial setup before benchmarking (creates and clears data directory)
- **Prepare**: Preparation before each benchmark run (syncs headers and loads snapshot)
- **Conclude**: Cleanup after each benchmark run (saves debug.log and clears data directory)
- **Cleanup**: Final cleanup after all benchmarks (removes temporary data)

## Tips

- If running against a local Bitcoin Core, it's generally easier to configure
  the "seed" node with custom `-port` and `-rpcport` settings, and then connect
  to it from the benchcoin node using `-connect=<host>:<port>`.

## Process Profiling

Benchkit supports runtime profiling of applications, measuring CPU usage,
memory consumption, disk I/O, and other metrics over time. This is particularly
useful for benchmarking Bitcoin Core operations to identify performance
bottlenecks.

Profiling is currently incompatible with `stop_at_log_line`.

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

Profiling results are stored in the benchmark output directory, subdirectoried undeer the run iteration:

- `<iteration>/profile_data.json` - Complete profiling data
- `<iteration>/profile_data.csv` - CSV format for easy visualization

The results include per-sample metrics for CPU usage (percentage), memory usage
(bytes), virtual memory usage (bytes), disk read/write (bytes), and elapsed
time.

## Perf Instrumentation (Linux only)

Benchkit supports running benchmarks under `perf` for detailed CPU profiling with call graphs.
When enabled, each benchmark runs twice: once normally and once under perf instrumentation.

### Enabling Perf Instrumentation

Add to your benchmark configuration:

```yaml
benchmark:
  perf_instrumentation: true
```

Requirements:

- Linux only
- `perf` must be installed
- Binaries built with `-fno-omit-frame-pointer` for better call graphs

### Perf Output

Perf data is stored alongside benchmark results:

- `<iteration>/perf.data` - Raw perf profiling data

Use standard perf tools to analyze:

```bash
perf report -i <iteration>/perf.data
perf script -i <iteration>/perf.data
```

Note: Perf instrumentation cannot be used with regular profiling - they are mutually exclusive.

## Contributing

Contributions are welcome! Please ensure your code:

- Updates documentation as needed
- Follows the project's code style

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

//! Benchmarking framework for Bitcoin Core
//!
//! The benchmarks module provides a complete framework for running performance benchmarks against
//! Bitcoin Core.
//!
//! The module is structured into several components:
//!
//! - `Runner`: Top-level orchestrator that coordinates benchmark execution
//! - `BenchmarkRunner`: Low-level executor that handles command execution and timing
//! - `Builder`: Manages building Bitcoin Core from source
//! - `RepositoryManager`: Handles Git repositories (local and remote)
//! - `HookRunner`: Executes lifecycle scripts around benchmarks
//! - `ParameterMatrix`: Manages parameter substitution for commands
//! - `ResultExporter`: Exports benchmark results to various formats
//! - `Profiler`: Collects performance metrics during benchmark runs

mod build;
pub use build::Builder;

mod repository;
pub use repository::{RepoSource, RepositoryManager};

mod hook_runner;
pub use hook_runner::{HookArgs, HookRunner, HookStage};

mod hooks;
pub use hooks::StandardHookExecutor;

mod results;
pub use results::{BenchmarkResult, InstrumentationType, RunResult, RunSummary};

mod parameters;
pub use parameters::{ParameterList, ParameterMatrix};

mod export;
pub use export::ResultExporter;

mod profiler;
pub use profiler::{ProfileSample, Profiler};

mod benchmark_runner;
pub use benchmark_runner::BenchmarkRunner;

mod runner;
pub use runner::Runner;

mod log_monitor;
pub use log_monitor::{LogMonitor, LogMonitorBuilder};

mod perf;
pub use perf::{PerfInstrumentor, PerfInstrumentorBuilder};

mod utils;
pub use utils::{binary_exists, check_binaries_exist, get_binary_path};

use anyhow::Result;
use std::path::PathBuf;

mod build;
pub use build::Builder;
mod config;
pub use config::{
    load_bench_config, merge_benchmark_options, BenchmarkConfig, BenchmarkGlobalConfig,
    BenchmarkOptions, SingleConfig,
};
mod parameter;
mod repository;
pub use repository::{RepoSource, RepositoryManager};
mod runner;
pub use runner::MainRunner;
mod hook_runner;
pub use hook_runner::{HookArgs, HookRunner, HookStage};
mod benchmark_runner;
pub use benchmark_runner::BenchmarkRunner;
mod profiler;
pub use profiler::{ProfileResult, ProfileSample, Profiler};
pub mod config_adapter;
// mod object_storage;
// pub use object_storage::ObjectStorage;

use crate::config::GlobalConfig;

/// Runner for backward compatibility - delegates to MainRunner
pub struct Runner {
    /// Global configuration
    global_config: GlobalConfig,
    /// Output directory
    out_dir: PathBuf,
}

impl Runner {
    /// Create a new Runner that delegates to MainRunner
    pub fn new(global_config: GlobalConfig, out_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            global_config,
            out_dir,
        })
    }

    /// Run benchmarks
    pub fn run(&self, name: Option<&str>) -> Result<()> {
        let runner = MainRunner::new(self.global_config.clone(), self.out_dir.clone())?;
        runner.run(name)
    }
}

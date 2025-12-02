use anyhow::Result;
use log::info;
use std::path::PathBuf;

use crate::benchmarks::hooks::StandardHookExecutor;

/// Represents the different hook script stages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookStage {
    Setup,
    Prepare,
    Conclude,
    Cleanup,
}

/// Arguments to pass to hook scripts
#[derive(Debug, Clone)]
pub struct HookArgs {
    /// Path to the binary being benchmarked
    pub binary: String,
    /// Address to connect to (e.g., for Bitcoin Core)
    pub connect_address: String,
    /// Network to use (e.g., mainnet, testnet, signet)
    pub network: String,
    /// Output directory for benchmark results
    pub out_dir: PathBuf,
    /// Temporary data directory for the benchmarked process
    pub tmp_data_dir: PathBuf,
    /// Current iteration number
    pub iteration: usize,
    /// Commit being benchmarked
    pub commit: String,
    /// Parameter string for directory organization (always present, "default" if no params)
    pub params_dir: String,
}

/// HookRunner manages the lifecycle hooks for benchmarks
pub struct HookRunner {
    executor: StandardHookExecutor,
}

impl Default for HookRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl HookRunner {
    /// Create a new HookRunner with standard hooks
    pub fn new() -> Self {
        Self {
            executor: StandardHookExecutor::new(),
        }
    }

    /// Run a hook for the given stage
    pub fn run_hook(&self, stage: HookStage, args: &HookArgs) -> Result<()> {
        info!("Running {stage:?} hook");

        match stage {
            HookStage::Setup => self.executor.setup(args),
            HookStage::Prepare => self.executor.prepare(args),
            HookStage::Conclude => self.executor.conclude(args),
            HookStage::Cleanup => self.executor.cleanup(args),
        }
    }
}

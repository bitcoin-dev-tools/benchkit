use anyhow::{Context, Result};
use clap::ValueEnum;
use log::{debug, info};
use std::path::{Path, PathBuf};

use crate::benchmarks::config::SingleConfig;
use crate::benchmarks::config_adapter::ConfigAdapter;
use crate::benchmarks::hook_runner::HookArgs;
use crate::benchmarks::parameter::ParameterList;
use crate::config::GlobalConfig;
use crate::download::SnapshotInfo;
use crate::types::Network;

/// The runner for benchmarks
pub struct MainRunner {
    global_config: GlobalConfig,
    out_dir: PathBuf,
}

impl MainRunner {
    /// Create a new MainRunner
    pub fn new(global_config: GlobalConfig, out_dir: PathBuf) -> Result<Self> {
        debug!("Using output directory: {}", out_dir.display());
        std::fs::create_dir_all(&out_dir)?;

        if std::fs::read_dir(&out_dir)?.next().is_some() {
            anyhow::bail!(
                "Output directory '{}' is not empty. Please clear it before running benchmarks",
                out_dir.display()
            );
        }

        // Copy config files to output directory
        let app_config_name = global_config.app.path.file_name().unwrap_or_default();
        let bench_config_name = global_config.bench.path.file_name().unwrap_or_default();
        std::fs::copy(
            global_config.app.path.clone(),
            out_dir.join(app_config_name),
        )?;
        std::fs::copy(
            global_config.bench.path.clone(),
            out_dir.join(bench_config_name),
        )?;

        // Dump system info
        crate::system_info::dump_sys_info(&out_dir.join("system_info"))?;

        Ok(Self {
            global_config,
            out_dir,
        })
    }

    /// Run all or a specific benchmark
    pub fn run(&self, name: Option<&str>) -> Result<()> {
        let benchmarks = match name {
            Some(n) => {
                let bench = self
                    .global_config
                    .bench
                    .benchmarks
                    .iter()
                    .enumerate()
                    .find(|(_, b)| b.name == n)
                    .with_context(|| format!("Benchmark not found: {}", n))?;
                vec![bench]
            }
            None => self
                .global_config
                .bench
                .benchmarks
                .iter()
                .enumerate()
                .collect(),
        };

        for (index, bench) in benchmarks {
            self.check_snapshot(bench, &self.global_config.app.snapshot_dir)?;
            self.run_benchmark(index, bench)?;
        }

        Ok(())
    }

    /// Check if required snapshot exists
    fn check_snapshot(&self, bench: &SingleConfig, snapshot_dir: &Path) -> Result<()> {
        // Check if we have the correct snapshot
        let network = Network::from_str(&bench.network, true)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .with_context(|| format!("Invalid network: {:?}", bench.network))?;

        if let Some(snapshot_info) = SnapshotInfo::for_network(&network) {
            let snapshot_path = snapshot_dir.join(snapshot_info.filename);
            if !snapshot_path.exists() {
                anyhow::bail!(
                    "Missing required snapshot file for network {}: {}\n
This can be downloaded with `benchkit snapshot download {}`",
                    bench.network,
                    snapshot_path.display(),
                    bench.network
                );
            }
        }

        Ok(())
    }

    /// Run a specific benchmark
    fn run_benchmark(&self, index: usize, bench: &SingleConfig) -> Result<()> {
        info!("Running benchmark: {:?}", bench.name);

        // Get merged options for this benchmark
        let options = ConfigAdapter::get_merged_options(&self.global_config.bench, index)?;

        // Create parameter lists for substitution
        let mut parameter_lists = Vec::new();
        if let Some(params) = &options.parameter_lists {
            for param in params {
                if let Some(var) = param.get("var").and_then(|v| v.as_str()) {
                    let values = if let Some(vals) = param.get("values").and_then(|v| v.as_array())
                    {
                        vals.iter()
                            .map(|v| v.as_str().unwrap_or_default().to_string())
                            .collect()
                    } else if let Some(vals) = param.get("values").and_then(|v| v.as_str()) {
                        vals.split(',').map(|s| s.trim().to_string()).collect()
                    } else {
                        Vec::new()
                    };

                    parameter_lists.push(ParameterList {
                        var: var.to_string(),
                        values,
                    });
                }
            }
        }

        // Add commits parameter list if not already present
        if !parameter_lists.iter().any(|list| list.var == "commit") {
            parameter_lists.push(ParameterList {
                var: "commit".to_string(),
                values: self.global_config.bench.global.commits.clone(),
            });
        }

        // Create benchmark runner with optional profiling
        let benchmark_runner = crate::benchmarks::benchmark_runner::BenchmarkRunner::new(
            self.out_dir.clone(),
            PathBuf::from("scripts"),
            options.capture_output,
        )
        .with_parameter_lists(parameter_lists)
        .with_profiling(options.profile.unwrap_or(false), options.profile_interval)
        .with_benchmark_cores(self.global_config.bench.global.benchmark_cores.clone());

        // Get snapshot info
        let snapshot_path = if let Some(snapshot_info) = SnapshotInfo::for_network(
            &Network::from_str(&bench.network, true)
                .map_err(|e| anyhow::anyhow!("{}", e))
                .with_context(|| format!("Invalid network: {:?}", bench.network))?,
        ) {
            self.global_config
                .app
                .snapshot_dir
                .join(snapshot_info.filename)
        } else {
            self.global_config.app.snapshot_dir.clone() // Fallback
        };

        // Get command template
        let command_template = match &options.command {
            Some(cmd) => {
                // Update command to use full binary path and apply chain param
                cmd.replace(
                    "bitcoind",
                    &format!(
                        "{}/bitcoind-{{commit}} -chain={} -datadir={} -connect={}",
                        self.global_config.app.bin_dir.display(),
                        bench.network,
                        self.global_config.bench.global.tmp_data_dir.display(),
                        bench.connect.clone().unwrap_or_default(),
                    ),
                )
            }
            None => anyhow::bail!(
                "No command template specified for benchmark: {}",
                bench.name
            ),
        };

        // Hooks are the various hyperfine-esque prepare/setup/conclude/cleanup scripts
        let hook_args = HookArgs {
            binary: format!(
                "{}/bitcoind-{{commit}}",
                self.global_config.app.bin_dir.display()
            ),
            connect_address: bench.connect.clone().unwrap_or_default(),
            network: bench.network.clone(),
            out_dir: self.out_dir.clone(),
            snapshot_path,
            tmp_data_dir: self.global_config.bench.global.tmp_data_dir.clone(),
            iteration: 0,
            commit: "{commit}".to_string(), // Will be replaced by parameter substitution
        };

        let results =
            benchmark_runner.run_parameter_matrix(&command_template, options.runs, &hook_args)?;

        let export_path = self.out_dir.join("results.json");
        crate::benchmarks::benchmark_runner::BenchmarkRunner::export_json_multiple(
            &results,
            &export_path,
        )?;

        info!("Benchmark {} completed successfully", bench.name);
        Ok(())
    }
}

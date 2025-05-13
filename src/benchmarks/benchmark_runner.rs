use anyhow::{Context, Result};
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::os::unix::process::CommandExt;
use std::os::unix::process::ExitStatusExt;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::Instant,
};

use crate::cpu_binding::CpuBinder;

use crate::benchmarks::hook_runner::{HookArgs, HookRunner, HookStage};
use crate::benchmarks::parameter::{ParameterList, ParameterMatrix};
use crate::benchmarks::profiler::{ProfileResult, Profiler};

/// Results from a single benchmark run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    /// The iteration number (0-indexed)
    pub iteration: usize,
    /// Duration in milliseconds
    pub duration_ms: f64,
    /// Exit code from the command
    pub exit_code: i32,
    /// Output from the command (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Profiling results (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<ProfileResult>,
}

/// Statistical summary of benchmark runs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    /// Minimum time in milliseconds
    pub min: f64,
    /// Maximum time in milliseconds
    pub max: f64,
    /// Mean time in milliseconds
    pub mean: f64,
    /// Median time in milliseconds
    pub median: f64,
    /// Standard deviation in milliseconds
    pub std_dev: f64,
}

/// Relative speed comparison between benchmark runs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedComparison {
    /// The command/parameters this comparison is made against
    pub reference_label: String,
    /// How many times faster this benchmark is compared to the reference
    pub times_faster: f64,
    /// Standard error of the times_faster value
    pub error: f64,
}

/// Master summary comparing all benchmark runs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterSummary {
    /// The fastest benchmark command
    pub fastest_command: String,
    /// The parameters used for the fastest benchmark
    pub fastest_parameters: HashMap<String, String>,
    /// Relative speed comparisons with other benchmarks
    pub comparisons: Vec<SpeedComparison>,
}

/// Complete results from a benchmark
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    /// The command that was executed
    pub command: String,
    /// Parameters used in the command
    pub parameters: HashMap<String, String>,
    /// Results from each run
    pub runs: Vec<RunResult>,
    /// Statistical summary
    pub summary: RunSummary,
}

/// The BenchmarkRunner handles timing and execution of commands
pub struct BenchmarkRunner {
    /// Hook runner for lifecycle scripts
    hook_runner: HookRunner,
    /// Whether to capture command output
    capture_output: bool,
    /// Parameter matrix for running template commands
    parameter_matrix: Option<ParameterMatrix>,
    /// Whether to enable profiling
    enable_profiling: bool,
    /// Directory to store profiling output
    out_dir: PathBuf,
    /// Sampling interval for profiling in seconds
    profile_interval: u64,
    /// Cores to constrain benchmarks to
    benchmark_cores: Option<String>,
}

impl BenchmarkRunner {
    /// Create a new BenchmarkRunner
    pub fn new(out_dir: PathBuf, script_dir: PathBuf, capture_output: bool) -> Self {
        Self {
            hook_runner: HookRunner::new(script_dir),
            capture_output,
            parameter_matrix: None,
            enable_profiling: false,
            out_dir,
            profile_interval: 5, // Default to 5 second interval
            benchmark_cores: None,
        }
    }

    /// Set benchmark cores to constrain command execution
    pub fn with_benchmark_cores(mut self, cores_spec: Option<String>) -> Self {
        self.benchmark_cores = cores_spec;
        self
    }

    /// Enable profiling with the specified sampling interval
    pub fn with_profiling(mut self, enable: bool, interval: Option<u64>) -> Self {
        self.enable_profiling = enable;
        if let Some(interval) = interval {
            self.profile_interval = interval;
        }
        self
    }

    /// Calculate a master summary for a set of benchmark results
    pub fn calculate_master_summary(results: &[BenchmarkResult]) -> Option<MasterSummary> {
        if results.is_empty() {
            return None;
        }

        // Find the fastest benchmark based on mean duration
        let fastest_idx = results
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.summary
                    .mean
                    .partial_cmp(&b.summary.mean)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(idx, _)| idx)
            .unwrap_or(0);

        let fastest = &results[fastest_idx];
        let fastest_mean = fastest.summary.mean;

        // Create comparisons for all other benchmarks
        let mut comparisons = Vec::new();
        for (i, result) in results.iter().enumerate() {
            if i == fastest_idx {
                continue; // Skip the fastest one (it would just be 1.0× faster than itself)
            }

            // Create a label from parameters
            let param_str = if result.parameters.is_empty() {
                "default".to_string()
            } else {
                result
                    .parameters
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            // Calculate how many times slower this benchmark is
            let times_slower = result.summary.mean / fastest_mean;

            // Simple error propagation (approximate)
            let relative_error_squared = (fastest.summary.std_dev / fastest_mean).powi(2)
                + (result.summary.std_dev / result.summary.mean).powi(2);
            let error = times_slower * relative_error_squared.sqrt();

            comparisons.push(SpeedComparison {
                reference_label: param_str,
                times_faster: times_slower,
                error,
            });
        }

        // Sort comparisons by slowest first (largest times_faster value)
        comparisons.sort_by(|a, b| {
            b.times_faster
                .partial_cmp(&a.times_faster)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Some(MasterSummary {
            fastest_command: fastest.command.clone(),
            fastest_parameters: fastest.parameters.clone(),
            comparisons,
        })
    }

    /// Set parameter lists for this benchmark runner
    pub fn with_parameter_lists(mut self, parameter_lists: Vec<ParameterList>) -> Self {
        self.parameter_matrix = Some(ParameterMatrix::new(&parameter_lists));
        self
    }

    /// Run a benchmark command with the specified number of runs
    pub fn run_benchmark(
        &self,
        command: &str,
        runs: usize,
        hook_args: &HookArgs,
    ) -> Result<BenchmarkResult> {
        let commit = &hook_args.commit;
        info!(
            "Running benchmark: {} for {} runs (commit: {})",
            command, runs, commit
        );

        // Run the setup script once before all benchmark runs
        self.hook_runner.run_hook(HookStage::Setup, hook_args)?;

        let mut results = Vec::with_capacity(runs);

        for i in 0..runs {
            // Create iteration-specific hook args
            let iter_args = HookArgs {
                iteration: i,
                ..hook_args.clone()
            };

            // Run prepare script before the benchmark run
            self.hook_runner.run_hook(HookStage::Prepare, &iter_args)?;

            // Start timing
            let start = Instant::now();

            // Execute command with profiling if enabled
            let (output, profile_result) = self.execute_command(command, i, commit)?;

            // Stop timing (if we're not profiling, otherwise the profiler takes care of timing)
            let duration = start.elapsed();
            let duration_ms = if let Some(profile) = &profile_result {
                // Use the duration from the profiler if available
                profile.duration * 1000.0
            } else {
                duration.as_secs_f64() * 1000.0
            };

            // Record result
            let run_result = RunResult {
                iteration: i,
                duration_ms,
                exit_code: output.status.code().unwrap_or(-1),
                output: if self.capture_output {
                    // Only store output if explicitly requested
                    Some(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    None
                },
                profile: profile_result,
            };

            results.push(run_result);

            // Run conclude script after the benchmark run
            self.hook_runner.run_hook(HookStage::Conclude, &iter_args)?;
        }

        // Run the cleanup script once after all benchmark runs
        self.hook_runner.run_hook(HookStage::Cleanup, hook_args)?;

        // Calculate statistics
        let summary = self.calculate_summary(&results);

        // Create the benchmark result
        let benchmark_result = BenchmarkResult {
            command: command.to_string(),
            parameters: HashMap::new(), // To be filled from config
            runs: results,
            summary,
        };

        Ok(benchmark_result)
    }

    /// Launch a command with CPU affinity constraints
    /// This is a helper function that can be used by both regular execution and profiling
    fn launch_command_with_affinity(&self, command: &str) -> Result<std::process::Child> {
        debug!("Launching command with affinity: {}", command);

        // Create a new command, setting process_group(0) to create a new process group
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(command)
            // When profiling, we don't need to capture the output to pipes
            // This prevents potential pipe buffer deadlocks
            .stdout(if self.enable_profiling {
                std::process::Stdio::null()
            } else {
                std::process::Stdio::piped()
            })
            .stderr(if self.enable_profiling {
                std::process::Stdio::null()
            } else {
                std::process::Stdio::piped()
            })
            .process_group(0); // This creates a new process group with the same ID as the child's PID

        // Spawn the command
        let child = cmd.spawn().context("Failed to spawn command")?;

        // Get the PID, which is also the process group ID due to process_group(0)
        let pid = child.id() as libc::pid_t;
        debug!("Spawned process with PID {}, which is also the PGID", pid);

        // If we have benchmark_cores, apply CPU affinity to the process and process group
        if let Some(cores) = &self.benchmark_cores {
            debug!("Binding process with PID {} to cores: {}", pid, cores);

            // Create a new CPU binder for this operation
            let mut cpu_binder = CpuBinder::new()?;

            // First bind the individual process to the specified cores
            cpu_binder.bind_pid_to_cores(pid, cores)?;

            // Now try to bind the process group
            // The process group binding might fail on some systems, but we'll try it anyway
            let pgid = -pid; // Negative PID means process group in Linux scheduling APIs

            // Use a separate block to capture any errors but continue execution
            match cpu_binder.bind_pid_to_cores(pgid, cores) {
                Ok(_) => debug!(
                    "Successfully bound process group {} to cores {}",
                    pid, cores
                ),
                Err(err) => {
                    // Log the error but continue - individual process binding is already done
                    debug!("Process group binding failed (non-critical): {}", err);
                    debug!("Individual process binding was successful and should be inherited by children");
                }
            }
        }

        Ok(child)
    }

    /// Execute a command and capture its output, optionally with profiling
    fn execute_command(
        &self,
        command: &str,
        iteration: usize,
        commit: &str,
    ) -> Result<(Output, Option<ProfileResult>)> {
        debug!("Executing command: {}", command);

        // If profiling is enabled, use the profiler to execute the command
        if self.enable_profiling {
            // Create a directory structure with commit/iteration
            let profile_out_dir = self.out_dir.join(commit).join(iteration.to_string());
            std::fs::create_dir_all(&profile_out_dir)?;

            // Create the profiler with our benchmark cores
            let mut profiler = Profiler::new(&profile_out_dir, self.profile_interval);
            profiler = profiler.with_benchmark_cores(self.benchmark_cores.clone());

            // Launch the command using our helper, which handles CPU affinity
            info!("Profiling command: {}", command);
            let child = self.launch_command_with_affinity(command)?;
            let profile_result = profiler.profile_process(command, child)?;

            // Make an Output manually for profile
            let output = Output {
                status: ExitStatusExt::from_raw(profile_result.exit_code),
                stdout: Vec::new(),
                stderr: Vec::new(),
            };

            return Ok((output, Some(profile_result)));
        }

        // For non-profiled commands, launch and wait
        let child = self.launch_command_with_affinity(command)?;

        // Wait for the command to complete and collect output
        let output = child
            .wait_with_output()
            .context("Failed to wait for command completion")?;

        if !output.status.success() {
            debug!(
                "Command failed with status: {}",
                output.status.code().unwrap_or(-1)
            );
            // We don't return an error here because we want to capture benchmark failures
            // and include them in the results
        }

        Ok((output, None))
    }

    /// Calculate summary statistics from run results
    fn calculate_summary(&self, results: &[RunResult]) -> RunSummary {
        if results.is_empty() {
            return RunSummary {
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                median: 0.0,
                std_dev: 0.0,
            };
        }

        // Extract durations
        let durations: Vec<f64> = results.iter().map(|r| r.duration_ms).collect();

        // Calculate min and max
        let min = *durations
            .iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        let max = *durations
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();

        // Calculate mean
        let mean = durations.iter().sum::<f64>() / durations.len() as f64;

        // Calculate median
        let mut sorted_durations = durations.clone();
        sorted_durations.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = if sorted_durations.len() % 2 == 0 {
            let mid = sorted_durations.len() / 2;
            (sorted_durations[mid - 1] + sorted_durations[mid]) / 2.0
        } else {
            sorted_durations[sorted_durations.len() / 2]
        };

        // Calculate standard deviation
        let variance: f64 = durations
            .iter()
            .map(|d| {
                let diff = d - mean;
                diff * diff
            })
            .sum::<f64>()
            / durations.len() as f64;
        let std_dev = variance.sqrt();

        RunSummary {
            min,
            max,
            mean,
            median,
            std_dev,
        }
    }

    /// Run all parameter combinations for a command template
    pub fn run_parameter_matrix(
        &self,
        command_template: &str,
        runs: usize,
        hook_args: &HookArgs,
    ) -> Result<Vec<BenchmarkResult>> {
        // If no parameter matrix is set, just run the command as-is
        if self.parameter_matrix.is_none() {
            let result = self.run_benchmark(command_template, runs, hook_args)?;
            return Ok(vec![result]);
        }

        let matrix = self.parameter_matrix.as_ref().unwrap();
        let commands = matrix.generate_commands(command_template);
        let mut results = Vec::with_capacity(commands.len());

        for (command, params) in commands {
            info!("Running command with parameters: {:?}", params);

            // Create a new hook_args with the specific commit for this parameter combination
            let mut current_hook_args = hook_args.clone();

            // Update the commit if it's in the params
            if let Some(commit) = params.get("commit") {
                current_hook_args.commit = commit.clone();
            }

            let mut result = self.run_benchmark(&command, runs, &current_hook_args)?;
            result.parameters = params;
            results.push(result);
        }

        Ok(results)
    }

    pub fn export_json(result: &BenchmarkResult, path: &Path) -> Result<()> {
        let json_data = serde_json::to_string_pretty(result)
            .context("Failed to serialize benchmark results")?;

        std::fs::write(path, json_data).context("Failed to write benchmark results to file")?;

        Ok(())
    }

    pub fn export_json_multiple(results: &[BenchmarkResult], path: &Path) -> Result<()> {
        // Calculate master summary if there are multiple results
        let master_summary = if results.len() > 1 {
            Self::calculate_master_summary(results)
        } else {
            None
        };

        // Create a combined structure with both results and summary
        #[derive(Serialize)]
        struct ExportData<'a> {
            results: &'a [BenchmarkResult],
            #[serde(skip_serializing_if = "Option::is_none")]
            master_summary: Option<MasterSummary>,
        }

        let export_data = ExportData {
            results,
            master_summary,
        };

        let json_data = serde_json::to_string_pretty(&export_data)
            .context("Failed to serialize benchmark results")?;

        std::fs::write(path, json_data).context("Failed to write benchmark results to file")?;

        Ok(())
    }
}

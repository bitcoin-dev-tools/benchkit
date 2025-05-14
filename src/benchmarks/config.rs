use anyhow::{Context, Result};
use log::debug;
use serde::Deserialize;
use serde_json::Value;
use shellexpand;
use std::{collections::HashMap, path::PathBuf};

/// Configuration for benchmark runs
#[derive(Debug, Deserialize, Clone)]
pub struct BenchmarkOptions {
    /// Number of warmup runs to perform (discarded from results)
    #[serde(default = "default_warmup")]
    pub warmup: usize,
    /// Number of measured runs to perform
    #[serde(default = "default_runs")]
    pub runs: usize,
    /// Whether to capture and store command output
    #[serde(default)]
    pub capture_output: bool,
    /// The command template to execute
    pub command: Option<String>,
    /// Lists of parameters to substitute in the command
    pub parameter_lists: Option<Vec<Value>>,
    /// Whether to enable profiling for this benchmark
    pub profile: Option<bool>,
    /// Sampling interval for profiling in seconds
    pub profile_interval: Option<u64>,
}

fn default_warmup() -> usize {
    0
}

fn default_runs() -> usize {
    1
}

/// Global configuration for all benchmarks
#[derive(Debug, Deserialize, Clone)]
pub struct BenchmarkGlobalConfig {
    /// Default benchmark options
    pub benchmark: Option<BenchmarkOptions>,
    /// Script paths
    pub scripts: Option<HashMap<String, String>>,
    /// CPU cores to run the benchmark on
    pub benchmark_cores: Option<String>,
    /// CPU cores to run the main program on
    pub runner_cores: Option<String>,
    /// Custom CMake build arguments
    pub cmake_build_args: Option<Vec<String>>,
    /// Path to source code repository
    pub source: PathBuf,
    /// Path to scratch directory for building
    pub scratch: PathBuf,
    /// Git commits to benchmark
    pub commits: Vec<String>,
    /// Temporary data directory for benchmark runs
    pub tmp_data_dir: PathBuf,
}

/// Configuration for a single benchmark
#[derive(Debug, Deserialize, Clone)]
pub struct SingleConfig {
    /// Benchmark name
    pub name: String,
    /// Environment variables to set
    pub env: Option<HashMap<String, String>>,
    /// Network to use (main, test, signet, etc.)
    pub network: String,
    /// Address to connect to
    pub connect: Option<String>,
    /// Benchmark-specific options (overrides global options)
    pub benchmark: HashMap<String, Value>,
}

/// Complete benchmark configuration
#[derive(Debug, Deserialize, Clone)]
pub struct BenchmarkConfig {
    /// Global configuration options
    pub global: BenchmarkGlobalConfig,
    /// List of benchmarks to run
    pub benchmarks: Vec<SingleConfig>,
    /// Unique run ID
    #[serde(default)]
    pub run_id: i64,
    #[serde(default)]
    pub path: PathBuf,
}

pub fn load_bench_config(bench_config_path: &PathBuf, run_id: i64) -> Result<BenchmarkConfig> {
    if !bench_config_path.exists() {
        anyhow::bail!("Benchmark config file not found: {:?}", bench_config_path);
    }
    let contents = std::fs::read_to_string(bench_config_path).with_context(|| {
        format!(
            "Failed to read benchmark config file: {:?}",
            bench_config_path
        )
    })?;
    let mut config: BenchmarkConfig = serde_yaml::from_str(&contents)
        .with_context(|| format!("Failed to parse YAML from file: {:?}", bench_config_path))?;
    config.run_id = run_id;

    // Expand environment variables in paths
    let source_path = config.global.source.to_string_lossy();
    let expanded_source = shellexpand::full(&source_path)
        .unwrap_or_else(|_| source_path.clone())
        .into_owned();
    config.global.source = PathBuf::from(expanded_source);

    let scratch_path = config.global.scratch.to_string_lossy();
    let expanded_scratch = shellexpand::full(&scratch_path)
        .unwrap_or_else(|_| scratch_path.clone())
        .into_owned();
    config.global.scratch = PathBuf::from(expanded_scratch);

    let tmp_data_dir_path = config.global.tmp_data_dir.to_string_lossy();
    let expanded_tmp_data_dir = shellexpand::full(&tmp_data_dir_path)
        .unwrap_or_else(|_| tmp_data_dir_path.clone())
        .into_owned();
    config.global.tmp_data_dir = PathBuf::from(expanded_tmp_data_dir);
    config.path = bench_config_path.to_path_buf();

    debug!("Using benchmark configuration\n{:?}", config);
    Ok(config)
}

/// Convert BenchmarkOptions to a HashMap for merging
fn options_to_hashmap(opts: &BenchmarkOptions) -> HashMap<String, Value> {
    let mut map = HashMap::new();
    map.insert("warmup".to_string(), Value::Number(opts.warmup.into()));
    map.insert("runs".to_string(), Value::Number(opts.runs.into()));
    map.insert(
        "capture_output".to_string(),
        Value::Bool(opts.capture_output),
    );

    if let Some(cmd) = &opts.command {
        map.insert("command".to_string(), Value::String(cmd.clone()));
    }

    if let Some(params) = &opts.parameter_lists {
        map.insert("parameter_lists".to_string(), Value::Array(params.clone()));
    }

    if let Some(profile) = opts.profile {
        map.insert("profile".to_string(), Value::Bool(profile));
    }

    if let Some(interval) = opts.profile_interval {
        map.insert(
            "profile_interval".to_string(),
            Value::Number(interval.into()),
        );
    }

    map
}

/// Convert merged HashMap back to BenchmarkOptions
fn hashmap_to_options(map: &HashMap<String, Value>) -> BenchmarkOptions {
    let warmup = map
        .get("warmup")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or_else(default_warmup);

    let runs = map
        .get("runs")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or_else(default_runs);

    let capture_output = map
        .get("capture_output")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let command = map
        .get("command")
        .and_then(|v| v.as_str())
        .map(String::from);

    let parameter_lists = map
        .get("parameter_lists")
        .and_then(|v| v.as_array())
        .cloned();

    let profile = map.get("profile").and_then(|v| v.as_bool());

    let profile_interval = map.get("profile_interval").and_then(|v| v.as_u64());

    BenchmarkOptions {
        warmup,
        runs,
        capture_output,
        command,
        parameter_lists,
        profile,
        profile_interval,
    }
}

/// Merge global and benchmark-specific options
pub fn merge_benchmark_options(
    global_opts: &Option<BenchmarkOptions>,
    benchmark_opts: &HashMap<String, Value>,
) -> Result<BenchmarkOptions> {
    // Star with global options
    let base_opts = global_opts.clone().unwrap();
    let mut map = options_to_hashmap(&base_opts);
    // Merge in benchmark-specific options.
    map.extend(benchmark_opts.clone());
    // Convert to BenchmarkOptions
    // TODO: This is nasty. get rid of it.
    Ok(hashmap_to_options(&map))
}

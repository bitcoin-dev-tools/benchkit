use anyhow::{Context, Result};
use log::debug;
use serde::Deserialize;
use serde_json::Value;
use shellexpand;
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Deserialize, Clone)]
pub struct BenchmarkGlobalConfig {
    pub hyperfine: Option<HashMap<String, Value>>,
    pub wrapper: Option<String>,
    pub source: PathBuf,
    pub commits: Vec<String>,
    pub tmp_data_dir: PathBuf,
    pub host: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SingleConfig {
    pub name: String,
    pub env: Option<HashMap<String, String>>,
    pub network: String,
    pub connect: Option<String>,
    pub hyperfine: HashMap<String, Value>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BenchmarkConfig {
    pub global: BenchmarkGlobalConfig,
    pub benchmarks: Vec<SingleConfig>,
    pub run_id: i64,
}

fn expand_path(path: &str) -> String {
    shellexpand::full(path)
        .unwrap_or_else(|_| path.into())
        .into_owned()
}

pub fn load_bench_config(bench_config_path: &PathBuf, run_id: i64) -> Result<BenchmarkConfig> {
    if !bench_config_path.exists() {
        anyhow::bail!("Config file not found: {:?}", bench_config_path);
    }

    let config_dir = bench_config_path
        .parent()
        .context("Failed to get config directory")?;

    let contents = std::fs::read_to_string(bench_config_path)
        .with_context(|| format!("Failed to read config file: {:?}", bench_config_path))?;

    // First deserialize into a temporary structure without run_id
    #[derive(Deserialize)]
    struct TempConfig {
        global: BenchmarkGlobalConfig,
        benchmarks: Vec<SingleConfig>,
    }

    let temp_config: TempConfig = serde_yaml::from_str(&contents)
        .with_context(|| format!("Failed to parse YAML from file: {:?}", bench_config_path))?;

    // Create the final config with run_id
    let mut config = BenchmarkConfig {
        global: temp_config.global,
        benchmarks: temp_config.benchmarks,
        run_id,
    };

    // Helper closure to process paths
    let process_path = |path: &mut PathBuf, is_tmp: bool| -> Result<()> {
        // Expand environment variables
        *path = PathBuf::from(expand_path(path.to_str().unwrap()));

        // Convert to absolute path if relative
        if !path.is_absolute() {
            *path = config_dir.join(&path);
        }

        // For tmp_data_dir, create if not exists. For source, verify it exists
        if is_tmp {
            debug!("Creating tmp directory: {:?}", path);
            std::fs::create_dir_all(&path)
                .with_context(|| format!("Failed to create directory: {:?}", path))?;
            debug!("Created tmp directory successfully");
        } else {
            path.canonicalize()
                .with_context(|| format!("Failed to resolve path: {:?}", path))?;
        }
        Ok(())
    };

    // Process both paths
    process_path(&mut config.global.source, false)?;
    process_path(&mut config.global.tmp_data_dir, true)?;

    debug!("Using configuration\n{:?}", config);
    Ok(config)
}

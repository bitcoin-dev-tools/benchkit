use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct GlobalConfig {
    pub database: Option<String>,
    pub hyperfine: Option<HashMap<String, Value>>,
    pub wrapper: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Benchmark {
    pub name: String,
    pub env: Option<HashMap<String, String>>,
    pub hyperfine: HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub global: Option<GlobalConfig>,
    pub benchmarks: Vec<Benchmark>,
}

pub fn load_config(path: &str) -> Result<Config> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path))?;
    serde_yaml::from_str(&contents)
        .with_context(|| format!("Failed to parse YAML from file: {}", path))
}

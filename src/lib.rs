use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use thiserror::Error;

mod db;
pub use db::store_results_in_db;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("YAML parsing error: {0}")]
    YamlError(#[from] serde_yaml::Error),

    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("SQLite error: {0}")]
    SqlError(#[from] rusqlite::Error),

    #[error("hyperfine command returned non-zero exit code: {0}")]
    CommandError(String),

    #[error("Other error: {0}")]
    Other(String),
}

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

fn load_config(path: &str) -> Result<Config, AppError> {
    let contents = std::fs::read_to_string(path)?;
    let config: Config = serde_yaml::from_str(&contents)?;

    if let Some(global) = &config.global {
        if global.database.is_none() {
            println!("Warning: No database path specified in global config. Will use default 'benchmarks.sqlite'");
        }
    } else {
        println!("Warning: No global config section found. Will use default database 'benchmarks.sqlite'");
    }

    Ok(config)
}

fn build_hyperfine_command(
    bench: &Benchmark,
    global_config: Option<&GlobalConfig>,
) -> Result<Command, AppError> {
    let mut cmd = Command::new("hyperfine");
    let mut command_str = String::new();

    // Merge global and benchmark-specific hyperfine options
    let mut final_options = HashMap::new();
    if let Some(global) = global_config {
        if let Some(global_opts) = &global.hyperfine {
            for (key, value) in global_opts {
                final_options.insert(key.clone(), value.clone());
            }
        }
    }

    // Benchmark options override global ones, but preserve global values not specified in benchmark
    for (key, value) in &bench.hyperfine {
        final_options.insert(key.clone(), value.clone());
    }

    // Special handling for command since it needs wrapper
    if let Some(Value::String(command)) = final_options.remove("command") {
        command_str = if let Some(global) = global_config {
            if let Some(wrapper) = &global.wrapper {
                format!("{} {}", wrapper, command)
            } else {
                command
            }
        } else {
            command
        };
    } else {
        return Err(AppError::Other(
            "command is required in hyperfine config".to_string(),
        ));
    }

    // Add all other hyperfine options as args
    for (key, value) in final_options {
        let arg_key = format!("--{}", key.replace('_', "-"));
        match value {
            Value::String(s) => {
                cmd.arg(arg_key).arg(s);
            }
            Value::Number(n) => {
                cmd.arg(arg_key).arg(n.to_string());
            }
            // Can we make this nicer somehow?
            Value::Array(arr) => {
                if key == "command_names" {
                    for name in arr {
                        if let Some(name_str) = name.as_str() {
                            cmd.arg("--command-name").arg(name_str);
                        }
                    }
                } else if key == "parameter_lists" {
                    for list in arr {
                        match list {
                            Value::Object(map) => {
                                let var =
                                    map.get("var").and_then(Value::as_str).ok_or_else(|| {
                                        AppError::Other(
                                            "Missing or invalid 'var' in parameter_lists"
                                                .to_string(),
                                        )
                                    })?;

                                let values = map
                                    .get("values")
                                    .and_then(Value::as_array)
                                    .ok_or_else(|| {
                                        AppError::Other(
                                            "Missing or invalid 'values' in parameter_lists"
                                                .to_string(),
                                        )
                                    })?;

                                let values_str: Vec<String> = values
                                    .iter()
                                    .map(|v| {
                                        v.as_str().map(String::from).ok_or_else(|| {
                                            AppError::Other(format!(
                                                "Invalid value in parameter_lists: {:?}",
                                                v
                                            ))
                                        })
                                    })
                                    .collect::<Result<_, _>>()?;

                                cmd.arg("--parameter-list")
                                    .arg(var)
                                    .arg(values_str.join(","));
                            }
                            _ => {
                                return Err(AppError::Other(format!(
                                    "Invalid parameter_lists entry: {:?}",
                                    list
                                )))
                            }
                        }
                    }
                }
            }
            Value::Bool(b) => {
                if b {
                    cmd.arg(arg_key);
                }
            }
            _ => {}
        }
    }

    // Add the command last
    cmd.arg(command_str);

    // Set environment variables if specified
    if let Some(env_map) = &bench.env {
        for (k, v) in env_map {
            cmd.env(k, v);
        }
    }

    Ok(cmd)
}

fn run_hyperfine_for_benchmark(
    bench: &Benchmark,
    global_config: Option<&GlobalConfig>,
) -> Result<(), AppError> {
    let mut cmd = build_hyperfine_command(bench, global_config)?;

    println!("Running hyperfine command: {:?}", cmd);
    let status = cmd.status()?;
    if !status.success() {
        return Err(AppError::CommandError(format!(
            "hyperfine failed for benchmark '{}'",
            bench.name
        )));
    }

    Ok(())
}

pub fn run_all_benchmarks(
    config_path: &str,
    pull_request_number: Option<i32>,
    run_id: Option<i32>,
) -> Result<(), AppError> {
    if !Path::new(config_path).exists() {
        return Err(AppError::Other(format!(
            "Config file not found: {}",
            config_path
        )));
    }

    let config = load_config(config_path)?;

    for bench in &config.benchmarks {
        run_hyperfine_for_benchmark(bench, config.global.as_ref())?;

        // Merge global and benchmark configs
        let mut merged_hyperfine = HashMap::new();
        if let Some(global) = &config.global {
            if let Some(global_opts) = &global.hyperfine {
                merged_hyperfine.extend(global_opts.clone());
            }
        }
        merged_hyperfine.extend(bench.hyperfine.clone());

        let export_path = merged_hyperfine
            .get("export_json")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AppError::Other(format!(
                    "Missing required 'export_json' field in benchmark '{}'",
                    bench.name
                ))
            })?;

        if !Path::new(export_path).exists() {
            return Err(AppError::Other(format!(
                "Expected JSON results file not found at '{}' for benchmark '{}'",
                export_path, bench.name
            )));
        }

        let results_json = std::fs::read_to_string(export_path)?;
        let database_path = config
            .global
            .as_ref()
            .and_then(|g| g.database.as_ref())
            .map(String::as_str)
            .unwrap_or("benchmarks.sqlite");

        store_results_in_db(
            database_path,
            &bench.name,
            &results_json,
            pull_request_number,
            run_id,
        )?;
        std::fs::remove_file(export_path)?;
    }

    Ok(())
}

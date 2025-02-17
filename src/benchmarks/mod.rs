use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

mod config;
pub use config::{Benchmark, Config, GlobalConfig};

pub struct Runner {
    config: Config,
    database_url: String,
    pull_request_number: Option<i32>,
    run_id: Option<i32>,
}

impl Runner {
    pub fn new(
        config_path: &str,
        database_url: &str,
        pull_request_number: Option<i32>,
        run_id: Option<i32>,
    ) -> Result<Self> {
        if !Path::new(config_path).exists() {
            anyhow::bail!("Config file not found: {}", config_path);
        }

        let config = config::load_config(config_path)?;

        Ok(Self {
            config,
            database_url: database_url.to_string(),
            pull_request_number,
            run_id,
        })
    }

    pub async fn run(&self) -> Result<()> {
        for bench in &self.config.benchmarks {
            self.run_benchmark(bench).await?;
        }
        Ok(())
    }

    async fn run_benchmark(&self, bench: &Benchmark) -> Result<()> {
        self.run_hyperfine(bench)?;

        let mut merged_hyperfine = HashMap::new();
        if let Some(global) = &self.config.global {
            if let Some(global_opts) = &global.hyperfine {
                merged_hyperfine.extend(global_opts.clone());
            }
        }
        merged_hyperfine.extend(bench.hyperfine.clone());

        let export_path = merged_hyperfine
            .get("export_json")
            .and_then(Value::as_str)
            .with_context(|| {
                format!(
                    "Missing required 'export_json' field in benchmark '{}'",
                    bench.name
                )
            })?;

        if !Path::new(export_path).exists() {
            anyhow::bail!(
                "Expected JSON results file not found at '{}' for benchmark '{}'",
                export_path,
                bench.name
            );
        }

        let results_json = std::fs::read_to_string(export_path)
            .with_context(|| format!("Failed to read results file: {}", export_path))?;

        let db_url = self
            .config
            .global
            .as_ref()
            .and_then(|g| g.database.as_ref())
            .map(String::as_str)
            .unwrap_or(&self.database_url);

        crate::database::store_results(
            db_url,
            &bench.name,
            &results_json,
            self.pull_request_number,
            self.run_id,
        )
        .await?;

        std::fs::remove_file(export_path)
            .with_context(|| format!("Failed to remove results file: {}", export_path))?;

        Ok(())
    }

    fn run_hyperfine(&self, bench: &Benchmark) -> Result<()> {
        let mut cmd = self.build_hyperfine_command(bench)?;

        println!("Running hyperfine command: {:?}", cmd);
        let status = cmd.status().with_context(|| {
            format!("Failed to execute hyperfine for benchmark '{}'", bench.name)
        })?;

        if !status.success() {
            anyhow::bail!("hyperfine failed for benchmark '{}'", bench.name);
        }

        Ok(())
    }

    fn build_hyperfine_command(&self, bench: &Benchmark) -> Result<Command> {
        let mut cmd = Command::new("hyperfine");
        let command_str;

        let mut final_options = HashMap::new();
        if let Some(global) = &self.config.global {
            if let Some(global_opts) = &global.hyperfine {
                final_options.extend(global_opts.clone());
            }
        }

        final_options.extend(bench.hyperfine.clone());

        if let Some(Value::String(command)) = final_options.remove("command") {
            command_str = if let Some(global) = &self.config.global {
                if let Some(wrapper) = &global.wrapper {
                    format!("{} {}", wrapper, command)
                } else {
                    command
                }
            } else {
                command
            };
        } else {
            anyhow::bail!("command is required in hyperfine config");
        }

        for (key, value) in final_options {
            let arg_key = format!("--{}", key.replace('_', "-"));
            match value {
                Value::String(s) => {
                    cmd.arg(arg_key).arg(s);
                }
                Value::Number(n) => {
                    cmd.arg(arg_key).arg(n.to_string());
                }
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
                                    let var = map.get("var").and_then(Value::as_str).with_context(
                                        || "Missing or invalid 'var' in parameter_lists",
                                    )?;

                                    let values =
                                        map.get("values").and_then(Value::as_array).with_context(
                                            || "Missing or invalid 'values' in parameter_lists",
                                        )?;

                                    let values_str: Vec<String> = values
                                        .iter()
                                        .map(|v| {
                                            v.as_str().map(String::from).with_context(|| {
                                                format!("Invalid value in parameter_lists: {:?}", v)
                                            })
                                        })
                                        .collect::<Result<_>>()?;

                                    cmd.arg("--parameter-list")
                                        .arg(var)
                                        .arg(values_str.join(","));
                                }
                                _ => anyhow::bail!("Invalid parameter_lists entry: {:?}", list),
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

        cmd.arg(command_str);

        if let Some(env_map) = &bench.env {
            for (k, v) in env_map {
                cmd.env(k, v);
            }
        }

        Ok(cmd)
    }
}

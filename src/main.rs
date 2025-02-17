use benchkit::{run_all_benchmarks, AppError};
use clap::Parser;

/// A simple wrapper around hyperfine that reads a YAML config and stores results in SQLite.
#[derive(Parser, Debug)]
#[command(name = "benchkit")]
#[command(
    author = "Will Clark",
    version = "0.1.0",
    about = "Run benchmarks using hyperfine from a YAML config"
)]
struct Cli {
    #[arg(short, long, default_value = "benchmark.yml")]
    config: String,

    #[arg(long)]
    pr_number: Option<i32>,

    #[arg(long)]
    run_id: Option<i32>,
}

fn main() -> Result<(), AppError> {
    let cli = Cli::parse();
    run_all_benchmarks(&cli.config, cli.pr_number, cli.run_id)?;
    println!("All benchmarks completed successfully.");
    Ok(())
}

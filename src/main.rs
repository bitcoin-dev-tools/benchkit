use anyhow::Result;
use benchkit::{
    benchmarks::{self, load_bench_config, BenchmarkConfig},
    config::{load_app_config, AppConfig, GlobalConfig},
    download::download_snapshot,
    system::SystemChecker,
    types::Network,
};

use clap::{Parser, Subcommand};
use env_logger::Env;
// use futures::StreamExt;
use log::{info, warn};
// use object_store::aws::{AmazonS3, AmazonS3Builder};
// use object_store::ObjectStore;
use std::{path::PathBuf, process};

const DEFAULT_CONFIG: &str = "config.yml";
const DEFAULT_BENCH_CONFIG: &str = "benchmark.yml";
// const BUCKET: &str = "benchcoin";
// const OBJECT_URL: &str = "https://hel1.your-objectstorage.com";

#[derive(Parser, Debug)]
#[command(
    version,
    about,
    long_about = "Run benchmarks for Bitcoin Core from a YAML config"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Application config
    #[arg(short, long, default_value = DEFAULT_CONFIG)]
    app_config: PathBuf,

    /// Benchmark config
    #[arg(short, long, default_value = DEFAULT_BENCH_CONFIG)]
    bench_config: PathBuf,

    /// Run ID (for CI)
    #[arg(short, long, env = "BENCH_RUN_ID")]
    run_id: Option<i64>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Build bitcoin core binaries
    Build {},
    /// Run benchmarks
    Run {
        /// Benchmark name to run (optional - runs all if not specified)
        #[arg(short, long)]
        name: Option<String>,

        /// Output directory for storing benchmark artifacts
        #[arg(short, long, required = true)]
        out_dir: PathBuf,
    },
    /// Download an assumeutxo snapshot
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCommands,
    },
    /// Check system performance settings
    System {
        #[command(subcommand)]
        command: SystemCommands,
    },
    S3,
    /// Check patches apply cleanly
    Patch {
        #[command(subcommand)]
        command: PatchCommands,
    },
}

#[derive(Subcommand, Debug)]
enum SystemCommands {
    /// Check current system configuration
    Check,
    /// Tune the system for benchmarking (requires sudo)
    Tune,
    /// Reset a previous tune
    Reset,
}

#[derive(Subcommand, Debug)]
enum SnapshotCommands {
    /// Download a snapshot
    Download {
        /// Network (mainnet or signet)
        #[arg(value_enum)]
        network: Network,
    },
}

#[derive(Subcommand, Debug)]
enum PatchCommands {
    /// Downloade latest patches from GitHub
    Update {},
    /// Test the patches will apply cleanly
    Test {},
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();

    // Run system commands without loading any configuration
    if let Commands::System { command } = &cli.command {
        let checker = SystemChecker::new()?;
        match command {
            SystemCommands::Check => checker.run_checks()?,
            SystemCommands::Tune => checker.tune()?,
            SystemCommands::Reset => checker.reset()?,
        }
        process::exit(0);
    }

    // If we didn't get a run_id generate a random one.
    // The run_id is used as a temporary directory for the run, collecting artifacts/pusing to S3
    // buckets.
    let run_id = cli.run_id.unwrap_or_else(|| {
        let id = generate_id(false);
        warn!("No run_id specified. Generated random run_id: {}", id);
        id
    });

    let app: AppConfig = load_app_config(&cli.app_config)?;
    let bench: BenchmarkConfig = load_bench_config(&cli.bench_config, run_id)?;
    let config = GlobalConfig { app, bench };

    match &cli.command {
        Commands::Build {} => {
            let mut builder = benchmarks::Builder::new(config.clone())?;
            builder.build()?;
        }
        Commands::Run { name, out_dir } => {
            let mut builder = benchmarks::Builder::new(config.clone())?;
            builder.build()?;
            if let Some(runner_cores) = &config.bench.global.runner_cores {
                let mut cpu_binder = benchkit::cpu_binding::CpuBinder::new()?;
                info!("Binding main benchkit process to cores: {}", runner_cores);
                cpu_binder.bind_current_process_to_cores(runner_cores)?;
            }
            let runner = benchmarks::Runner::new(config.clone(), out_dir.clone())?;
            runner.run(name.as_deref())?;
            info!(
                "{} completed successfully.",
                name.as_deref().unwrap_or("All benchmarks")
            );
        }
        Commands::Snapshot { command } => match command {
            SnapshotCommands::Download { network } => {
                download_snapshot(network, &config.app.snapshot_dir)?;
            }
        },
        Commands::Patch { command } => match command {
            PatchCommands::Test {} => {
                let mut builder = benchmarks::Builder::new(config.clone())?;
                builder.test_patch_commits()?;
            }
            PatchCommands::Update {} => {
                let builder = benchmarks::Builder::new(config.clone())?;
                builder.update_patches(true)?;
            }
        },
        // Commands::S3 {} => {
        //     // Create an S3 store pointing to Hetzner
        //     let key_id = std::env::var("KEY_ID").unwrap();
        //     let secret_key = std::env::var("SECRET_ACCESS_KEY").unwrap();
        //     info!("Using:");
        //     info!("  url: {OBJECT_URL}");
        //     info!("  bucket: {BUCKET}");
        //     info!("  key_id: {key_id}");
        //     // info!("  secret_key: {secret_key}");
        //     let store = AmazonS3Builder::new()
        //         .with_bucket_name(BUCKET)
        //         .with_access_key_id(key_id)
        //         .with_secret_access_key(secret_key)
        //         .with_endpoint(OBJECT_URL)
        //         .build()?;
        //     list_files(&store).await?;
        // }
        _ => {}
    }

    Ok(())
}

// async fn list_files(store: &AmazonS3) -> anyhow::Result<()> {
//     let mut list_stream = store.list(None);
//
//     while let Some(meta) = list_stream.next().await {
//         match meta {
//             Ok(meta) => {
//                 info!("Name: {}, Size: {} bytes", meta.location, meta.size);
//             }
//             Err(e) => error!("Error listing object: {}", e),
//         }
//     }
//
//     Ok(())
// }

fn generate_id(pr: bool) -> i64 {
    use rand::Rng;
    let mut rng = rand::rng();
    if pr {
        rng.random_range(100_000_000..999_999_999)
    } else {
        rng.random_range(1000..50000)
    }
}

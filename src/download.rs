use crate::types::Network;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use log::info;
use reqwest::Client;
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[derive(Debug)]
pub struct SnapshotInfo {
    pub network: Network,
    pub filename: &'static str,
    pub height: u32,
}

impl SnapshotInfo {
    pub fn for_network(network: &Network) -> Option<Self> {
        match network {
            Network::Mainnet => Some(Self {
                network: Network::Mainnet,
                filename: "mainnet-840000.dat",
                height: 840000,
            }),
            Network::Signet => Some(Self {
                network: Network::Signet,
                filename: "signet-160000.dat",
                height: 160000,
            }),
        }
    }
}

const SNAPSHOT_HOST: &str = "https://utxo.download/";

pub async fn download_snapshot(network: &Network, snapshot_dir: &Path) -> anyhow::Result<()> {
    let snapshot_info = SnapshotInfo::for_network(network)
        .ok_or_else(|| anyhow::anyhow!("No snapshot available for network {:?}", network))?;
    let filename = snapshot_info.filename;

    let url = format!("{}{}", SNAPSHOT_HOST, filename);
    let client = Client::new();
    let filepath = snapshot_dir.join(filename);
    info!("Downloading {url} to {filepath:?}");

    // Get the content length for the progress bar
    let response = client.get(&url).send().await?;
    let total_size = response.content_length().unwrap_or(0);

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:60.magenta/black}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            // .progress_chars("█▓▒░  "),
            .progress_chars("⟨⟨⟨⟨⟨····· "),
    );
    let mut file = File::create(&filepath).await?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }
    pb.finish();
    info!("Successfully downloaded {filepath:?}");
    Ok(())
}

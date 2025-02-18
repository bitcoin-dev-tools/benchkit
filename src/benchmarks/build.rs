use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Builder {
    src_dir: PathBuf,
    out_dir: PathBuf,
    commits: Vec<String>,
}

impl Builder {
    pub fn new(src_dir: &Path, out_dir: &Path, commits: Vec<String>) -> Result<Self> {
        if !src_dir.exists() {
            anyhow::bail!("Source directory does not exist: {}", src_dir.display());
        }

        // Expand any short commit hashes to full hashes
        let mut full_commits = Vec::new();
        for commit in &commits {
            let output = Command::new("git")
                .current_dir(src_dir)
                .arg("rev-parse")
                .arg(commit)
                .output()
                .with_context(|| format!("Failed to expand commit hash '{}'", commit))?;

            if !output.status.success() {
                anyhow::bail!("Failed to resolve commit hash '{}'", commit);
            }

            let full_hash = String::from_utf8(output.stdout)
                .with_context(|| format!("Invalid UTF-8 in git output for commit '{}'", commit))?
                .trim()
                .to_string();

            full_commits.push(full_hash);
        }

        std::fs::create_dir_all(out_dir)?;

        Ok(Self {
            src_dir: src_dir.to_path_buf(),
            out_dir: out_dir.to_path_buf(),
            commits: full_commits,
        })
    }

    pub fn build(&self) -> Result<()> {
        let initial_ref = self.get_initial_ref()?;

        for commit in &self.commits {
            self.build_commit(commit)?;
        }

        self.restore_git_state(&initial_ref)?;
        Ok(())
    }

    fn get_initial_ref(&self) -> Result<String> {
        let output = Command::new("git")
            .current_dir(&self.src_dir)
            .arg("symbolic-ref")
            .arg("-q")
            .arg("HEAD")
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8(output.stdout)?.trim().to_string())
        } else {
            let output = Command::new("git")
                .current_dir(&self.src_dir)
                .arg("rev-parse")
                .arg("HEAD")
                .output()?;

            if output.status.success() {
                Ok(String::from_utf8(output.stdout)?.trim().to_string())
            } else {
                anyhow::bail!("Failed to get git ref");
            }
        }
    }

    fn build_commit(&self, commit: &str) -> Result<()> {
        self.checkout_commit(commit)?;
        self.run_build(commit)?;
        self.copy_binary(commit)?;
        Ok(())
    }

    fn checkout_commit(&self, commit: &str) -> Result<()> {
        let status = Command::new("git")
            .current_dir(&self.src_dir)
            .arg("checkout")
            .arg(commit)
            .status()
            .with_context(|| format!("Failed to checkout commit {}", commit))?;

        if !status.success() {
            anyhow::bail!("Git checkout failed for commit {}", commit);
        }
        Ok(())
    }

    fn run_build(&self, commit: &str) -> Result<()> {
        let short_commit = &commit[..12];

        let mut cmd = if std::env::var("CI").is_ok() {
            let mut cmd = Command::new("taskset");
            cmd.current_dir(&self.src_dir)
                .arg("-c")
                .arg("2-15")
                .arg("chrt")
                .arg("-f")
                .arg("1")
                .arg("bench-ci/guix/guix-build");
            cmd
        } else {
            let mut cmd = Command::new("bench-ci/guix/guix-build");
            cmd.current_dir(&self.src_dir);
            cmd
        };

        let status = cmd
            // Remember to re-set these in CI...
            // .env("HOSTS", "x86_64-linux-gnu")
            // .env("SOURCES_PATH", "/data/SOURCES_PATH")
            // .env("BASE_CACHE", "/data/BASE_CACHE")
            .status()
            .with_context(|| format!("Failed to run build for commit {}", commit))?;

        if !status.success() {
            anyhow::bail!("Build failed for commit {}", commit);
        }

        let archive_path = self.src_dir.join(format!(
            "guix-build-{}/output/x86_64-linux-gnu/bitcoin-{}-x86_64-linux-gnu.tar.gz",
            short_commit, short_commit
        ));

        let status = Command::new("tar")
            .current_dir(&self.src_dir)
            .arg("-xzf")
            .arg(&archive_path)
            .status()
            .with_context(|| format!("Failed to extract archive for commit {}", commit))?;

        if !status.success() {
            anyhow::bail!("Failed to extract archive for commit {}", commit);
        }

        Ok(())
    }

    fn copy_binary(&self, commit: &str) -> Result<()> {
        let short_commit = &commit[..12];
        let src_path = self
            .src_dir
            .join(format!("bitcoin-{}/bin/bitcoind", short_commit));
        let dest_path = self.out_dir.join(format!("bitcoind-{}", commit));

        std::fs::copy(&src_path, &dest_path)
            .with_context(|| format!("Failed to copy binary for commit {}", commit))?;

        std::fs::remove_dir_all(self.src_dir.join(format!("bitcoin-{}", short_commit)))
            .with_context(|| format!("Failed to cleanup extracted files for commit {}", commit))?;

        Ok(())
    }

    fn restore_git_state(&self, initial_ref: &str) -> Result<()> {
        let status = Command::new("git")
            .current_dir(&self.src_dir)
            .arg("checkout")
            .arg(initial_ref)
            .status()
            .with_context(|| format!("Failed to restore git state to {}", initial_ref))?;

        if !status.success() {
            anyhow::bail!("Failed to restore git state");
        }
        Ok(())
    }
}

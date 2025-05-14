use anyhow::{Context, Result};
use log::{debug, info};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::benchmarks::{RepoSource, RepositoryManager};
use crate::config::GlobalConfig;

pub struct Builder {
    config: GlobalConfig,
    patches: Vec<String>,
    repo_manager: Option<RepositoryManager>,
}

impl Builder {
    pub fn new(config: GlobalConfig) -> Result<Self> {
        let source_path = config.bench.global.source.to_string_lossy().to_string();
        let repo_source = RepoSource::new(&source_path);

        let patches = vec!["0001-validation-assumeutxo-benchmarking-patches.patch".to_string()];

        match &repo_source {
            RepoSource::Local(path) => {
                // For local repos, verify the path exists
                if !path.exists() {
                    anyhow::bail!("Source directory does not exist: {}", path.display());
                }

                // We don't need a repo manager for local repos
                Ok(Self {
                    config,
                    patches,
                    repo_manager: None,
                })
            }
            RepoSource::Remote(url) => {
                // For remote repos, create a repository manager
                info!("Using remote Git repository: {}", url);
                let repo_cache_dir = config.bench.global.scratch.join("repos");
                let repo_manager = RepositoryManager::new(&source_path, &repo_cache_dir);

                Ok(Self {
                    config,
                    patches,
                    repo_manager: Some(repo_manager),
                })
            }
        }
    }

    fn binary_exists(&self, commit: &str) -> bool {
        let binary_path = self.config.app.bin_dir.join(format!("bitcoind-{}", commit));
        binary_path.exists()
    }

    pub fn build(&mut self) -> Result<()> {
        // If we're using a remote repository, ensure it's available
        let source_dir = if let Some(repo_manager) = &mut self.repo_manager {
            let repo_path = repo_manager.ensure_repository_available()?;
            repo_manager.validate_commits(&self.config.bench.global.commits)?;
            repo_path
        } else {
            // Using a local repository
            self.config.bench.global.source.clone()
        };

        self.check_clean_worktree(&source_dir)?;
        // Get the initial reference to restore later
        let initial_ref = self.get_initial_ref(&source_dir)?;

        // Build all commits up-front
        for commit in &self.config.bench.global.commits {
            if !self.binary_exists(commit) {
                info!("Building binary for commit {}", commit);
                self.build_commit(&source_dir, commit)?;
            } else {
                info!(
                    "Binary already exists for commit {}, skipping build",
                    commit
                );
            };
        }

        // Restore the original git state
        self.restore_git_state(&source_dir, &initial_ref)?;
        Ok(())
    }

    fn check_clean_worktree(&self, source_dir: &PathBuf) -> Result<()> {
        let unstaged = Command::new("git")
            .current_dir(source_dir)
            .args(["diff", "--quiet"])
            .status()?;

        if !unstaged.success() {
            anyhow::bail!("Worktree has unstaged changes. Please commit or stash them first.");
        }

        let staged = Command::new("git")
            .current_dir(source_dir)
            .args(["diff", "--quiet", "--staged"])
            .status()?;

        if !staged.success() {
            anyhow::bail!("Worktree has staged changes. Please commit or stash them first.");
        }
        Ok(())
    }

    fn get_initial_ref(&self, source_dir: &PathBuf) -> Result<String> {
        // Get the initial ref to check back out to afterwards
        let output = Command::new("git")
            .current_dir(source_dir)
            .arg("symbolic-ref")
            .arg("-q")
            .arg("HEAD")
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8(output.stdout)?.trim().to_string())
        } else {
            let output = Command::new("git")
                .current_dir(source_dir)
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

    fn build_commit(&self, source_dir: &PathBuf, original_commit: &str) -> Result<()> {
        self.checkout_commit(source_dir, original_commit)?;
        let patched_commit = self.apply_patches(source_dir)?;
        debug!("Commit hash after applying patches: {}", patched_commit);
        self.run_build(source_dir, original_commit)?;
        self.copy_binary(original_commit)?;
        Ok(())
    }

    pub fn test_patch_commits(&mut self) -> Result<()> {
        // If we're using a remote repository, ensure it's available
        let source_dir = if let Some(repo_manager) = &mut self.repo_manager {
            let repo_path = repo_manager.ensure_repository_available()?;
            repo_manager.validate_commits(&self.config.bench.global.commits)?;
            repo_path
        } else {
            self.config.bench.global.source.clone()
        };

        self.check_clean_worktree(&source_dir)?;
        let initial_ref = self.get_initial_ref(&source_dir)?;

        for commit in &self.config.bench.global.commits {
            self.checkout_commit(&source_dir, commit)?;
            self.test_patches(&source_dir)?;
        }

        self.restore_git_state(&source_dir, &initial_ref)?;
        Ok(())
    }

    fn checkout_commit(&self, source_dir: &PathBuf, commit: &str) -> Result<()> {
        let status = Command::new("git")
            .current_dir(source_dir)
            .arg("checkout")
            .arg(commit)
            .status()
            .with_context(|| format!("Failed to checkout commit {}", commit))?;

        if !status.success() {
            anyhow::bail!("Git checkout failed for commit {}", commit);
        }
        Ok(())
    }

    fn apply_patches(&self, source_dir: &PathBuf) -> Result<String> {
        self.process_patches(source_dir, false)?;

        // Get the current commit hash after applying patches
        let output = Command::new("git")
            .current_dir(source_dir)
            .arg("rev-parse")
            .arg("HEAD")
            .output()
            .context("Failed to get HEAD commit hash after applying patches")?;

        if !output.status.success() {
            anyhow::bail!("Failed to get HEAD commit hash after applying patches");
        }

        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }

    fn test_patches(&self, source_dir: &PathBuf) -> Result<()> {
        self.process_patches(source_dir, true)
    }

    fn download_patch(&self, patch_name: &str, patches_dir: &PathBuf) -> Result<()> {
        let client = reqwest::blocking::Client::new();
        let url = format!(
            "https://raw.githubusercontent.com/bitcoin-dev-tools/benchkit/master/patches/{}",
            patch_name
        );
        let response = client.get(&url).send()?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to download patch {}: {}",
                patch_name,
                response.status()
            );
        }

        let content = response.bytes()?;
        let patch_path = patches_dir.join(patch_name);

        // Ensure the patches directory exists
        if !patches_dir.exists() {
            std::fs::create_dir_all(patches_dir)?;
        }

        std::fs::write(&patch_path, content)?;
        info!("Successfully downloaded patch: {}", patch_name);
        Ok(())
    }

    pub fn update_patches(&self, force: bool) -> Result<()> {
        for patch in &self.patches {
            let patch_path = &self.config.app.patch_dir.join(patch);
            if !patch_path.exists() || force {
                info!("Downloading patch: {patch}");
                self.download_patch(patch, &self.config.app.patch_dir)?;
            } else {
                info!("Patch {patch} already exists, skipping download");
            }
        }
        Ok(())
    }

    fn process_patches(&self, source_dir: &PathBuf, check_only: bool) -> Result<()> {
        self.update_patches(false)?;

        let patches_dir = &self.config.app.patch_dir;

        // Verify all patches exist
        for patch in &self.patches {
            let patch_path = patches_dir.join(patch);
            if !patch_path.exists() {
                anyhow::bail!("Patch file not found: {}", patch_path.display());
            }
        }

        // Apply each patch
        for patch in &self.patches {
            let patch_path = patches_dir.join(patch);
            let operation = if check_only { "Testing" } else { "Applying" };
            info!("{} patch: {}", operation, patch_path.display());

            let mut cmd = Command::new("git");
            cmd.current_dir(source_dir);

            if check_only {
                cmd.arg("apply")
                    .arg("--check")
                    .arg(patch_path.display().to_string());
            } else {
                cmd.arg("-c")
                    .arg("user.name=temp")
                    .arg("-c")
                    .arg("user.email=temp@temp.com")
                    .arg("am")
                    .arg("--no-signoff")
                    .arg(patch_path.display().to_string());
            }

            let status = cmd.status().with_context(|| {
                let action = if check_only { "test" } else { "apply" };
                format!("Failed to {} patch {}", action, patch)
            })?;

            if !status.success() {
                if !check_only {
                    // If patch application fails, abort the am session
                    let _ = Command::new("git")
                        .current_dir(source_dir)
                        .arg("am")
                        .arg("--abort")
                        .status();
                }
                anyhow::bail!(
                    "Failed to {} patch: {}",
                    if check_only { "test" } else { "apply" },
                    patch
                );
            }

            let action = if check_only { "tested" } else { "applied" };
            info!("Successfully {} patch: {}", action, patch);
        }
        Ok(())
    }

    fn run_build(&self, source_dir: &PathBuf, commit_hash: &str) -> Result<()> {
        // Make a build-dir using the commit-hash
        let dir = self
            .config
            .bench
            .global
            .scratch
            .join(format!("build-{}", commit_hash));

        info!("Making build dir: {:?}", dir);
        fs::create_dir_all(&dir)?;
        let canonical_dir = fs::canonicalize(&dir)?;

        // Run cmake configuration
        let mut cmd = Command::new("cmake");
        cmd.current_dir(source_dir)
            .arg("-B")
            .arg(&canonical_dir);

        // Add default build flags
        cmd.arg("-DBUILD_CLI=OFF")
            .arg("-DBUILD_TESTS=OFF");

        // Add custom build flags if configured
        if let Some(cmake_args) = &self.config.bench.global.cmake_build_args {
            for arg in cmake_args {
                cmd.arg(arg);
            }
        }

        let config_status = cmd
            .status()
            .with_context(|| format!("Failed to configure cmake for commit {}", commit_hash))?;

        if !config_status.success() {
            anyhow::bail!("CMake configuration failed for commit {}", commit_hash);
        }

        // Run cmake build
        let mut cmd = Command::new("cmake");
        cmd.current_dir(source_dir)
            .arg("--build")
            .arg(&canonical_dir)
            .arg("--target")
            .arg("bitcoind")
            .arg("--parallel");

        let build_status = cmd
            .status()
            .with_context(|| format!("Failed to build bitcoind for commit {commit_hash}"))?;

        if !build_status.success() {
            anyhow::bail!("CMake build failed for commit {commit_hash}");
        }

        Ok(())
    }

    fn copy_binary(&self, commit_hash: &str) -> Result<()> {
        let dir = self
            .config
            .bench
            .global
            .scratch
            .join(format!("build-{}", commit_hash));
        let src_path = dir.clone().join("bin/bitcoind");
        let dest_path = self
            .config
            .app
            .bin_dir
            .join(format!("bitcoind-{}", commit_hash));
        debug!("Copying {src_path:?} to {dest_path:?}");

        // Make sure the destination directory exists
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        std::fs::copy(&src_path, &dest_path)
            .with_context(|| format!("Failed to copy binary for commit {commit_hash}"))?;

        std::fs::remove_dir_all(
            self.config
                .bench
                .global
                .scratch
                .join(format!("build-{}", commit_hash)),
        )
        .with_context(|| {
            format!(
                "Failed to cleanup extracted files for commit {} from {:?}",
                commit_hash, dir
            )
        })?;

        Ok(())
    }

    fn restore_git_state(&self, source_dir: &PathBuf, initial_ref: &str) -> Result<()> {
        let status = Command::new("git")
            .current_dir(source_dir)
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

use anyhow::{Context, Result};
use log::{debug, info};
use std::path::PathBuf;
use std::process::Command;

use crate::benchmarks::{binary_exists, RepoSource, RepositoryManager};
use crate::config::GlobalConfig;
use crate::path_utils;

pub struct Builder {
    config: GlobalConfig,
    repo_manager: Option<RepositoryManager>,
}

impl Builder {
    pub fn new(config: GlobalConfig) -> Result<Self> {
        // Get the source path as a string, preserving URL format for remote repos
        let source_path_str = config.bench.global.source.to_string_lossy().to_string();

        debug!("Source path from config: {source_path_str}");

        // Check if we potentially have a URL in a local path format
        // This happens when the URL is incorrectly processed by path_utils
        let actual_source = if source_path_str.contains("/https:/")
            || source_path_str.contains("/http:/")
            || source_path_str.contains("/git:/")
        {
            // Extract the URL part from the path
            let parts: Vec<&str> = source_path_str.split('/').collect();
            let mut url_parts = Vec::new();
            let mut found_protocol = false;

            for part in parts {
                if part.contains(':') && (part == "https:" || part == "http:" || part == "git:") {
                    found_protocol = true;
                }

                if found_protocol {
                    url_parts.push(part);
                }
            }

            // Reconstruct the URL
            let url = url_parts.join("/");
            debug!("Extracted URL from path: {url}");
            url
        } else {
            source_path_str
        };

        // Create RepoSource based on the corrected source
        let repo_source = RepoSource::new(&actual_source);

        match &repo_source {
            RepoSource::Local(path) => {
                // For local repos, verify the path exists
                if !path.exists() {
                    anyhow::bail!("Source directory does not exist: {}", path.display());
                }

                debug!("Using local repository at: {}", path.display());
                // We don't need a repo manager for local repos
                Ok(Self {
                    config,
                    repo_manager: None,
                })
            }
            RepoSource::Remote(url) => {
                // For remote repos, create a repository manager
                info!("Using remote Git repository: {url}");
                // Use the scratch directory directly to avoid duplicate "repos" in path
                let scratch_dir = config.bench.global.scratch.clone();
                // Important: pass the raw URL string, not the processed path
                let repo_manager = RepositoryManager::builder(url, &scratch_dir).build()?;

                Ok(Self {
                    config,
                    repo_manager: Some(repo_manager),
                })
            }
        }
    }

    pub fn build(&mut self) -> Result<()> {
        debug!("Starting build");
        // If we're using a remote repository, ensure it's available
        let source_dir = if let Some(repo_manager) = &mut self.repo_manager {
            let repo_path = repo_manager.ensure_repository_available()?;
            repo_manager.validate_commits(&self.config.bench.global.commits)?;
            repo_path
        } else {
            // Using a local repository
            self.config.bench.global.source.clone()
        };

        debug!("Using source_dir: {source_dir:?}");

        // Verify this is a valid git repository before proceeding
        let git_dir = source_dir.join(".git");
        if !git_dir.exists() {
            anyhow::bail!("Not a valid git repository: {}", source_dir.display());
        }

        self.check_clean_worktree(&source_dir)?;
        // Get the initial reference to restore later
        let initial_ref = self.get_initial_ref(&source_dir)?;

        // Build all commits up-front
        for commit in &self.config.bench.global.commits {
            if !binary_exists(&self.config.app.bin_dir, commit) {
                info!("Building binary for commit {commit}");
                self.build_commit(&source_dir, commit)?;
            } else {
                info!("Binary already exists for commit {commit}, skipping build");
            };
        }

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
        self.run_build(source_dir, original_commit)?;
        self.copy_binary(original_commit)?;
        Ok(())
    }

    fn checkout_commit(&self, source_dir: &PathBuf, commit: &str) -> Result<()> {
        let status = Command::new("git")
            .current_dir(source_dir)
            .arg("checkout")
            .arg(commit)
            .status()
            .with_context(|| format!("Failed to checkout commit {commit}"))?;

        if !status.success() {
            anyhow::bail!("Git checkout failed for commit {}", commit);
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
            .join(format!("build-{commit_hash}"));

        info!("Making build dir: {dir:?}");
        path_utils::ensure_directory(&dir)?;
        let canonical_dir = dir.canonicalize()?;

        // cmake configuration
        let mut cmd = Command::new("cmake");
        cmd.current_dir(source_dir).arg("-B").arg(&canonical_dir);
        // Add custom build flags if configured
        if let Some(cmake_args) = &self.config.bench.global.cmake_build_args {
            for arg in cmake_args {
                cmd.arg(arg);
            }
        }
        let config_status = cmd
            .status()
            .with_context(|| format!("Failed to configure cmake for commit {commit_hash}"))?;
        if !config_status.success() {
            anyhow::bail!("CMake configuration failed for commit {}", commit_hash);
        }

        // cmake build
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
            .join(format!("build-{commit_hash}"));

        let src_path = dir.clone().join("bin/bitcoind");
        let dest_path = self
            .config
            .app
            .bin_dir
            .join(format!("bitcoind-{commit_hash}"));

        if let Some(parent) = dest_path.parent() {
            path_utils::ensure_directory(parent)?;
        }

        path_utils::copy_file(&src_path, &dest_path)
            .with_context(|| format!("Failed to copy binary for commit {commit_hash}"))?;
        // Clean up the build directory
        std::fs::remove_dir_all(dir.clone()).with_context(|| {
            format!("Failed to cleanup extracted files for commit {commit_hash} from {dir:?}")
        })?;

        Ok(())
    }

    fn restore_git_state(&self, source_dir: &PathBuf, initial_ref: &str) -> Result<()> {
        debug!("restoring git state of {source_dir:?}");
        let status = Command::new("git")
            .current_dir(source_dir)
            .arg("checkout")
            .arg(initial_ref)
            .status()
            .with_context(|| format!("Failed to restore git state to {initial_ref}"))?;

        if !status.success() {
            anyhow::bail!("Failed to restore git state");
        }
        Ok(())
    }
}

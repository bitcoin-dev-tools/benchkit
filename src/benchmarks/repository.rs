use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::process::Command;
use url::Url;

/// Repository source can be either local or remote
#[derive(Debug, Clone)]
pub enum RepoSource {
    /// Local directory path
    Local(PathBuf),
    /// Remote Git URL
    Remote(String),
}

impl RepoSource {
    /// Create a new RepoSource from a source string
    /// This will detect if the source is a URL or a local path
    pub fn new(source: &str) -> Self {
        // Try to parse as URL first
        if let Ok(url) = Url::parse(source) {
            // Valid URL schemes for Git
            if url.scheme() == "http" || url.scheme() == "https" || url.scheme() == "git" {
                return RepoSource::Remote(source.to_string());
            }
        }

        // Check for SSH-style Git URLs (git@github.com:user/repo.git)
        if source.starts_with("git@") && source.contains(':') && source.ends_with(".git") {
            return RepoSource::Remote(source.to_string());
        }

        // Otherwise, treat as local path
        RepoSource::Local(PathBuf::from(source))
    }

    /// Get a clean directory name for the repository
    /// This is used for creating cache directories
    pub fn get_cache_name(&self) -> String {
        match self {
            RepoSource::Local(path) => {
                // For local repos, use the directory name
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| "local_repo".to_string())
            }
            RepoSource::Remote(url) => {
                // For remote repos, sanitize the URL to create a valid directory name
                // Remove protocol and special characters
                let url_str = url.to_string();
                url_str
                    .replace("http://", "")
                    .replace("https://", "")
                    .replace("git://", "")
                    .replace("git@", "")
                    .replace([':', '/', '.'], "_")
            }
        }
    }
}

/// Manages Git repositories for benchmarking
pub struct RepositoryManager {
    /// The source of the repository (local or remote)
    source: RepoSource,
    /// Directory to store cloned repositories
    cache_dir: PathBuf,
    /// Path to the actual repository to use (either original local path or cloned path)
    repo_path: Option<PathBuf>,
}

impl RepositoryManager {
    /// Create a new repository manager
    pub fn new(source: &str, scratch_dir: &Path) -> Self {
        let repo_source = RepoSource::new(source);
        let cache_dir = scratch_dir.join("repos");

        Self {
            source: repo_source,
            cache_dir,
            repo_path: None,
        }
    }

    /// Ensure the repository is available locally
    /// If it's a remote repository, clone it if needed
    /// Returns the path to the repository to use for operations
    pub fn ensure_repository_available(&mut self) -> Result<PathBuf> {
        match &self.source {
            RepoSource::Local(path) => {
                // For local repos, just check if it exists and contains a .git directory
                if !path.exists() {
                    anyhow::bail!("Source directory does not exist: {}", path.display());
                }

                let git_dir = path.join(".git");
                if !git_dir.exists() {
                    anyhow::bail!(
                        "Source directory is not a Git repository: {}",
                        path.display()
                    );
                }

                self.repo_path = Some(path.clone());
                Ok(path.clone())
            }
            RepoSource::Remote(url) => {
                // For remote repos, check if we have it cached already
                let repo_name = self.source.get_cache_name();
                let repo_path = self.cache_dir.join(&repo_name);

                if repo_path.exists() {
                    debug!("Using cached repository: {}", repo_path.display());
                    // Repository already exists, just update it
                    self.update_repository(&repo_path)?;
                } else {
                    // Repository doesn't exist, clone it
                    info!("Cloning repository: {} to {}", url, repo_path.display());
                    self.clone_repository(url, &repo_path)?;
                }

                self.repo_path = Some(repo_path.clone());
                Ok(repo_path)
            }
        }
    }

    /// Clone a repository
    fn clone_repository(&self, url: &str, target_path: &Path) -> Result<()> {
        // Create parent directory if needed
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let status = Command::new("git")
            .arg("clone")
            .arg(url)
            .arg(target_path)
            .status()
            .context(format!("Failed to clone repository: {}", url))?;

        if !status.success() {
            anyhow::bail!("Git clone failed with status code: {}", status);
        }

        Ok(())
    }

    /// Update an existing repository
    fn update_repository(&self, repo_path: &Path) -> Result<()> {
        // Fetch latest changes
        let status = Command::new("git")
            .current_dir(repo_path)
            .arg("fetch")
            .status()
            .context(format!(
                "Failed to update repository: {}",
                repo_path.display()
            ))?;

        if !status.success() {
            warn!("Git fetch failed with status code: {}", status);
            // Continue anyway, as the repository might still be usable
        }

        Ok(())
    }

    /// Get the path to the repository
    pub fn get_repository_path(&self) -> Result<PathBuf> {
        self.repo_path.clone().ok_or_else(|| {
            anyhow::anyhow!("Repository not initialized. Call ensure_repository_available() first.")
        })
    }

    /// Validate that all required commits are available in the repository
    pub fn validate_commits(&self, commits: &[String]) -> Result<()> {
        let repo_path = self.repo_path.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Repository not initialized. Call ensure_repository_available() first.")
        })?;

        for commit in commits {
            // Try to get commit info to verify it exists
            let output = Command::new("git")
                .current_dir(repo_path)
                .arg("cat-file")
                .arg("-t")
                .arg(commit)
                .output()
                .context(format!("Failed to check commit: {}", commit))?;

            if !output.status.success() {
                anyhow::bail!("Commit not found in repository: {}", commit);
            }

            // Verify it's a commit object
            let obj_type = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if obj_type != "commit" {
                anyhow::bail!("Object is not a commit: {} (type: {})", commit, obj_type);
            }
        }

        Ok(())
    }
}

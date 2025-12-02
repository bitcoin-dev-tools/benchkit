use anyhow::{Context, Result};
use log::{debug, info};
use std::fs;
use std::path::Path;

use crate::benchmarks::hook_runner::HookArgs;

/// Base hook executor with common functionality
struct BaseHookExecutor;

impl BaseHookExecutor {
    /// Create a directory, including all parent directories
    fn create_directory(&self, path: &Path) -> Result<()> {
        debug!("Creating directory: {}", path.display());
        fs::create_dir_all(path)
            .with_context(|| format!("Failed to create directory: {}", path.display()))?;
        Ok(())
    }

    /// Clear all contents of a directory
    fn clear_directory(&self, path: &Path) -> Result<()> {
        debug!("Clearing directory contents: {}", path.display());

        if path.exists() {
            for entry in fs::read_dir(path)
                .with_context(|| format!("Failed to read directory: {}", path.display()))?
            {
                let entry = entry?;
                let entry_path = entry.path();

                if entry_path.is_dir() {
                    fs::remove_dir_all(&entry_path).with_context(|| {
                        format!("Failed to remove directory: {}", entry_path.display())
                    })?;
                } else {
                    fs::remove_file(&entry_path).with_context(|| {
                        format!("Failed to remove file: {}", entry_path.display())
                    })?;
                }
            }
        }

        Ok(())
    }

    /// Clear and recreate a directory
    fn clear_and_recreate_directory(&self, path: &Path) -> Result<()> {
        self.create_directory(path)?;
        self.clear_directory(path)?;
        Ok(())
    }
}

/// Standard hook executor for full initial block download
pub struct StandardHookExecutor {
    base: BaseHookExecutor,
}

impl StandardHookExecutor {
    pub fn new() -> Self {
        Self {
            base: BaseHookExecutor,
        }
    }
}

impl Default for StandardHookExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl StandardHookExecutor {
    pub fn setup(&self, args: &HookArgs) -> Result<()> {
        info!("Running setup hook");
        self.base.create_directory(&args.tmp_data_dir)?;
        self.base.clear_directory(&args.tmp_data_dir)?;

        Ok(())
    }

    pub fn prepare(&self, args: &HookArgs) -> Result<()> {
        info!("Running prepare hook");

        // Create datadir and clear contents
        self.base.clear_and_recreate_directory(&args.tmp_data_dir)?;

        Ok(())
    }

    pub fn conclude(&self, args: &HookArgs) -> Result<()> {
        info!("Running conclude hook");

        // Create output directory structure
        let output_path = args
            .out_dir
            .join(&args.commit)
            .join(&args.params_dir)
            .join(args.iteration.to_string());

        info!("Moving debug.log to {}", output_path.display());
        self.base.create_directory(&output_path)?;

        // Determine debug.log source path based on network
        let debug_log_source = if args.network == "main" {
            args.tmp_data_dir.join("debug.log")
        } else {
            args.tmp_data_dir.join(&args.network).join("debug.log")
        };

        let debug_log_dest = output_path.join("debug.log");

        // Move debug.log
        if debug_log_source.exists() {
            fs::rename(&debug_log_source, &debug_log_dest)
                .or_else(|_| -> Result<()> {
                    // If rename fails (e.g., cross-filesystem), fall back to copy and delete
                    fs::copy(&debug_log_source, &debug_log_dest)?;
                    fs::remove_file(&debug_log_source)?;
                    Ok(())
                })
                .with_context(|| {
                    format!(
                        "Failed to move debug.log from {} to {}",
                        debug_log_source.display(),
                        debug_log_dest.display()
                    )
                })?;
        } else {
            debug!("debug.log not found at {}", debug_log_source.display());
        }

        // Clean datadir contents
        self.base.clear_directory(&args.tmp_data_dir)?;

        Ok(())
    }

    pub fn cleanup(&self, args: &HookArgs) -> Result<()> {
        info!("Running cleanup hook");

        // Final cleanup of datadir
        self.base.clear_directory(&args.tmp_data_dir)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_create_directory() {
        let base_executor = BaseHookExecutor;
        let temp_dir = tempdir().unwrap();
        let test_path = temp_dir.path().join("test").join("nested").join("dir");

        base_executor.create_directory(&test_path).unwrap();
        assert!(test_path.exists());
    }

    #[test]
    fn test_clear_directory() {
        let base_executor = BaseHookExecutor;
        let temp_dir = tempdir().unwrap();
        let test_path = temp_dir.path();

        // Create some test files and directories
        let file_path = test_path.join("test_file.txt");
        fs::write(&file_path, "test content").unwrap();

        let dir_path = test_path.join("test_dir");
        fs::create_dir(&dir_path).unwrap();
        let nested_file = dir_path.join("nested.txt");
        fs::write(&nested_file, "nested content").unwrap();

        // Clear the directory
        base_executor.clear_directory(test_path).unwrap();

        // Verify everything is removed
        assert!(!file_path.exists());
        assert!(!dir_path.exists());

        // Directory itself should still exist
        assert!(test_path.exists());
    }
}

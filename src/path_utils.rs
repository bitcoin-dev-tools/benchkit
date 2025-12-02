use anyhow::{Context, Result};
use log::debug;
use std::path::{Path, PathBuf};

/// Expand environment variables in a path string
pub fn expand_path_str(path: &str) -> String {
    shellexpand::full(path)
        .unwrap_or_else(|_| path.into())
        .into_owned()
}

/// Expand a PathBuf with environment variables
pub fn expand_path_buf(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    PathBuf::from(expand_path_str(&path_str))
}

/// Create a directory and all parent directories if they don't exist
pub fn ensure_directory(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)
            .with_context(|| format!("Failed to create directory: {path:?}"))?;
        debug!("Created directory: {path:?}");
    }
    Ok(())
}

/// Resolve a path to an absolute path, creating directories if needed
pub fn resolve_path(path: &Path, config_dir: &Path, create_dirs: bool) -> Result<PathBuf> {
    // First expand any environment variables
    let expanded_path = expand_path_buf(path);

    // Determine if path is absolute or needs to be made relative to config_dir
    let abs_path = if expanded_path.is_absolute() {
        expanded_path
    } else {
        config_dir.join(&expanded_path)
    };

    if create_dirs {
        ensure_directory(&abs_path)?;
    }

    // Get the canonical path to resolve any symlinks or .. components
    let canonical = abs_path
        .canonicalize()
        .with_context(|| format!("Failed to resolve path: {abs_path:?}"))?;

    Ok(canonical)
}

/// Process multiple paths at once, resolving them all relative to a config directory
pub fn process_paths(
    paths: &mut [&mut PathBuf],
    config_dir: &Path,
    create_dirs: bool,
) -> Result<()> {
    for path in paths.iter_mut() {
        **path = expand_path_buf(path);
        **path = resolve_path(path, config_dir, create_dirs)?;
    }
    Ok(())
}

/// Make a clean output directory, ensuring it exists and is empty
pub fn prepare_output_directory(dir: &Path) -> Result<()> {
    ensure_directory(dir)?;

    // Check if empty
    if std::fs::read_dir(dir)?.next().is_some() {
        anyhow::bail!(
            "Output directory '{}' is not empty. Please clear it before running benchmarks",
            dir.display()
        );
    }

    Ok(())
}

/// Convenience function to copy a file with better error handling
pub fn copy_file(source: &Path, dest: &Path) -> Result<()> {
    std::fs::copy(source, dest)
        .with_context(|| format!("Failed to copy {source:?} to {dest:?}"))?;
    debug!("Copied {source:?} to {dest:?}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    #[test]
    fn test_expand_path_str() {
        // Test with no environment variables
        assert_eq!(expand_path_str("/tmp/test"), "/tmp/test");

        // With one variable
        env::set_var("TEST_PATH_STR", "/test/path");
        let result = expand_path_str("$TEST_PATH_STR/file");
        assert!(result.contains("/test/path/file"));
        env::remove_var("TEST_PATH_STR");

        // With HOME variable (if available)
        if let Ok(home) = env::var("HOME") {
            let result = expand_path_str("~/file");
            assert!(result.contains(&format!("{}/file", home)));
        }
    }

    #[test]
    fn test_expand_path_buf() {
        // Test with no environment variables
        assert_eq!(
            expand_path_buf(Path::new("/tmp/test")),
            PathBuf::from("/tmp/test")
        );

        // with env vars (use unique name to avoid race with parallel tests)
        env::set_var("TEST_PATH_BUF", "/test/path");
        let result = expand_path_buf(Path::new("$TEST_PATH_BUF/file"));
        assert!(result.to_string_lossy().contains("/test/path/file"));
        env::remove_var("TEST_PATH_BUF");
    }

    #[test]
    fn test_ensure_directory() {
        let tempdir = tempdir().unwrap();
        let test_dir = tempdir.path().join("test_dir");
        let nested_dir = test_dir.join("nested").join("path");

        // Test creating a directory
        ensure_directory(&test_dir).unwrap();
        assert!(test_dir.exists());
        assert!(test_dir.is_dir());

        // Nested dirs
        ensure_directory(&nested_dir).unwrap();
        assert!(nested_dir.exists());
        assert!(nested_dir.is_dir());

        // Existing dir
        ensure_directory(&test_dir).unwrap();
        assert!(test_dir.exists());
    }

    #[test]
    fn test_resolve_path() {
        let tempdir = tempdir().unwrap();
        let config_dir = tempdir.path();

        // Test with absolute path
        let abs_path = config_dir.join("abs_test");
        let resolved = resolve_path(&abs_path, config_dir, true).unwrap();
        assert!(resolved.is_absolute());
        assert!(abs_path.exists());

        // with relative path
        let rel_path = PathBuf::from("rel_test");
        let resolved = resolve_path(&rel_path, config_dir, true).unwrap();
        assert!(resolved.is_absolute());
        assert!(config_dir.join(rel_path).exists());

        // with nested relative path
        let nested_rel_path = PathBuf::from("nested/rel/test");
        let resolved = resolve_path(&nested_rel_path, config_dir, true).unwrap();
        assert!(resolved.is_absolute());
        assert!(config_dir.join(nested_rel_path).exists());

        // without directory creation
        let no_create_path = PathBuf::from("no_create");
        let result = resolve_path(&no_create_path, config_dir, false);
        assert!(result.is_err()); // Should fail as the directory doesn't exist
    }

    #[test]
    fn test_process_paths() {
        let tempdir = tempdir().unwrap();
        let config_dir = tempdir.path();
        let mut path1 = PathBuf::from("path1");
        let mut path2 = PathBuf::from("path2");
        let mut path3 = PathBuf::from("nested/path3");

        let mut paths = vec![&mut path1, &mut path2, &mut path3];
        process_paths(&mut paths, config_dir, true).unwrap();

        // Verify each path is now absolute and directories exist
        for path in &paths {
            assert!(path.is_absolute());
            assert!(path.exists());
            assert!(path.is_dir());
        }
    }

    #[test]
    fn test_prepare_output_directory() {
        let tempdir = tempdir().unwrap();
        let output_dir = tempdir.path().join("output");

        // Test creating output directory
        prepare_output_directory(&output_dir).unwrap();
        assert!(output_dir.exists());
        assert!(output_dir.is_dir());

        // With non-empty directory
        let file_path = output_dir.join("test_file.txt");
        let mut file = fs::File::create(&file_path).unwrap();
        file.write_all(b"test content").unwrap();

        // Should fail as directory is not empty
        let result = prepare_output_directory(&output_dir);
        assert!(result.is_err());

        // Clean up
        fs::remove_file(file_path).unwrap();

        // Should succeed now
        prepare_output_directory(&output_dir).unwrap();
    }

    #[test]
    fn test_copy_file() {
        let tempdir = tempdir().unwrap();
        let source_path = tempdir.path().join("source.txt");
        let dest_path = tempdir.path().join("dest.txt");

        // Verify file is copied correctly
        let content = b"test content for copy";
        let mut file = fs::File::create(&source_path).unwrap();
        file.write_all(content).unwrap();
        copy_file(&source_path, &dest_path).unwrap();
        assert!(dest_path.exists());
        let copied_content = fs::read_to_string(&dest_path).unwrap();
        assert_eq!(copied_content, String::from_utf8_lossy(content));

        // Test copying non-existent file
        let nonexistent = tempdir.path().join("nonexistent.txt");
        let result = copy_file(&nonexistent, &dest_path);
        assert!(result.is_err());
    }
}

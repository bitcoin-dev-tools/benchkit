use anyhow::{Context, Result};
use log::{debug, info};
use std::collections::HashMap;
use std::fmt::Debug;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Output, Stdio};

#[cfg(target_os = "linux")]
use crate::cpu_binding::CpuBinder;

/// Command execution context
#[derive(Debug, Clone, Default)]
pub struct CommandContext {
    /// Name of the command for logging
    pub command_name: Option<String>,
    /// Current working directory
    pub working_dir: Option<String>,
    /// Environment variables to set
    pub env_vars: HashMap<String, String>,
    /// CPU cores to bind the command to
    pub cpu_cores: Option<String>,
    /// Whether to create a process group
    pub process_group: bool,
    /// Capture output
    pub capture_output: bool,
    /// Allow command to fail without returning an error
    pub allow_failure: bool,
}

/// Builder for CommandExecutor
pub struct CommandExecutorBuilder {
    context: CommandContext,
}

impl Default for CommandExecutorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandExecutorBuilder {
    /// Create a new CommandExecutorBuilder with default settings
    pub fn new() -> Self {
        Self {
            context: CommandContext::default(),
        }
    }

    /// Set CPU cores to bind the command to
    pub fn cpu_cores(mut self, cores: Option<String>) -> Self {
        self.context.cpu_cores = cores;
        self
    }

    /// Set whether to capture command output
    pub fn capture_output(mut self, capture: bool) -> Self {
        self.context.capture_output = capture;
        self
    }

    /// Set the working directory
    pub fn working_dir<P: AsRef<Path>>(mut self, dir: Option<P>) -> Self {
        self.context.working_dir = dir.map(|d| d.as_ref().to_string_lossy().to_string());
        self
    }

    /// Set whether to create a process group
    pub fn process_group(mut self, create_group: bool) -> Self {
        self.context.process_group = create_group;
        self
    }

    /// Add environment variables
    pub fn env_vars(mut self, vars: HashMap<String, String>) -> Self {
        self.context.env_vars.extend(vars);
        self
    }

    /// Add a single environment variable
    pub fn env_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.env_vars.insert(key.into(), value.into());
        self
    }

    /// Set whether to allow command failures without returning an error
    pub fn allow_failure(mut self, allow: bool) -> Self {
        self.context.allow_failure = allow;
        self
    }

    /// Set a name for the command for logging purposes
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.context.command_name = Some(name.into());
        self
    }

    /// Build the CommandExecutor
    pub fn build(self) -> Result<CommandExecutor> {
        // Validate the configuration
        // As an example, we might validate cores format, but we'll leave
        // more complex validation to the CpuBinder implementation

        Ok(CommandExecutor {
            context: self.context,
        })
    }
}

/// A unified interface for executing commands
pub struct CommandExecutor {
    context: CommandContext,
}

impl Default for CommandExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandExecutor {
    /// Create a new CommandExecutor with default settings
    pub fn new() -> Self {
        Self {
            context: CommandContext::default(),
        }
    }

    /// Create a builder for CommandExecutor with fluent configuration
    pub fn builder() -> CommandExecutorBuilder {
        CommandExecutorBuilder::new()
    }

    /// Create a CommandExecutor with a specific context
    pub fn with_context(context: CommandContext) -> Self {
        Self { context }
    }

    /// Bind the current process to specified CPU cores
    pub fn bind_current_process_to_cores(cores: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            let mut cpu_binder = CpuBinder::new()?;
            info!("Binding current process to cores: {}", cores);
            cpu_binder.bind_current_process_to_cores(cores)?;
        }

        #[cfg(not(target_os = "linux"))]
        {
            info!("CPU binding is not supported on this platform, skipping (cores: {cores})");
        }

        Ok(())
    }

    // Functions removed as they've been replaced by the builder pattern

    /// Execute a shell command line and wait for it to complete, returning the output
    pub fn execute_shell(&self, cmd_line: &str) -> Result<Output> {
        self.execute_command_with_args("sh", &["-c", cmd_line])
    }

    /// Execute a command with arguments and wait for it to complete, returning the output
    pub fn execute_command_with_args(&self, cmd: &str, args: &[&str]) -> Result<Output> {
        let child = self.launch_command(cmd, args)?;

        let output = child.wait_with_output().with_context(|| {
            format!(
                "Failed to wait for command completion: {}",
                self.format_command(cmd, args)
            )
        })?;

        if !output.status.success() && !self.context.allow_failure {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!(
                "Command failed with status {}: {}\nStderr: {}",
                output.status.code().unwrap_or(-1),
                self.format_command(cmd, args),
                stderr
            ));
        }

        Ok(output)
    }

    /// Execute a command and launch it, returning the child process handle
    pub fn launch_command(&self, cmd: &str, args: &[&str]) -> Result<Child> {
        let command_str = self.format_command(cmd, args);
        debug!("Launching command: {command_str}");

        let mut command = Command::new(cmd);
        command.args(args);

        // Set working directory if specified
        if let Some(dir) = &self.context.working_dir {
            command.current_dir(dir);
        }

        // Add environment variables
        for (key, value) in &self.context.env_vars {
            command.env(key, value);
        }

        // Configure output capturing
        if self.context.capture_output {
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
        } else {
            command.stdout(Stdio::null()).stderr(Stdio::null());
        }

        // Create process group if requested
        if self.context.process_group {
            command.process_group(0);
        }

        // Spawn the command
        let child = command
            .spawn()
            .with_context(|| format!("Failed to spawn command: {command_str}"))?;

        // Apply CPU affinity if specified
        if let Some(cores) = &self.context.cpu_cores {
            self.apply_cpu_affinity(&child, cores)?;
        }

        Ok(child)
    }

    /// Format command and arguments for logging
    fn format_command(&self, cmd: &str, args: &[&str]) -> String {
        if let Some(name) = &self.context.command_name {
            return name.clone();
        }

        format!("{} {}", cmd, args.join(" "))
    }

    /// Apply CPU affinity to a process
    fn apply_cpu_affinity(&self, child: &Child, cores: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            let pid = child.id() as libc::pid_t;
            debug!("Binding process with PID {} to cores: {}", pid, cores);

            // Create a new CPU binder for this operation
            let mut cpu_binder = CpuBinder::new()?;

            // First bind the individual process to the specified cores
            cpu_binder.bind_pid_to_cores(pid, cores)?;

            // If we created a process group, try to bind the whole group too
            if self.context.process_group {
                let pgid = -pid; // Negative PID means process group in Linux scheduling APIs

                // Check if the process group is properly established before attempting to bind
                let group_exists = unsafe { libc::getpgid(pid) } == pid;

                if group_exists {
                    // Use a separate block to capture any errors but continue execution
                    match cpu_binder.bind_pid_to_cores(pgid, cores) {
                        Ok(_) => debug!(
                            "Successfully bound process group {} to cores {}",
                            pid, cores
                        ),
                        Err(err) => {
                            // Log the error but continue - individual process binding is already done
                            debug!("Process group binding failed (non-critical): {}", err);
                            debug!("Individual process binding was successful and should be inherited by children");
                        }
                    }
                } else {
                    debug!("Process group not yet established, skipping group binding (individual binding will be inherited)");
                }
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = child; // Suppress unused variable warning
            debug!("CPU binding is not supported on this platform, skipping (cores: {cores})");
        }

        Ok(())
    }

    /// Execute a command and check its exit status
    pub fn execute_check_status(&self, cmd: &str, args: &[&str]) -> Result<ExitStatus> {
        let output = self.execute_command_with_args(cmd, args)?;
        Ok(output.status)
    }

    /// Execute multiple commands sequentially
    pub fn execute_sequence(&self, commands: &[(&str, Vec<&str>)]) -> Result<Vec<Output>> {
        let mut results = Vec::with_capacity(commands.len());

        for (cmd, args) in commands {
            let output = self.execute_command_with_args(cmd, args)?;
            results.push(output);
        }

        Ok(results)
    }

    /// Convert a command context to options - for backward compatibility
    pub fn context_to_options(context: &CommandContext) -> CommandOptions {
        CommandOptions {
            capture_output: context.capture_output,
            cpu_cores: context.cpu_cores.clone(),
            process_group: context.process_group,
            working_dir: context.working_dir.clone(),
            env_vars: context.env_vars.clone(),
            allow_failure: context.allow_failure,
            command_name: context.command_name.clone(),
        }
    }

    /// Convert options to a command context - for backward compatibility
    pub fn options_to_context(options: &CommandOptions) -> CommandContext {
        CommandContext {
            capture_output: options.capture_output,
            cpu_cores: options.cpu_cores.clone(),
            process_group: options.process_group,
            working_dir: options.working_dir.clone(),
            env_vars: options.env_vars.clone(),
            allow_failure: options.allow_failure,
            command_name: options.command_name.clone(),
        }
    }
}

/// Options for command execution (for backward compatibility)
#[derive(Debug, Clone)]
pub struct CommandOptions {
    /// Whether to capture the command's output
    pub capture_output: bool,
    /// CPU cores to bind to (comma-separated list or range, like "0,1,2" or "0-3")
    pub cpu_cores: Option<String>,
    /// Whether to create a process group for the command
    pub process_group: bool,
    /// Current working directory
    pub working_dir: Option<String>,
    /// Environment variables to set
    pub env_vars: HashMap<String, String>,
    /// Allow command to fail without returning an error
    pub allow_failure: bool,
    /// Command name for logging
    pub command_name: Option<String>,
}

impl Default for CommandOptions {
    fn default() -> Self {
        Self {
            capture_output: true,
            cpu_cores: None,
            process_group: false,
            working_dir: None,
            env_vars: HashMap::new(),
            allow_failure: false,
            command_name: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_builder() {
        let executor = CommandExecutor::builder()
            .name("test command")
            .working_dir(Some("/tmp"))
            .cpu_cores(Some("0-1".to_string()))
            .capture_output(true)
            .process_group(true)
            .env_var("TEST_VAR", "test_value")
            .allow_failure(true)
            .build()
            .unwrap();

        assert_eq!(
            executor.context.command_name,
            Some("test command".to_string())
        );
        assert_eq!(executor.context.working_dir, Some("/tmp".to_string()));
        assert_eq!(executor.context.cpu_cores, Some("0-1".to_string()));
        assert!(executor.context.capture_output);
        assert!(executor.context.process_group);
        assert_eq!(
            executor.context.env_vars.get("TEST_VAR"),
            Some(&"test_value".to_string())
        );
        assert!(executor.context.allow_failure);
    }

    #[test]
    fn test_execute_shell_success() {
        let executor = CommandExecutor::builder()
            .capture_output(true)
            .build()
            .unwrap();

        let output = executor.execute_shell("echo 'hello world'").unwrap();
        assert!(output.status.success());

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("hello world"));
    }

    #[test]
    fn test_execute_command_with_args() {
        let executor = CommandExecutor::builder()
            .capture_output(true)
            .build()
            .unwrap();

        let output = executor
            .execute_command_with_args("echo", &["test", "arguments"])
            .unwrap();
        assert!(output.status.success());

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("test arguments"));
    }

    #[test]
    fn test_execute_with_env_vars() {
        let executor = CommandExecutor::builder()
            .capture_output(true)
            .env_var("TEST_ENV_VAR", "test_value")
            .build()
            .unwrap();

        // Use shell command to echo an environment variable
        let output = executor.execute_shell("echo $TEST_ENV_VAR").unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("test_value"));
    }

    #[test]
    fn test_command_failure_handling() {
        // Test with allow_failure = false (default)
        let strict_executor = CommandExecutor::builder()
            .capture_output(true)
            .build()
            .unwrap();

        // This command should fail
        let result = strict_executor.execute_shell("false");
        assert!(result.is_err());

        // Test with allow_failure = true
        let lenient_executor = CommandExecutor::builder()
            .capture_output(true)
            .allow_failure(true)
            .build()
            .unwrap();

        // This command should fail but not return an error
        let result = lenient_executor.execute_shell("false");
        assert!(result.is_ok());
        assert!(!result.unwrap().status.success());
    }

    #[test]
    fn test_format_command() {
        // Test with command name
        let named_executor = CommandExecutor::builder()
            .name("test command")
            .build()
            .unwrap();

        assert_eq!(
            named_executor.format_command("echo", &["hello", "world"]),
            "test command"
        );

        // Test without command name
        let unnamed_executor = CommandExecutor::builder().build().unwrap();
        assert_eq!(
            unnamed_executor.format_command("echo", &["hello", "world"]),
            "echo hello world"
        );
    }
}

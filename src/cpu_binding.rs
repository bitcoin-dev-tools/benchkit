use anyhow::{Context, Result};
use hwloc::{CpuSet, ObjectType, Topology, CPUBIND_PROCESS};
use log::{debug, info};

/// Utility for CPU binding
pub struct CpuBinder {
    topology: Topology,
}

impl CpuBinder {
    pub fn new() -> Result<Self> {
        Ok(Self {
            topology: Topology::new(),
        })
    }

    /// Bind the current process to specified cores
    pub fn bind_current_process_to_cores(&mut self, cores_spec: &str) -> Result<()> {
        let pid = unsafe { libc::getpid() };
        let cpuset = self.parse_cores_spec(cores_spec)?;

        info!("Binding current runner process with PID {pid} to cores: {cores_spec}");
        debug!(
            "PID {pid} affinity before binding: {:?}",
            self.topology
                .get_cpubind_for_process(pid, CPUBIND_PROCESS)
                .unwrap_or_default()
        );

        match self
            .topology
            .set_cpubind_for_process(pid, cpuset, CPUBIND_PROCESS)
        {
            Ok(_) => {}
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Failed to bind process to specified cores: {:?}",
                    e
                ))
            }
        };

        debug!(
            "PID {pid} affinity after binding: {:?}",
            self.topology
                .get_cpubind_for_process(pid, CPUBIND_PROCESS)
                .unwrap_or_default()
        );

        Ok(())
    }

    /// Parse a core specification string (e.g. "0-3,5,7-9") into a CpuSet
    /// Roughyl mirrors taskset syntax
    fn parse_cores_spec(&self, cores_spec: &str) -> Result<CpuSet> {
        let mut cpuset = CpuSet::new();

        // Split by commas
        for part in cores_spec.split(',') {
            if part.contains('-') {
                // Handle ranges like "0-3"
                let range: Vec<&str> = part.split('-').collect();
                if range.len() != 2 {
                    anyhow::bail!("Invalid core range specification: {}", part);
                }

                let start = range[0]
                    .parse::<u32>()
                    .with_context(|| format!("Invalid core number: {}", range[0]))?;
                let end = range[1]
                    .parse::<u32>()
                    .with_context(|| format!("Invalid core number: {}", range[1]))?;

                for core in start..=end {
                    cpuset.set(core);
                }
            } else {
                // Handle single core like "5"
                let core = part
                    .parse::<u32>()
                    .with_context(|| format!("Invalid core number: {}", part))?;
                cpuset.set(core);
            }
        }

        Ok(cpuset)
    }

    /// Get information about available cores
    pub fn get_core_info(&self) -> String {
        let mut result = String::new();

        let core_depth = self
            .topology
            .depth_or_below_for_type(&ObjectType::Core)
            .unwrap_or_else(|_| self.topology.depth() - 1);

        let all_cores = self.topology.objects_at_depth(core_depth);

        result.push_str(&format!("Found {} cores:\n", all_cores.len()));

        for (i, core) in all_cores.iter().enumerate() {
            if let Some(cpuset) = core.cpuset() {
                result.push_str(&format!("Core {}: CPU IDs {:?}\n", i, cpuset));
            }
        }

        result
    }

    /// Bind a specified process ID or process group ID to cores
    pub fn bind_pid_to_cores(&mut self, pid: libc::pid_t, cores_spec: &str) -> Result<()> {
        let cpuset = self.parse_cores_spec(cores_spec)?;

        if pid < 0 {
            // This is a process group ID (negative PID means process group in Linux)
            let pgid = -pid; // Convert back to positive number for display
            info!(
                "Binding process group with PGID {} to cores: {}",
                pgid, cores_spec
            );

            // We can't get the current binding for a process group, so we skip the "before" debug

            // For process groups, we need to use a raw syscall as hwloc doesn't support it directly
            // In Linux, -pid in sched_setaffinity() refers to all processes in process group |pid|

            // Create a libc CPU set from our hwloc CpuSet
            let mut cpu_set: libc::cpu_set_t = unsafe { std::mem::zeroed() };

            // Walk through our CpuSet and set bits in the libc cpu_set_t
            for i in 0..1024 {
                // Standard Linux supports up to 1024 CPUs
                if cpuset.is_set(i as u32) {
                    unsafe {
                        libc::CPU_SET(i, &mut cpu_set);
                    }
                }
            }

            // Safety: sched_setaffinity is a Linux system call that sets the CPU affinity
            // The first arg is the process ID to set affinity for (negative = process group)
            let result = unsafe {
                libc::sched_setaffinity(
                    pid,
                    std::mem::size_of::<libc::cpu_set_t>() as libc::size_t,
                    &cpu_set as *const libc::cpu_set_t,
                )
            };

            if result != 0 {
                let err = std::io::Error::last_os_error();
                return Err(anyhow::anyhow!(
                    "Failed to bind process group to cores: {:?} (code: {}, raw pgid: {})",
                    err,
                    err.raw_os_error().unwrap_or(-1),
                    pid
                ));
            }

            info!(
                "Successfully bound process group PGID {} to cores: {}",
                pgid, cores_spec
            );
        } else {
            // This is a regular process ID
            info!("Binding process with PID {} to cores: {}", pid, cores_spec);
            debug!(
                "Before binding: {:?}",
                self.topology
                    .get_cpubind_for_process(pid, CPUBIND_PROCESS)
                    .unwrap_or_default()
            );

            match self
                .topology
                .set_cpubind_for_process(pid, cpuset, CPUBIND_PROCESS)
            {
                Ok(_) => {}
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Failed to bind process to specified cores: {:?}",
                        e
                    ))
                }
            };

            debug!(
                "After binding: {:?}",
                self.topology
                    .get_cpubind_for_process(pid, CPUBIND_PROCESS)
                    .unwrap_or_default()
            );
        }

        Ok(())
    }
}

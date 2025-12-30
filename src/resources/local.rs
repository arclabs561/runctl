//! Local process listing

use crate::error::Result;
use sysinfo::{Pid, System};

/// List local training processes
pub async fn list_local_processes(detailed: bool) -> Result<()> {
    println!("\n💻 Local Training Processes:");
    println!("{}", "-".repeat(80));

    // Use sysinfo for native Rust process listing
    let mut system = System::new_all();
    system.refresh_all();

    let current_pid = Pid::from_u32(std::process::id());
    let mut found = false;

    for (pid, process) in system.processes() {
        // Skip current process
        if *pid == current_pid {
            continue;
        }

        let cmd: Vec<String> = process
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        let cmd_str = cmd.join(" ");

        // Filter out system processes
        if cmd_str.contains("runctl")
            || cmd_str.contains("ps aux")
            || cmd_str.contains("runpodctl")
            || cmd_str.contains("ripgrep")
            || cmd_str.contains("mypy")
            || cmd_str.contains("lsp_server")
            || cmd_str.contains("Cursor.app")
        {
            continue;
        }

        // Look for actual training scripts: Python scripts with train/epoch keywords, or .py files
        let is_training = (cmd_str.contains(".py")
            && (cmd_str.contains("train")
                || cmd_str.contains("epoch")
                || cmd_str.contains("training")))
            || (cmd_str.contains("python")
                && cmd_str.contains(".py")
                && (cmd_str.contains("train") || cmd_str.contains("epoch")));

        if is_training {
            found = true;
            let cpu_usage = process.cpu_usage();
            let memory_mb = process.memory() as f64 / 1024.0 / 1024.0;
            if detailed {
                println!(
                    "  PID: {}  CPU: {:.1}%  MEM: {:.1}MB  CMD: {}",
                    pid.as_u32(),
                    cpu_usage,
                    memory_mb,
                    cmd_str
                );
            } else {
                println!("  PID: {}  CMD: {}", pid.as_u32(), cmd_str);
            }
        }
    }

    if !found {
        println!("  No training processes found");
    }

    Ok(())
}


//! Utilities (shell command execution, etc.).

use std::process::Command;

pub fn run_command(cmd: &str) {
    if cfg!(windows) {
        // Prefer PowerShell if PSModulePath looks present
        let ps_paths = std::env::var("PSModulePath").unwrap_or_default();
        let is_powershell = ps_paths.split(std::path::MAIN_SEPARATOR).count() >= 3;
        if is_powershell {
            let _ = Command::new("powershell.exe").args(["-Command", cmd]).status();
        } else {
            let _ = Command::new("cmd.exe").args(["/c", cmd]).status();
        }
    } else {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let _ = Command::new(shell).arg("-c").arg(cmd).status();
    }
}

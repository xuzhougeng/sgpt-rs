//! Utilities (shell command execution, etc.).

use std::process::Command;

pub fn run_command(cmd: &str) {
    if cfg!(windows) {
        // Allow explicit override via SHELL_NAME
        let override_shell = std::env::var("SHELL_NAME").unwrap_or_default().to_ascii_lowercase();
        let prefer_ps = if override_shell.contains("powershell") {
            true
        } else if override_shell.contains("cmd") {
            false
        } else {
            // Fallback heuristic: if PSModulePath exists, prefer PowerShell; otherwise cmd
            !std::env::var("PSModulePath").unwrap_or_default().is_empty()
        };
        if prefer_ps {
            let _ = Command::new("powershell.exe")
                .args(["-NoLogo", "-NoProfile", "-Command", cmd])
                .status();
        } else {
            let _ = Command::new("cmd.exe").args(["/c", cmd]).status();
        }
    } else {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let _ = Command::new(shell).arg("-c").arg(cmd).status();
    }
}

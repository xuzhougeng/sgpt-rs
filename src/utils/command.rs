//! Shell command execution utilities.

use std::process::Command;

/// Execute a shell command using the appropriate shell for the current platform.
/// 
/// On Windows: Uses PowerShell if available (determined by PSModulePath), otherwise cmd.exe
/// On Unix-like systems: Uses the shell specified by SHELL environment variable, or /bin/sh as fallback
/// 
/// # Examples
/// 
/// ```rust
/// use crate::utils::command::run_command;
/// 
/// run_command("echo 'Hello World'");
/// ```
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
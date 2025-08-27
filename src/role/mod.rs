//! Roles subsystem and default role strings.

use std::path::Path;

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultRole {
    Default,
    Shell,
    DescribeShell,
    Code,
}

impl DefaultRole {
    pub fn from_flags(shell: bool, describe: bool, code: bool) -> Self {
        if shell {
            return Self::Shell;
        }
        if describe {
            return Self::DescribeShell;
        }
        if code {
            return Self::Code;
        }
        Self::Default
    }
}

pub fn default_role_text(cfg: &Config, role: DefaultRole) -> String {
    let (os, shell) = (detect_os(cfg), detect_shell(cfg));
    match role {
        DefaultRole::Default => format!(
            "You are programming and system administration assistant.\nYou are managing {os} operating system with {shell} shell.\nProvide short responses in about 100 words, unless you are specifically asked for more details.\nIf you need to store any data, assume it will be stored in the conversation."
        ),
        DefaultRole::Shell => format!(
            "Provide only {shell} commands for {os} without any description.\nIf there is a lack of details, provide most logical solution.\nEnsure the output is a valid shell command.\nIf multiple steps required try to combine them together using &&.\nProvide only plain text without Markdown formatting.\nDo not provide markdown formatting such as ```."
        ),
        DefaultRole::DescribeShell =>
            "Provide a terse, single sentence description of the given shell command.\nDescribe each argument and option of the command.\nProvide short responses in about 80 words.".to_string(),
        DefaultRole::Code =>
            "Provide only code as output without any description.\nProvide only code in plain text format without Markdown formatting.\nDo not include symbols such as ``` or ```python.\nIf there is a lack of details, provide most logical solution.\nYou are not allowed to ask for more details.\nFor example if the prompt is \"Hello world Python\", you should return \"print('Hello world')\".".to_string(),
    }
}

fn detect_os(cfg: &Config) -> String {
    if let Some(v) = cfg.get("OS_NAME") { if v != "auto" { return v; } }
    match std::env::consts::OS {
        "linux" => "Linux".to_string(),
        "macos" => "Darwin/MacOS".to_string(),
        "windows" => format!("Windows {}", std::env::var("OS").unwrap_or_default()),
        other => other.to_string(),
    }
}

fn detect_shell(cfg: &Config) -> String {
    if let Some(v) = cfg.get("SHELL_NAME") { if v != "auto" { return v; } }
    if cfg!(windows) {
        let ps = std::env::var("PSModulePath").unwrap_or_default();
        let is_powershell = ps.split(std::path::MAIN_SEPARATOR).count() >= 3;
        return if is_powershell { "powershell.exe".into() } else { "cmd.exe".into() };
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    Path::new(&shell)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or(shell)
}

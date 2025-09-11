//! Roles subsystem: default role strings and persistent SystemRole store.

use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

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
        DefaultRole::Shell => {
            let ch = chain_hint(&shell);
            let ph = platform_hint(&shell);
            format!(
                "Provide only {shell} commands for {os} without any description.\nIf there is a lack of details, provide most logical solution.\nEnsure the output is a valid shell command.\n{ch}\n{ph}\nProvide only plain text without Markdown formatting.\nDo not provide markdown formatting such as ```."
            )
        }
        DefaultRole::DescribeShell =>
            "Provide a terse, single sentence description of the given shell command.\nDescribe each argument and option of the command.\nProvide short responses in about 80 words.".to_string(),
        DefaultRole::Code =>
            "Provide only code as output without any description.\nProvide only code in plain text format without Markdown formatting.\nDo not include symbols such as ``` or ```python.\nIf there is a lack of details, provide most logical solution.\nYou are not allowed to ask for more details.\nFor example if the prompt is \"Hello world Python\", you should return \"print('Hello world')\".".to_string(),
    }
}

fn chain_hint(shell: &str) -> String {
    let sh = shell.to_ascii_lowercase();
    if sh.contains("powershell") {
        "If multiple steps are required, separate commands with ; (not &&).".into()
    } else if sh.contains("cmd") {
        "If multiple steps are required, combine commands with &&.".into()
    } else {
        "If multiple steps are required, combine commands with &&.".into()
    }
}

fn platform_hint(shell: &str) -> String {
    let sh = shell.to_ascii_lowercase();
    if sh.contains("powershell") {
        "Prefer native PowerShell cmdlets and parameters (e.g., Get-ChildItem, Select-String) rather than Unix commands."
            .into()
    } else if sh.contains("cmd") {
        "Prefer built-in Windows commands (e.g., dir, findstr) where appropriate.".into()
    } else {
        String::new()
    }
}

fn detect_os(cfg: &Config) -> String {
    if let Some(v) = cfg.get("OS_NAME") {
        if v != "auto" {
            return v;
        }
    }
    match std::env::consts::OS {
        "linux" => "Linux".to_string(),
        "macos" => "Darwin/MacOS".to_string(),
        "windows" => format!("Windows {}", std::env::var("OS").unwrap_or_default()),
        other => other.to_string(),
    }
}

fn detect_shell(cfg: &Config) -> String {
    if let Some(v) = cfg.get("SHELL_NAME") {
        if v != "auto" {
            return v;
        }
    }
    if cfg!(windows) {
        let ps = std::env::var("PSModulePath").unwrap_or_default();
        let is_powershell = ps.split(std::path::MAIN_SEPARATOR).count() >= 3;
        return if is_powershell {
            "powershell.exe".into()
        } else {
            "cmd.exe".into()
        };
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    Path::new(&shell)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or(shell)
}

// Persistent roles

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemRole {
    pub name: String,
    pub role: String,
}

impl SystemRole {
    fn storage_dir(cfg: &Config) -> PathBuf {
        cfg.roles_path()
    }

    pub fn create_defaults(cfg: &Config) -> Result<()> {
        let dir = Self::storage_dir(cfg);
        fs::create_dir_all(&dir)?;
        let (os, shell) = (detect_os(cfg), detect_shell(cfg));
        let defaults = vec![
            (
                "ShellGPT",
                default_role_text(cfg, DefaultRole::Default)
                    .replace("{os}", &os)
                    .replace("{shell}", &shell),
            ),
            (
                "Shell Command Generator",
                default_role_text(cfg, DefaultRole::Shell)
                    .replace("{os}", &os)
                    .replace("{shell}", &shell),
            ),
            (
                "Shell Command Descriptor",
                default_role_text(cfg, DefaultRole::DescribeShell),
            ),
            ("Code Generator", default_role_text(cfg, DefaultRole::Code)),
        ];
        for (name, body) in defaults {
            let rp = dir.join(format!("{}.json", name));
            if rp.exists() {
                continue;
            }
            let sr = SystemRole {
                name: name.to_string(),
                role: format!("You are {}\n{}", name, body),
            };
            fs::write(rp, serde_json::to_string(&sr)?)?;
        }
        Ok(())
    }

    pub fn list(cfg: &Config) -> Vec<PathBuf> {
        let dir = Self::storage_dir(cfg);
        if let Ok(rd) = fs::read_dir(&dir) {
            let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
            files.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
            files
        } else {
            Vec::new()
        }
    }

    pub fn get(cfg: &Config, name: &str) -> Result<SystemRole> {
        let rp = Self::storage_dir(cfg).join(format!("{}.json", name));
        if !rp.exists() {
            return Err(anyhow!("role not found: {}", name));
        }
        let text = fs::read_to_string(rp)?;
        let sr: SystemRole = serde_json::from_str(&text)?;
        Ok(sr)
    }

    pub fn show(cfg: &Config, name: &str) -> Result<String> {
        Ok(Self::get(cfg, name)?.role)
    }

    pub fn create_interactive(cfg: &Config, name: &str) -> Result<()> {
        let dir = Self::storage_dir(cfg);
        fs::create_dir_all(&dir)?;
        let rp = dir.join(format!("{}.json", name));
        if rp.exists() {
            // Overwrite without confirmation to keep it simple
        }
        eprintln!(
            "Enter role description for \"{}\". Press Ctrl+D when done:\n",
            name
        );
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        if buf.trim().is_empty() {
            return Err(anyhow!("empty role description"));
        }
        let content = format!("You are {}\n{}", name, buf.trim());
        let sr = SystemRole {
            name: name.to_string(),
            role: content,
        };
        let data = serde_json::to_string(&sr)?;
        let mut f = fs::File::create(rp)?;
        f.write_all(data.as_bytes())?;
        Ok(())
    }
}

pub fn resolve_role_text(cfg: &Config, user_role: Option<&str>, fallback: DefaultRole) -> String {
    if let Some(name) = user_role {
        if let Ok(sr) = SystemRole::get(cfg, name) {
            return sr.role;
        }
    }
    let (os, shell) = (detect_os(cfg), detect_shell(cfg));
    default_role_text(cfg, fallback)
        .replace("{os}", &os)
        .replace("{shell}", &shell)
}

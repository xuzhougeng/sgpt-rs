//! Native JSON tools registry and executor.

use std::{collections::HashMap, fs, path::PathBuf, time::Duration};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, process::Command, time::timeout};

use crate::{
    config::Config,
    llm::{FunctionSchema, ToolSchema},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecSpec {
    pub program: String,
    #[serde(default)]
    pub args_template: Vec<String>,
    #[serde(default)]
    pub stdin: bool,
    #[serde(default)]
    pub timeout_sec: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: serde_json::Value,
    pub exec: ExecSpec,
}

#[derive(Debug, Clone)]
pub struct Registry {
    tools: HashMap<String, ToolDef>,
}

impl Registry {
    pub fn load(cfg: &Config) -> Result<Self> {
        let mut map = HashMap::new();
        let dir = cfg.functions_path();
        let _ = fs::create_dir_all(&dir);
        if let Ok(rd) = fs::read_dir(&dir) {
            for e in rd.filter_map(|e| e.ok()) {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let text = fs::read_to_string(&p)
                    .with_context(|| format!("reading tool file: {}", p.display()))?;
                let def: ToolDef = serde_json::from_str(&text)
                    .with_context(|| format!("parsing tool file: {}", p.display()))?;
                map.insert(def.name.clone(), def);
            }
        }
        Ok(Self { tools: map })
    }

    pub fn schemas(&self) -> Vec<ToolSchema> {
        self.tools
            .values()
            .map(|t| ToolSchema {
                r#type: "function".into(),
                function: FunctionSchema {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect()
    }

    pub async fn execute(&self, name: &str, args_json: &str) -> Result<String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| anyhow!("tool not found: {}", name))?;
        let args_val: serde_json::Value = serde_json::from_str(args_json)
            .with_context(|| format!("invalid tool args json: {}", args_json))?;

        let mut args: Vec<String> = Vec::new();
        for t in &tool.exec.args_template {
            args.push(apply_template(t, &args_val));
        }

        let mut cmd = Command::new(&tool.exec.program);
        cmd.args(&args);
        if tool.exec.stdin {
            cmd.stdin(std::process::Stdio::piped());
        }
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().with_context(|| {
            format!(
                "failed to spawn tool {} with program {}",
                name, tool.exec.program
            )
        })?;

        if tool.exec.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                let payload = serde_json::to_string(&args_val)?;
                stdin.write_all(payload.as_bytes()).await.ok();
            }
        }

        let timeout_dur = Duration::from_secs(tool.exec.timeout_sec.unwrap_or(60));
        let out = timeout(timeout_dur, child.wait_with_output())
            .await
            .map_err(|_| anyhow!("tool execution timeout: {}", name))??;

        let code = out.status.code().unwrap_or(-1);
        let mut body = String::new();
        if !out.stdout.is_empty() {
            body.push_str(&String::from_utf8_lossy(&out.stdout));
        }
        if !out.stderr.is_empty() {
            if !body.is_empty() {
                body.push_str("\n");
            }
            body.push_str(&String::from_utf8_lossy(&out.stderr));
        }
        Ok(format!("Exit code: {}\n{}", code, body))
    }
}

fn apply_template(t: &str, args: &serde_json::Value) -> String {
    let mut s = t.to_string();
    if let Some(obj) = args.as_object() {
        for (k, v) in obj {
            let needle = format!("{{{{{}}}}}", k);
            let repl = if v.is_string() {
                v.as_str().unwrap().to_string()
            } else {
                v.to_string()
            };
            s = s.replace(&needle, &repl);
        }
    }
    s
}

pub fn install_default_functions(cfg: &Config) -> Result<PathBuf> {
    let dir = cfg.functions_path();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("execute_shell_command.json");
    #[cfg(windows)]
    let (program, args): (&str, Vec<&str>) = ("powershell.exe", vec!["-Command", "{{cmd}}"]);
    #[cfg(not(windows))]
    let (program, args): (&str, Vec<&str>) = ("/bin/sh", vec!["-c", "{{cmd}}"]);
    let spec = serde_json::json!({
        "name": "execute_shell_command",
        "description": "Executes a shell command and returns the output.",
        "parameters": {
            "type": "object",
            "properties": { "cmd": {"type": "string"} },
            "required": ["cmd"]
        },
        "exec": {
            "program": program,
            "args_template": args,
            "stdin": false,
            "timeout_sec": 60
        }
    });
    fs::write(&path, serde_json::to_string_pretty(&spec)?)?;
    Ok(path)
}

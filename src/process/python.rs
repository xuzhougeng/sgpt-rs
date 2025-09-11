//! Python interpreter process bootstrap and I/O glue (skeleton).

use anyhow::Result;
use tokio::process::{Child, Command};

use super::ProcessHandle;

#[allow(dead_code)]
pub async fn start_python(bootstrap: &str) -> Result<ProcessHandle> {
    let mut cmd = Command::new("python");
    cmd.arg("-u") // unbuffered
        .arg("-c")
        .arg(bootstrap)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child: Child = cmd.spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("no stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("no stdout"))?;

    Ok(ProcessHandle {
        child,
        stdin,
        stdout,
    })
}

//! Shell integration installer for bash/zsh.

use std::fs::OpenOptions;
use std::io::Write;

use anyhow::{anyhow, Result};
use directories::BaseDirs;

const BASH_INTEGRATION: &str = r#"
# Shell-GPT integration BASH v0.2
_sgpt_bash() {
if [[ -n "$READLINE_LINE" ]]; then
    READLINE_LINE=$(sgpt --shell <<< "$READLINE_LINE" --no-interaction)
    READLINE_POINT=${#READLINE_LINE}
fi
}
bind -x '"\\C-l": _sgpt_bash'
# Shell-GPT integration BASH v0.2
"#;

const ZSH_INTEGRATION: &str = r#"
# Shell-GPT integration ZSH v0.2
_sgpt_zsh() {
if [[ -n "$BUFFER" ]]; then
    _sgpt_prev_cmd=$BUFFER
    BUFFER+="⌛"
    zle -I && zle redisplay
    BUFFER=$(sgpt --shell <<< "$_sgpt_prev_cmd" --no-interaction)
    zle end-of-line
fi
}
zle -N _sgpt_zsh
bindkey ^l _sgpt_zsh
# Shell-GPT integration ZSH v0.2
"#;

pub fn install() -> Result<()> {
    // Only non-Windows for now
    if cfg!(windows) {
        return Err(anyhow!(
            "Shell integrations only available for ZSH and Bash on Unix-like shells"
        ));
    }

    let shell = std::env::var("SHELL").unwrap_or_default();
    let home = BaseDirs::new()
        .map(|b| b.home_dir().to_path_buf())
        .ok_or_else(|| anyhow!("Cannot determine user home directory"))?;

    if shell.contains("zsh") {
        let path = home.join(".zshrc");
        println!("Installing ZSH integration into {}...", path.display());
        append_file(&path, ZSH_INTEGRATION)?;
        println!("Done! Restart your shell to apply changes.");
        return Ok(());
    }
    if shell.contains("bash") {
        let path = home.join(".bashrc");
        println!("Installing Bash integration into {}...", path.display());
        append_file(&path, BASH_INTEGRATION)?;
        println!("Done! Restart your shell to apply changes.");
        return Ok(());
    }

    Err(anyhow!(
        "Shell integrations only available for ZSH and Bash. SHELL={} not supported",
        shell
    ))
}

fn append_file(path: &std::path::Path, content: &str) -> Result<()> {
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(content.as_bytes())?;
    Ok(())
}

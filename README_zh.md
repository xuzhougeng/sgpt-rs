# sgpt-rs

[English](README.md) | 中文

该项目受到 https://github.com/TheR1D/shell_gpt 启发, 使用 Rust 进行开发

## 快速开始

```bash
git clone https://github.com/xuzhougeng/sgpt-rs
cargo build --release
```

将编译的二进制的文件移动到你的可执行目录下

```bash
mv target/release/sgpt ~/.local/bin
```

编辑 ~/.config/sgpt_rs/.sgptrc 设置使用DeepSeek作为默认模型

```yaml
API_BASE_URL=https://api.deepseek.com
OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
DEFAULT_MODEL=deepseek-chat
```

也可以直接在环境变量

```bash
export API_BASE_URL=https://api.deepseek.com
export OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
export DEFAULT_MODEL=deepseek-chat
```

直接使用sgpt

```
$ sgpt
Hello! This is ShellGPT. How can I assist you with your shell command needs today? 😊
```

发送信息

```
$ sgpt "say hi in one word"
Hi
```

## Windows 与 PowerShell 支持

- 指定目标 Shell：使用 `--target-shell` 强制生成特定 Shell 的命令。
  - 可选值：`auto`、`powershell`、`cmd`、`bash`、`zsh`、`fish`、`sh`
  - 示例：
    - 生成 PowerShell 命令：`sgpt -s --target-shell powershell "查看当前目录下包含 foo 的文件"`
    - 生成 CMD 命令：`sgpt -s --target-shell cmd "打印 PATH 并退出"`
- 交互执行：在 Windows 上会根据 `--target-shell` 或自动检测优先使用 PowerShell 执行（否则回退到 CMD）。
- 生成提示优化：当目标为 PowerShell 时，提示会引导模型优先使用 PowerShell 原生命令（如 `Get-ChildItem`、`Select-String`），并使用 `;` 连接多步命令（而不是 `&&`）。

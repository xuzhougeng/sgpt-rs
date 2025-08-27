# sgpt-rs

English | [中文](README_zh.md)

This project is inspired by https://github.com/TheR1D/shell_gpt and is developed using Rust.

## Quick Start

```bash
git clone https://github.com/xuzhougeng/sgpt-rs
cargo build --release
```

Move the compiled binary file to your executable directory.

```bash
mv target/release/sgpt ~/.local/bin
```

Edit ~/.config/sgpt_rs/.sgptrc to set DeepSeek as the default model.

```yaml
API_BASE_URL=https://api.deepseek.com
OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
DEFAULT_MODEL=deepseek-chat
```

You can also set directly in environment variables.

```bash
export API_BASE_URL=https://api.deepseek.com
export OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
export DEFAULT_MODEL=deepseek-chat
```

Use sgpt directly.

```
$ sgpt
Hello! This is ShellGPT. How can I assist you with your shell command needs today? 😊
```

Send a message.

```
$ sgpt "say hi in one word"
Hi
```
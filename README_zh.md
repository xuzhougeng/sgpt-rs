# sgpt-rs

[English](README.md) | 中文

该项目受 https://github.com/TheR1D/shell_gpt 启发, 在Claude-Code, Codex, Cursor协助下，使用 Rust 进行开发

## 快速开始

```bash
git clone https://github.com/xuzhougeng/sgpt-rs
cargo build --release
```

将编译的二进制的文件移动到你的可执行目录下

```bash
mv target/release/sgpt ~/.local/bin
```

或者也可以从 <https://github.com/xuzhougeng/sgpt-rs/releases/>下载预编译的二进制文件。

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

大抵是支持所所有OpenAI接口兼容的模型。

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

## 文档处理功能

支持直接处理文档文件，将文件内容作为上下文进行对话：

```bash
# 处理文档并提问
sgpt --doc README.md "这个项目是做什么的？"

# 仅处理文档（等价于 cat README.md | sgpt）
sgpt --doc notes.txt

# 结合其他参数使用
sgpt --doc changelog.log "最近有什么更新？" --md
```

**支持的文件类型：**
- `.md` - Markdown 文件
- `.txt` - 纯文本文件  
- `.rst` - reStructuredText 文件
- `.log` - 日志文件
- 无扩展名文件

这个功能等价于 `cat xxx.md | sgpt 'xxx'`，但更方便直接使用文件路径。

## 网络搜索功能

支持使用 Tavily 进行网络搜索，提供两种搜索模式：

### 配置

- 环境变量配置：
  - `export TVLY_API_KEY=tvly_xxxxxxxxxxxxx`
  - 可选：`export TAVILY_API_BASE=https://api.tavily.com`
- 或在 `~/.config/sgpt_rs/.sgptrc` 中添加：
  - `TVLY_API_KEY=tvly_xxxxxxxxxxxxx`
  - `TAVILY_API_BASE=https://api.tavily.com`

### 基础搜索

直接返回搜索结果，输出标题、URL 与摘要：

```bash
sgpt --search "Who is Leo Messi?"
echo "recent Rust release" | sgpt --search
```

### 增强搜索 🚀

三步智能搜索流程，提供更全面的分析：

```bash
# 完整参数
sgpt --enhanced-search "Who is Leo Messi?"

# 或使用缩写
sgpt -e "Who is Leo Messi?"
```

增强搜索流程：
1. **意图分析**：AI 分析问题并构建 3 组不同角度的检索词
2. **多维检索**：并行执行多组搜索，获取全面信息
3. **综合回答**：基于搜索结果生成详细的综合分析（支持最多 4096 tokens 的详细回答）

程序会优先输出结果标题、URL 与摘要；若结构不含常见字段，将以 JSON 格式原样输出。

## Windows 与 PowerShell 支持

- 指定目标 Shell：使用 `--target-shell` 强制生成特定 Shell 的命令。
  - 可选值：`auto`、`powershell`、`cmd`、`bash`、`zsh`、`fish`、`sh`
  - 示例：
    - 生成 PowerShell 命令：`sgpt -s --target-shell powershell "查看当前目录下包含 foo 的文件"`
    - 生成 CMD 命令：`sgpt -s --target-shell cmd "打印 PATH 并退出"`
- 交互执行：在 Windows 上会根据 `--target-shell` 或自动检测优先使用 PowerShell 执行（否则回退到 CMD）。
- 生成提示优化：当目标为 PowerShell 时，提示会引导模型优先使用 PowerShell 原生命令（如 `Get-ChildItem`、`Select-String`），并使用 `;` 连接多步命令（而不是 `&&`）。

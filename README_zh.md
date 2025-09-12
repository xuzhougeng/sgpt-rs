# sgpt-rs

[English](README.md) | 中文

该项目受 https://github.com/TheR1D/shell_gpt 启发, 在Claude-Code, Codex 协助下，使用 Rust 进行开发, 旨在降低配置难度

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

也可以直接在环境变量或者在~/.zshrc, ~/.bashrc中配置

```bash
export API_BASE_URL=https://api.deepseek.com
export OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
export DEFAULT_MODEL=deepseek-chat
```

大抵是支持所所有OpenAI接口兼容的模型。

### 使用 Ollama（本地模型）

本项目兼容 OpenAI 风格接口的提供方，Ollama 亦可直接使用：

1) 启动 Ollama（默认端口 11434）。

2) 配置环境变量或 `~/.config/sgpt_rs/.sgptrc`：

```bash
# 推荐写法：带 /v1（客户端会自动补齐 /v1，不带也可以）
export API_BASE_URL=http://localhost:11434/v1
# Ollama 本地不需要 API Key（未设置时不会发送 Authorization 头）
unset OPENAI_API_KEY
# 选择已拉取的模型，例如：llama3.1 / qwen2.5 / mistral 等
export DEFAULT_MODEL=llama3.1
```

3) 试运行：

```bash
sgpt --model llama3.1 "你好，介绍一下你自己"
```

注意：
- 优先使用“Chat Completions”流式接口；部分后端（含 Ollama）可能不支持 Responses API。
- 若遇到工具调用（functions/tools）相关 400/422，说明后端不支持 OpenAI 工具协议，请避免使用 `--functions`。

直接使用sgpt, 确认能正常工作

```
$ sgpt
Hello! This is ShellGPT. How can I assist you with your shell command needs today? 😊
```

## 使用案例

### 对话

> 默认是等待输出完成后渲染，如果需要流式输出, 可以用参数 --no-md, 或者环境变量设置export PRETTIFY_MARKDOWN=false

每次就是一轮, 没有上下文

```bash
sgpt "say hi in one word"
Hi
```

多轮对话（开启/继续会话）, 参数`--chat <id>` 

```bash
sgpt --chat test '你现在是小爱'
sgpt --chat test '你是谁?'
```

查看与管理会话

```bash
# 查看会话历史
sgpt --show-chat test

# 查看所有会话（及其路径）
sgpt --list-chats
```

临时会话（不保存历史）

```bash
sgpt --chat temp "这一轮不会被保存"
```

开启一个“常驻的交互式多轮会话”（TUI），参数 `--repl <id>` 

```bash
sgpt --repl test2
```

### SHELL

生成shell命令, `--shell`

```bash
sgpt --shell '统计rs文件数'
```

此时他会弹出一个`[E]xecute, [M]odify, [D]escribe, [A]bort:`让你去选择操作

- [E]xecute 执行
- [M]odify 发现命令不对，再给点提示
- [D]escribe 解释这个命令
- [A]bort 直接退出不执行


关于这个行为可以通过如下两个参数或通过SHELL_INTERACTION来设置行为

- `--interaction`: 需要手动确认是否需要执行命令，默认行为
- `--no-interaction`: 不需要交互，直接运行


在shell的基础上增加 `--repl <id>`  就可以进入交互式命令行, 在获取命令行后, 可以用e来执行

```bash
sgpt --shell --repl temp
状态栏：
  - Shell REPL: e=execute, r=repeat, d=describe | ctrl+h help
  - Python/R REPL: e=execute, r=repeat | ctrl+h help

输入与帮助：
  - Enter=发送，Shift+Enter=换行（备用：Ctrl+S 发送，Ctrl+J 换行）
  - Ctrl+H 或 F1 弹出/关闭简洁帮助
```

Windows 与 PowerShell 支持

- 指定目标 Shell：使用 `--target-shell` 强制生成特定 Shell 的命令。
  - 可选值：`auto`、`powershell`、`cmd`、`bash`、`zsh`、`fish`、`sh`
  - 示例：
    - 生成 PowerShell 命令：`sgpt -s --target-shell powershell "查看当前目录下包含 foo 的文件"`
    - 生成 CMD 命令：`sgpt -s --target-shell cmd "打印 PATH 并退出"`
- 交互执行：在 Windows 上会根据 `--target-shell` 或自动检测优先使用 PowerShell 执行（否则回退到 CMD）。
- 生成提示优化：当目标为 PowerShell 时，提示会引导模型优先使用 PowerShell 原生命令（如 `Get-ChildItem`、`Select-String`），并使用 `;` 连接多步命令（而不是 `&&`）。

## 文档处理功能

支持直接处理文档文件，将文件内容作为上下文进行对话：

```bash
# 单个文件
sgpt --doc document.md "your question"

# 多个文件
sgpt --doc file1.md --doc file2.txt --doc file3.md "your question"
```

**支持的文件类型：**
- `.pdf` - PDF文件, 提取文本作为输入, 没有图像OCR
- `.md` - Markdown 文件
- `.txt` - 纯文本文件  
- `.rst` - reStructuredText 文件
- `.log` - 日志文件
- 无扩展名文件

这个功能等价于 `cat xxx.md yyy.md | sgpt 'xxx'`，但更方便直接使用文件路径。

TODO: 当前就是将所有内容作为输入给LLM, 后续可能应该单个文件编辑

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

## 详细文档

- `doc/Config.md`: .config/sgpt_rs/.sgptrc 可以配置的参数说明
- `doc/Role.md`: 跟角色配置相关的参数的详细说明
- `doc/Model.md`: 项目内置兼容 OpenAI Chat Completions 的流式客户端，支持工具调用（function-calling）。关于配置、数据结构、流式事件、错误提示与使用示例

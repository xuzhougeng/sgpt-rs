# sgpt-rs

[English](README.md) | 中文

该项目收到 https://github.com/TheR1D/shell_gpt 启发, 使用 Rust 进行开发

## Quick Start

1) 设置环境变量（可覆盖 `.sgptrc`）:

```
export API_BASE_URL=https://api.deepseek.com
export OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
export DEFAULT_MODEL=deepseek-chat
```

2) 构建并查看帮助:

```
cargo build
cargo run -- --help
```

3) 默认一次性响应（将 tokens 流式输出到 stdout）:

```
cargo run -- "say hi in one word"
```

4) 会话聊天（可持久化）:

```
# 启动或继续一个会话
cargo run -- --chat mychat "say hi"
cargo run -- --chat mychat "and now say bye"
```

聊天历史默认以 JSON 存在 `${TMPDIR}/shell_gpt/chat_cache/<chat_id>`
（例如：`/tmp/shell_gpt/chat_cache/mychat`）。通过 `CHAT_CACHE_PATH` 可覆盖。

5) REPL（临时会话）:

```
cargo run -- --repl temp
>>> Hello in 3 words
Hello there friend
>>> """
... First line
... Second line
... """
<model output>
>>> exit()
```

6) 显示/列出会话（可彩色/Markdown）:

```
cargo run -- --list-chats
cargo run -- --show-chat mychat        # 彩色分角色
cargo run -- --show-chat mychat --md   # Markdown 渲染（termimad）
```

7) 标准输入与分隔符（`__sgpt__eof__`）:

```
printf "context from stdin\n" | cargo run -- "and argument prompt"

# 对于 REPL，`__sgpt__eof__` 之前作为初始提示，之后来自终端/tty 的交互输入。
printf "This is init\n__sgpt__eof__\nNext line from tty\nexit()\n" | cargo run -- --repl temp
```

8) 模型与采样参数:

```
# 模型解析优先级：CLI --model > DEFAULT_MODEL (env/config) > gpt-4o
cargo run -- --model deepseek-chat "hello"

# 设置采样参数
cargo run -- --temperature 0.5 --top-p 0.9 "short answer please"
```

## Shell 模式（交互与自动执行）

基本交互流程：生成命令 → 提示 `[E]xecute, [M]odify, [D]escribe, [A]bort`

```
# 交互执行
cargo run -- -s "create a temp file named x and write hello"
# 按 e 执行，按 d 查看解释，按 m 输入修改要求后重新生成命令
```

默认自动执行（回车=执行）：

```
export DEFAULT_EXECUTE_SHELL_CMD=true
cargo run -- -s "echo hello"
# 交互提示下直接回车会执行命令
```

非交互模式（仅打印，不执行）：

```
cargo run -- -s "list files" --no-interaction
```

TTY 检测：当 stdin 不是终端（例如通过管道运行）时，自动切换为非交互，仅打印命令。

注意：命令具有副作用，请谨慎执行。

## 函数调用（tools）

安装默认函数（本地 JSON 工具定义）：

```
cargo run -- --install-functions
# 安装到 ~/.config/shell_gpt/functions/execute_shell_command.json
```

默认在 Default/Chat 模式允许工具调用（可用 `--no-functions` 关闭）。若后端不支持 OpenAI tools（或返回 4xx），请关闭工具调用。

自定义工具以 JSON 定义（示例字段）：

```
{
  "name": "execute_shell_command",
  "description": "Executes a shell command and returns the output.",
  "parameters": { "type": "object", "properties": {"cmd": {"type":"string"}}, "required":["cmd"] },
  "exec": { "program": "/bin/sh", "args_template": ["-c", "{{cmd}}"], "stdin": false, "timeout_sec": 60 }
}
```

更多工具示例（将 JSON 文件放入 `~/.config/shell_gpt/functions/`）：

1) 打开浏览器链接（open_url.json）

```
{
  "name": "open_url",
  "description": "Open a URL in default browser.",
  "parameters": {
    "type": "object",
    "properties": { "url": {"type": "string"} },
    "required": ["url"]
  },
  "exec": {
    "program": "/bin/sh",
    "args_template": ["-c", "xdg-open '{{url}}' >/dev/null 2>&1 || open '{{url}}'"] ,
    "stdin": false,
    "timeout_sec": 10
  }
}
```

2) 读取文本文件（read_file.json）

```
{
  "name": "read_file",
  "description": "Read a text file and return content.",
  "parameters": {
    "type": "object",
    "properties": { "path": {"type": "string"} },
    "required": ["path"]
  },
  "exec": {
    "program": "/bin/sh",
    "args_template": ["-c", "cat '{{path}}'"] ,
    "stdin": false,
    "timeout_sec": 10
  }
}
```

3) 写入文本文件（write_file.json，谨慎使用）

```
{
  "name": "write_file",
  "description": "Write text content to a file (overwrite).",
  "parameters": {
    "type": "object",
    "properties": {
      "path": {"type": "string"},
      "content": {"type": "string"}
    },
    "required": ["path", "content"]
  },
  "exec": {
    "program": "/bin/sh",
    "args_template": ["-c", "printf '%s' '{{content}}' > '{{path}}'"] ,
    "stdin": false,
    "timeout_sec": 10
  }
}
```

安全提示：工具可能产生副作用，建议优先使用只读操作，并在描述中明确限制；必要时关闭工具调用（`--no-functions`）。

## REPL Shell 模式

在 REPL 中以 Shell 角色生成命令，并支持快捷指令：

- `e`：执行当前生成的命令
- `d`：描述当前命令
- `m`：输入修改说明，生成改写后的新命令
- `r`：重复执行上一次生成的命令
- `p`：打印上一次生成的命令
- 普通输入：根据输入再生成新命令

示例：

```
cargo run -- --repl temp --shell
>>> list current folder
ls -l
>>> d
<描述 ls -l 的输出与参数>
>>> m
Modify with instructions: add human readable sizes
ls -lh
>>> e
<执行 ls -l 的结果>
>>> exit()
```

禁用 REPL Shell 交互（将 e/d/m/r/p 视作普通输入）:

```
cargo run -- --repl temp --shell --no-interaction
```

## 其它说明与状态

- Markdown 渲染：Default/Chat/REPL 在开启 `--md` 时整段渲染（termimad），否则逐 token 打印。
- 会话与缓存：会话存 `CHAT_CACHE_PATH`，请求缓存存 `CACHE_PATH`；含 tool_calls 的回答不缓存。
- `API_BASE_URL` 自动补 `/v1`（若缺失），`OPENAI_API_KEY` 以 `Authorization: Bearer` 发送。

Refer to top-level Plan.md for milestones and next steps.

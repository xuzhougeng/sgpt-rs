# Code 模式（`--code`）

面向“只要代码”的使用场景：返回纯代码、无解释、无 Markdown 代码块，实时流式输出到终端。

## 功能概述

- 纯代码输出：仅输出代码文本，不附带解释、注释或 Markdown 代码块围栏。
- 流式打印：使用 SSE 流式返回，边生成边输出，结束时自动换行。
- 关闭工具调用：不启用 OpenAI Functions/Tools（仅纯文本补全）。
- 忽略 Markdown 美化：在该模式下会强制关闭 Markdown 美化开关。
- 模型参数透传：`--model`、`--temperature`、`--top-p`、`--max-tokens` 均生效。
- 可与文档上下文合并：支持 `--doc` 将文件内容与提示合并后一并提交给模型。

## 基本用法

```bash
# 最简示例：输出一段 Python Hello World 代码
sgpt --code "Hello world Python"

# 指定模型与采样参数
sgpt --code --model gpt-4o-mini --temperature 0.2 "写一个冒泡排序（Go）"

# 结合文档上下文：将文件内容并入提示
sgpt --code --doc src/lib.rs --doc README.md "补全 parse_args 函数实现"
```

## 与其它参数/模式的关系

- 模式互斥：与下列模式互斥（同属 `mode` 分组）
  - `--shell`、`--describe-shell`、`--search`、`--enhanced-search`
- 勿与会话模式混用：`--chat` / `--repl` 在参数路由上优先生效；若同时提供，程序会进入聊天/REPL 分支而非 Code 分支，因此不建议与 `--code` 同用。
- 忽略自定义角色：`--role` 在 `--code` 下不生效，本模式使用内置“Code Generator”系统角色。
- Markdown 开关：`--md` / `--no-md` 在该模式下被强制为关闭（流式直接打印）。
- 函数调用：在该模式下会显式禁用 functions/tools。

## 设计与实现要点（源码索引）

- 参数定义与分组：
  - `src/cli.rs:5`（`mode` 分组包含 `code`）
  - `src/cli.rs:67`（`-c, --code` 开关）
- 模式路由与强制开关：
  - `src/main.rs:167`（从 flags 推导角色）
  - `src/main.rs:169`（在 Shell/Code/Describe 下强制关闭 Markdown 并禁用 functions）
  - `src/main.rs:239`（进入 `handlers::code::run(...)`）
- Code 处理器：
  - `src/handlers/code.rs:12`（构造 system+user 消息，`tools: None`，SSE 流式仅打印 `Content`）
- 内置角色文案（仅输出代码、无 Markdown/解释）：
  - `src/role/mod.rs:52`
- 文档上下文并入逻辑（`--doc`）：
  - `src/main.rs:75`（读取并合并文档内容与用户提示）

## 行为细节

- 仅代码、非 Markdown：不会出现 ``` 或 ```python 等围栏。
- 无澄清提问：内置角色约束为“缺少细节时给出最合理实现，不提澄清问题”。
- 末尾换行：收到流结束标记时会自动输出一次换行。
- 模型与上限：`--model` 未提供时回落到配置中的 `DEFAULT_MODEL`；`--max-tokens` 透传给后端（受具体模型上限影响）。

## 常见问题与建议

- 想要解释或注释？请不要使用 `--code`，改用默认模式或 `--chat`。
- 想要 Markdown 代码块？仍然不要使用 `--code`（本模式明确禁用 Markdown 围栏），改用默认模式并启用 `--md`（或配置 `PRETTIFY_MARKDOWN=true`）。
- 想配合文档/代码库上下文？可以使用 `--doc` 多次传入文件，系统会先将文件内容与提示合并再提交模型。

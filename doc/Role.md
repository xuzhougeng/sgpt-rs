# 角色（Role）使用说明

本文档介绍如何在 sgpt-rs 中创建、查看、管理并使用系统角色（System Role），以便为不同场景配置合适的人设或行为规范。

## 角色存储位置

- 角色以 JSON 文件形式保存在目录：`~/.config/sgpt_rs/roles`
- 每个角色文件名为 `<角色名>.json`，内容示例：

```json
{"name":"SQL Master","role":"You are SQL Master\n...角色正文..."}
```

## 基本命令

- 创建角色（交互式）：
  - `sgpt --create-role "SQL Master"`
  - 在提示中输入角色正文（Ctrl+D 结束）。若同名已存在，将覆盖。

- 查看角色：
  - `sgpt --show-role "SQL Master"`

- 列出所有角色：
  - `sgpt --list-roles`（别名：`-r` / `--lr`）

## 在对话中使用角色

- 单轮对话：
  - `sgpt --role "SQL Master" "按用户统计总消费"`

- 多轮会话（上下文记忆）：
  - `sgpt --chat db --role "SQL Master" "先查看订单表结构"`
  - 后续继续：`sgpt --chat db "写一个汇总 SQL"`

- 与文档结合（将文件内容并入上下文）：
  - `sgpt --doc schema.sql --role "SQL Master" "根据 schema 写查询"`

说明：`--role` 适用于默认对话、`--chat` 与 `--repl`；`--shell`/`--describe-shell`/`--code` 会使用各自的默认系统角色。

## 推荐的角色编写建议

编写角色时可包含以下要点（以“SQL Master”为例）：

- 输出规范：只输出 SQL，纯文本，不要 Markdown 代码块与解释。
- 方言控制：默认 ANSI SQL；用户指定方言时严格遵循（MySQL/PostgreSQL/SQLite/SQL Server/Oracle）。
- 安全约束：禁止危险操作（如无 WHERE 的 DELETE/UPDATE、DROP）；读操作加 LIMIT；写操作使用参数化占位符（如 `:user_id`）。
- 结构约束：严格遵循提供的表结构，不使用不存在的列；避免 `SELECT *`。
- 语义清晰：JOIN 使用显式 ON；聚合配合 GROUP BY；需要时用 CTE（WITH）。
- 互动策略：不确定时先提出澄清问题再给出方案（或在注释中给出关键点）。

示例（可作为 `SQL Master` 角色正文的起点）：

```
You are SQL Master.
Rules:
- Output only SQL in plain text, no Markdown or explanations.
- Default to ANSI SQL; honor specified dialect strictly (MySQL/PostgreSQL/SQLite/SQL Server/Oracle).
- Avoid destructive queries; never DELETE/UPDATE without WHERE; avoid DROP.
- Prefer parameterized queries (e.g., :user_id) for writes.
- Follow provided schemas; never reference non-existent columns; avoid SELECT *.
- Use explicit JOIN ... ON ...; group aggregates with GROUP BY; use CTEs when helpful.
- If critical info is missing, ask one concise clarifying question.
```

## 进阶：手工创建或编辑角色

- 直接编辑文件：`~/.config/sgpt_rs/roles/<角色名>.json`
- JSON 结构：

```json
{"name":"<角色名>","role":"You are <角色名>\n<角色正文>"}
```

## 默认角色与覆盖

- 首次运行会自动写入一组默认角色（默认/命令生成/命令解释/代码），并根据操作系统与 Shell 环境注入变量（如 `{os}`、`{shell}`）。
- 使用 `--role <NAME>` 时，若存在同名自定义角色，则优先使用自定义角色内容。

## 参数总结（与角色相关）

- `--role <ROLE>`：在默认对话、`--chat`、`--repl` 中设置系统角色；`--shell`/`--describe-shell`/`--code` 使用各自内置角色，不受此参数影响。
- `--create-role <NAME>`：交互式创建/覆盖角色，写入 `~/.config/sgpt_rs/roles/<NAME>.json`。
- `--show-role <NAME>`：打印指定角色的完整正文。
- `-r, --list-roles`（别名 `--lr`）：列出所有已保存的角色文件。
- 角色生效时机：作为对话的第一条 system 消息写入。如果需要更换角色，建议新开会话（`--chat <new_id>` 或 `--repl <new_id>`）。
- 存储目录：`~/.config/sgpt_rs/roles`（可直接手动编辑 JSON）。
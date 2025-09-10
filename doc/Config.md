# 配置说明（.sgptrc 与环境变量）

本项目使用简单的 `KEY=VALUE` 配置方式。推荐在下述文件中进行持久化配置：

- 文件路径：`~/.config/sgpt_rs/.sgptrc`
- 格式：每行一项，形如 `KEY=VALUE`，`#` 开头为注释
- 覆盖优先级：命令行参数 > 环境变量 > `.sgptrc` 文件 > 内置默认值
  - 任何键的读取都会优先使用同名环境变量（`Config::get` 会先查环境变量）

示例（OpenAI/DeepSeek）：

```
# OpenAI 兼容接口
API_BASE_URL=https://api.openai.com
OPENAI_API_KEY=sk-xxxxxxxx
DEFAULT_MODEL=gpt-4o-mini

# DeepSeek 示例
# API_BASE_URL=https://api.deepseek.com
# OPENAI_API_KEY=sk-xxxxxxxx
# DEFAULT_MODEL=deepseek-chat

# 终端输出偏好
PRETTIFY_MARKDOWN=true
REQUEST_TIMEOUT=60
```

## 关键配置项

- API_BASE_URL：OpenAI 兼容接口基础地址
  - 默认值：`default`（内部解析为 `https://api.openai.com/v1`）
  - 版本补全策略（Strategy A）：若路径中已含 `/v{数字}`（如 `/v4`）保持不变；否则自动补 `/v1`
  - 示例：`https://api.openai.com` -> `https://api.openai.com/v1`；`https://open.bigmodel.cn/api/paas/v4` 保持不变

- OPENAI_API_KEY：API 密钥
  - 用于设置 `Authorization: Bearer <key>` 请求头

- DEFAULT_MODEL：默认模型名
  - CLI `--model` 优先于该项

- REQUEST_TIMEOUT：请求超时时间（秒）
  - 默认：`60`

- PRETTIFY_MARKDOWN：是否美化 Markdown（影响是否实时逐字输出）
  - 默认：`true`
  - `true`：缓冲完整内容后统一渲染为 Markdown
  - `false`：实时逐字打印（等价于 CLI `--no-md`）

- OPENAI_USE_FUNCTIONS：是否启用工具调用（function-calling）
  - 默认：`false`
  - CLI `--functions` 会开启；部分后端不兼容时需关闭

- OPENAI_FUNCTIONS_PATH：工具函数定义目录
  - 默认：`~/.config/sgpt_rs/functions`

- SHOW_FUNCTIONS_OUTPUT：是否在输出中显示工具执行结果
  - 默认：`false`

- DEFAULT_EXECUTE_SHELL_CMD：Shell 交互模式中按回车是否默认执行
  - 默认：`false`

- SHELL_INTERACTION：是否启用 Shell 交互（确认/修改/解释等）
  - 默认：`true`
  - 也可用 CLI `--interaction` / `--no-interaction` 覆盖

- OS_NAME / SHELL_NAME：操作系统与 Shell 名称（用于角色模板变量）
  - 默认：`auto`（自动探测）
  - 可配合 CLI `--target-shell`（如 `powershell.exe`、`cmd.exe`、`zsh` 等）

## 缓存与持久化

- CHAT_CACHE_PATH：会话缓存目录
  - 默认：系统临时目录下 `sgpt_rs/chat_cache`
- CHAT_CACHE_LENGTH：单会话最大消息数（保留第一条 system）
  - 默认：`100`
- CACHE_PATH：请求结果缓存目录（非会话记忆）
  - 默认：系统临时目录下 `sgpt_rs/cache`
- CACHE_LENGTH：请求缓存条目上限
  - 默认：`100`

## 角色与相关路径

- ROLE_STORAGE_PATH：系统角色存储目录
  - 默认：`~/.config/sgpt_rs/roles`
  - 相关命令：`--create-role`、`--show-role`、`--list-roles`（详见 `doc/Role.md`）

## Web 搜索（Tavily）

- TVLY_API_KEY：Tavily API Key
- TAVILY_API_BASE：Tavily 接口地址（可选，默认 `https://api.tavily.com`）

说明：`.sgptrc` 中任何键均会被读取；同名环境变量可覆盖文件值。

## 其他（保留/前向兼容）

- DISABLE_STREAMING：默认 `false`（预留开关）
- CODE_THEME：代码主题（默认 `dracula`）
- USE_LITELLM：默认 `false`（预留开关）

## 参考

- 加载实现：`src/config/mod.rs`
- LLM 客户端与 Base URL 规则：`src/llm/mod.rs`，详见 `doc/Model.md`
- 角色系统：`src/role/mod.rs`，详见 `doc/Role.md`


# 模型与 LLM 客户端

本文档基于 `src/llm/mod.rs` 的实现，介绍本项目内置的基于 Reqwest 的、兼容 OpenAI Chat Completions 的流式 LLM 客户端：配置项、数据结构、接口以及错误处理与使用示例。

## 概览

- 客户端：`LlmClient` 使用 `reqwest` 发送 `POST {base_url}/chat/completions` 请求，并以 `text/event-stream` 接收增量结果。
- 兼容性：请求/响应格式按 OpenAI Chat Completions 接口约定（含流式 SSE 与工具调用字段）。
- 流式事件：以枚举 `StreamEvent` 产出内容分片、工具调用增量、结束标记等。

## 配置

客户端从全局配置中读取以下键（环境变量或 `~/.config/sgpt_rs/.sgptrc`）：

- `API_BASE_URL`：服务端根地址。默认值特殊为 `default`，将解析为 `https://api.openai.com/v1`；否则会确保以 `/v1` 结尾（会自动补齐）。
  - 版本补全策略（Strategy A）：若基础 URL 中已包含形如 `/v{数字}` 的版本段（例如 `/v4`），将保持不变；否则自动补齐 `/v1`。例如：
    - `https://api.openai.com` -> `https://api.openai.com/v1`
    - `https://api.openai.com/v1` -> 保持不变
    - `https://open.bigmodel.cn/api/paas/v4` -> 保持不变（不会再追加 `/v1`）
- `OPENAI_API_KEY`：用于设置 `Authorization: Bearer <key>` 头；未设置则不发送鉴权头。
- `REQUEST_TIMEOUT`：请求超时秒数，默认 `60`。

提示：项目层面还支持 `DEFAULT_MODEL`（常用模型名），可在 CLI 或配置中设置；非本模块直接读取，但与下文 `ChatOptions.model` 一起使用。

## 数据结构

以下类型通过 `serde`（`Serialize/Deserialize`）与服务端进行编解码。

- 角色 `Role`（序列化为小写）：`system` | `user` | `assistant` | `tool`。
- 消息 `ChatMessage`：
  - `role: Role`
  - `content: String`
  - `name: Option<String>`（用于 `tool` 消息时可选）
  - `tool_calls: Option<Vec<ToolCall>>`（当 assistant 产生工具调用时）
- 工具与函数定义：
  - `FunctionSchema`：`name: String`，`description: Option<String>`，`parameters: serde_json::Value`（建议为 JSON Schema；为 null 时不序列化）
  - `ToolSchema`：`type: "function"`，`function: FunctionSchema`
  - `ToolCall`：`id: Option<String>`，`type: "function"`，`function: FunctionCall`
  - `FunctionCall`：`name: String`，`arguments: String`（原样字符串，通常为 JSON 文本）
- 会话选项 `ChatOptions`：
  - `model: String`
  - `temperature: f32`
  - `top_p: f32`
  - `tools: Option<Vec<ToolSchema>>`
  - `parallel_tool_calls: bool`
  - `tool_choice: Option<String>`（如 `"auto"`）
  - `max_tokens: Option<u32>`（未设置时，客户端会在请求体中使用 `512`；可通过 CLI `--max-tokens`/`--max_tokens` 指定）
- 流事件 `StreamEvent`：
  - `Content(String)`：内容增量分片
  - `ToolCallDelta { name: Option<String>, arguments: Option<String> }`：工具调用增量（函数名与参数可能分别推送）
  - `ToolCallsFinish`：表示后续不再有工具调用增量（对应 finish_reason = "tool_calls"）
  - `Done`：流结束（收到 `[DONE]`）

## 客户端接口

- 构造：`LlmClient::from_config(cfg: &Config) -> anyhow::Result<LlmClient>`
  - 读取并规范化 `API_BASE_URL`（自动补 `/v1`）、`OPENAI_API_KEY`、`REQUEST_TIMEOUT`。
  - 内部初始化 `reqwest::Client`（超时为 `REQUEST_TIMEOUT` 秒）。

- 调用：`chat_stream(&self, messages: Vec<ChatMessage>, opts: ChatOptions) -> Stream<Item = Result<StreamEvent>>`
  - **特殊处理**：当 `opts.model` 为 `"fake"` 时，进入调试模式，不发送真实 API 请求，而是输出完整的请求体 JSON 格式供调试使用。
  - 请求头：`Content-Type: application/json`；`Accept: text/event-stream`；如有 `OPENAI_API_KEY` 则附带 `Authorization`。
  - 请求体：
    - 基本字段：`model`、`temperature`、`top_p`、`messages`、`stream: true`、`max_tokens`（未指定则为 `512`）。
    - 当 `opts.tools` 存在时，额外包含：`tools`、`parallel_tool_calls`，可选 `tool_choice`。
  - 解析 SSE：按行读取 `data:` 前缀负载；`[DONE]` 触发 `StreamEvent::Done`；其余 JSON 以 OpenAI 流式格式解析为分片并产生相应事件。

## 错误处理与提示

当 HTTP 状态码非 2xx 时，会返回包含服务端响应片段（最多 800 字符）的错误，并尽量附加排查提示：

- 401：提示设置 `OPENAI_API_KEY`。
- 422/400 且包含工具相关字段报错（`tool_choice`/`parallel_tool_calls`/`tools`/`function_call`/`tool calls`）：提示后端可能不支持 OpenAI 工具协议，可移除 `--functions` 或设置 `OPENAI_USE_FUNCTIONS=false`。
- 模型相关错误（`model` + `not found`/`unknown`/`invalid`）：提示检查 `--model` 或正确设置 `DEFAULT_MODEL`。
- 限流相关（`rate limit`/`quota`）：提示稍后重试或降低并发。

## 使用示例（Rust）

以下示例展示如何创建客户端、发送消息并消费流式事件：

```rust
use futures_util::StreamExt;
use sgpt_rs::config::Config;
use sgpt_rs::llm::{LlmClient, ChatMessage, ChatOptions, Role, StreamEvent};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = Config::load()?; // 从环境与 ~/.config/sgpt_rs/.sgptrc 加载
    let client = LlmClient::from_config(&cfg)?;

    let messages = vec![
        ChatMessage { role: Role::System, content: "You are a helpful assistant.".into(), name: None, tool_calls: None },
        ChatMessage { role: Role::User, content: "你好，帮我总结一下Rust是什么？".into(), name: None, tool_calls: None },
    ];

    let opts = ChatOptions {
        model: cfg.get("DEFAULT_MODEL").unwrap_or_else(|| "gpt-4o-mini".into()),
        temperature: 0.7,
        top_p: 1.0,
        tools: None,
        parallel_tool_calls: false,
        tool_choice: None,
        max_tokens: None, // 未设置则请求体使用 512
    };

    let mut stream = client.chat_stream(messages, opts);
    while let Some(event) = stream.next().await.transpose()? {
        match event {
            StreamEvent::Content(chunk) => print!("{}", chunk),
            StreamEvent::ToolCallDelta { name, arguments } => {
                eprintln!("\n[tool-delta] name={:?} args={:?}", name, arguments);
            }
            StreamEvent::ToolCallsFinish => eprintln!("\n[tool-calls-finish]"),
            StreamEvent::Done => break,
        }
    }

    Ok(())
}
```

## 调试模式（Fake Model）

当使用 `--model fake` 时，客户端会进入特殊的调试模式：

### 功能特点

- **不发送 API 请求**：避免消耗 API 额度和网络流量
- **显示完整请求内容**：以格式化 JSON 输出本来要发送给 LLM 的请求体
- **支持所有功能**：与文档处理、函数调用、角色系统等功能完全兼容
- **无需 API Key**：可在未配置 `OPENAI_API_KEY` 的环境中使用

### 输出格式

```
=== FAKE MODEL DEBUG OUTPUT ===
Request that would be sent to LLM API:

{
  "model": "fake",
  "messages": [
    {
      "role": "system",
      "content": "You are a helpful assistant."
    },
    {
      "role": "user", 
      "content": "Hello world"
    }
  ],
  "temperature": 0.0,
  "top_p": 1.0,
  "max_tokens": 512,
  "stream": true
}

=== END DEBUG OUTPUT ===
```

### 使用场景

- **调试文档处理**：`sgpt --model fake --doc file.pdf "总结文档"`，查看 PDF 内容是否正确提取并包含在请求中
- **验证参数传递**：`sgpt --model fake --temperature 0.7 --max-tokens 1000 "test"`，确认参数设置
- **测试角色配置**：`sgpt --model fake --role researcher "分析数据"`，检查系统角色是否正确应用
- **函数调用调试**：`sgpt --model fake --functions "搜索信息"`，查看工具定义和调用配置
- **离线开发**：在无网络环境下验证功能逻辑

### CLI 使用示例

```bash
# 基本调试
sgpt --model fake "Hello world"

# 文档处理调试
sgpt --model fake --doc document.pdf "What are the key points?"

# 参数调试
sgpt --model fake --temperature 0.5 --top-p 0.9 --max-tokens 2000 "test"

# 角色功能调试  
sgpt --model fake --role researcher "Analyze this data"

# 函数调用调试
sgpt --model fake --functions "Search for recent news about AI"
```

## 兼容性与注意事项

- 若后端不支持 OpenAI 工具协议，使用 `tools`/`tool_choice`/`parallel_tool_calls` 可能导致 400/422 错误，请按提示禁用相应功能。
- `API_BASE_URL` 会被规范化为以 `/v1` 结尾；若已包含 `/v1` 则不重复添加。
- SSE 以行分割并以 `data:` 前缀承载 JSON；`[DONE]` 用于指示流结束。
- 本模块不主动重试；建议在上层根据需要实现重试与超时处理。
- **调试模式**：使用 `fake` 模型名（不区分大小写）可触发调试输出，适用于开发和测试场景。

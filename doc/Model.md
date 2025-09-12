# 模型与 LLM 客户端

本文档基于 `src/llm/mod.rs` 的实现，介绍本项目内置的基于 Reqwest 的 LLM 客户端，支持两套 OpenAI 兼容的接口：
- **Chat Completions API**：流式聊天完成接口
- **Responses API**：非流式响应接口（新增）

涵盖配置项、数据结构、接口以及错误处理与使用示例。

## 概览

- **Chat Completions API（流式）**：`LlmClient` 使用 `reqwest` 发送 `POST {base_url}/chat/completions` 请求，并以 `text/event-stream` 接收增量结果。
- **Responses API（非流式）**：发送 `POST {base_url}/responses` 请求，一次性返回完整响应。
- **兼容性**：请求/响应格式按 OpenAI 接口约定（含流式 SSE 与工具调用字段）。
- **流式事件**：以枚举 `StreamEvent` 产出内容分片、工具调用增量、结束标记等。
- **测试模式**：两套 API 均支持 `fake` 模型进行调试。

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

- 角色 `Role`（序列化为小写）：`system` | `user` | `assistant` | `tool` | `developer`（新增）。
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

### Responses API 专用结构（新增）

- 输入 `ResponseInput`（枚举）：
  - `Text(String)`：简单文本输入
  - `Messages(Vec<ChatMessage>)`：完整消息数组
- 选项 `ResponseOptions`：
  - `model: String`：模型名称
  - `instructions: Option<String>`：高优先级指令
  - `temperature: Option<f32>`：采样温度
  - `max_tokens: Option<u32>`：最大生成tokens
  - `reasoning: Option<ReasoningOptions>`：推理参数（用于推理模型）
- 推理选项 `ReasoningOptions`：
  - `effort: String`：推理强度（`"low"` | `"medium"` | `"high"`）
- 响应 `ResponsesApiResponse`：
  - `id: String`：响应ID
  - `object: String`：对象类型
  - `model: String`：使用的模型
  - `output: Vec<ResponseOutput>`：输出数组
  - `output_text: Option<String>`：便利字段，聚合所有文本输出
  - `usage: Option<Usage>`：token使用统计
- 输出 `ResponseOutput`：
  - `id: String`：输出项ID
  - `type: String`：输出类型
  - `role: String`：角色
  - `content: Vec<OutputContent>`：内容数组
- 输出内容 `OutputContent`：
  - `type: String`：内容类型（如 `"output_text"`）
  - `text: Option<String>`：文本内容
  - `annotations: Vec<serde_json::Value>`：注释数组
- 使用统计 `Usage`：
  - `prompt_tokens: u32`：输入token数
  - `completion_tokens: u32`：输出token数
  - `total_tokens: u32`：总token数

## 客户端接口

- 构造：`LlmClient::from_config(cfg: &Config) -> anyhow::Result<LlmClient>`
  - 读取并规范化 `API_BASE_URL`（自动补 `/v1`）、`OPENAI_API_KEY`、`REQUEST_TIMEOUT`。
  - 内部初始化 `reqwest::Client`（超时为 `REQUEST_TIMEOUT` 秒）。

### Chat Completions API（流式）

- 调用：`chat_stream(&self, messages: Vec<ChatMessage>, opts: ChatOptions) -> Stream<Item = Result<StreamEvent>>`
  - **特殊处理**：当 `opts.model` 为 `"fake"` 时，进入调试模式，生成模拟的流式响应。
  - 请求头：`Content-Type: application/json`；`Accept: text/event-stream`；如有 `OPENAI_API_KEY` 则附带 `Authorization`。
  - 请求体：
    - 基本字段：`model`、`temperature`、`top_p`、`messages`、`stream: true`、`max_tokens`（未指定则为 `512`）。
    - 当 `opts.tools` 存在时，额外包含：`tools`、`parallel_tool_calls`，可选 `tool_choice`。
  - 解析 SSE：按行读取 `data:` 前缀负载；`[DONE]` 触发 `StreamEvent::Done`；其余 JSON 以 OpenAI 流式格式解析为分片并产生相应事件。

### Responses API（非流式）

- 调用：`create_response(&self, input: ResponseInput, opts: ResponseOptions) -> Result<ResponsesApiResponse>`
  - **特殊处理**：当 `opts.model` 为 `"fake"` 时，返回模拟的完整响应。
  - 请求端点：`POST {base_url}/responses`
  - 请求头：`Content-Type: application/json`；如有 `OPENAI_API_KEY` 则附带 `Authorization`。
  - 请求体：根据输入类型构建，支持 `input`、`instructions`、`temperature`、`max_tokens`、`reasoning` 等参数。
  - 返回：完整的响应对象，包含输出数组和便利的聚合文本字段。

### 便利方法

- `generate_text(&self, input: &str, model: &str) -> Result<String>`：简单文本生成
- `generate_text_with_instructions(&self, input: &str, instructions: &str, model: &str) -> Result<String>`：带指令的文本生成

### 链式配置

`ResponseOptions` 支持链式配置：
```rust
let opts = ResponseOptions::new("gpt-4".to_string())
    .with_instructions("Be helpful".to_string())
    .with_temperature(0.7)
    .with_max_tokens(1000)
    .with_reasoning("high");
```

### 响应解析方法

`ResponsesApiResponse` 提供便利方法：
- `get_text() -> Option<&str>`：获取主要文本输出
- `get_all_text() -> String`：获取所有文本输出的连接结果

## 错误处理与提示

### Chat Completions API 错误处理

当 HTTP 状态码非 2xx 时，会返回包含服务端响应片段（最多 800 字符）的错误，并尽量附加排查提示：

- 401：提示设置 `OPENAI_API_KEY`。
- 422/400 且包含工具相关字段报错（`tool_choice`/`parallel_tool_calls`/`tools`/`function_call`/`tool calls`）：提示后端可能不支持 OpenAI 工具协议，可移除 `--functions` 或设置 `OPENAI_USE_FUNCTIONS=false`。
- 模型相关错误（`model` + `not found`/`unknown`/`invalid`）：提示检查 `--model` 或正确设置 `DEFAULT_MODEL`。
- 限流相关（`rate limit`/`quota`）：提示稍后重试或降低并发。

### Responses API 错误处理

Responses API 具有类似的错误处理机制，额外包括：

- 404：提示 API 提供商可能不支持 Responses API，建议使用 `chat_stream` 替代。
- 422/400 且包含模型错误：提示检查模型名称或为提供商正确设置 `DEFAULT_MODEL`。

## 使用示例（Rust）

### Chat Completions API（流式）示例

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

### Responses API（非流式）示例

以下示例展示如何使用新的 Responses API：

```rust
use sgpt_rs::config::Config;
use sgpt_rs::llm::{LlmClient, ResponseInput, ResponseOptions, ReasoningOptions};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    let client = LlmClient::from_config(&cfg)?;
    let model = cfg.get("DEFAULT_MODEL").unwrap_or_else(|| "gpt-4".into());

    // 简单文本生成
    let result = client.generate_text("解释什么是Rust编程语言", &model).await?;
    println!("简单生成结果：\n{}", result);

    // 带指令的文本生成
    let result = client.generate_text_with_instructions(
        "写一个排序算法",
        "用中文解释，并提供Rust代码示例",
        &model
    ).await?;
    println!("带指令结果：\n{}", result);

    // 完整配置的使用
    let opts = ResponseOptions::new(model)
        .with_instructions("你是一个Rust专家，请提供详细的解释".to_string())
        .with_temperature(0.3)
        .with_max_tokens(1000)
        .with_reasoning("medium");

    let input = ResponseInput::Text("什么是所有权系统？".to_string());
    let response = client.create_response(input, opts).await?;
    
    // 使用便利方法获取文本
    if let Some(text) = response.get_text() {
        println!("完整配置结果：\n{}", text);
    }
    
    // 查看token使用情况
    if let Some(usage) = &response.usage {
        println!("Token使用: 输入={}, 输出={}, 总计={}",
            usage.prompt_tokens, usage.completion_tokens, usage.total_tokens);
    }

    Ok(())
}
```

### 推理模型示例

对于支持推理的模型（如 o1、o3 系列），可以使用推理参数：

```rust
// 推理模型的高效配置
let opts = ResponseOptions::new("o1-preview".to_string())
    .with_reasoning("high")  // 高强度推理
    .with_instructions("仔细分析这个复杂问题".to_string());

let input = ResponseInput::Text("如何设计一个高并发的分布式系统？".to_string());
let response = client.create_response(input, opts).await?;

println!("推理结果：\n{}", response.get_all_text());
```

## 调试模式（Fake Model）

当使用 `--model fake` 时，客户端会进入特殊的调试模式，**同时支持流式和非流式两种API**：

### 功能特点

- **不发送 API 请求**：避免消耗 API 额度和网络流量
- **双API支持**：
  - Chat Completions API：生成模拟的流式响应（适配shell/chat模式）
  - Responses API：返回结构化的模拟响应对象
- **智能响应**：根据输入内容生成相应的模拟回复
- **支持所有功能**：与文档处理、函数调用、角色系统等功能完全兼容
- **无需 API Key**：可在未配置 `OPENAI_API_KEY` 的环境中使用

### 流式模式输出特点

- **Shell模式**：根据输入生成合适的shell命令（如 `ls`, `git status` 等）
- **Chat模式**：生成模拟的对话响应，支持代码生成等场景
- **流式展示**：逐字符输出模拟真实的流式体验，包含延迟效果

### 非流式模式输出特点

- **结构化响应**：返回完整的 `ResponsesApiResponse` 对象
- **Usage统计**：包含模拟的token使用数据
- **便利方法**：支持 `get_text()` 和 `get_all_text()` 方法

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

### API兼容性

- **Chat Completions API**：
  - 若后端不支持 OpenAI 工具协议，使用 `tools`/`tool_choice`/`parallel_tool_calls` 可能导致 400/422 错误，请按提示禁用相应功能。
  - SSE 以行分割并以 `data:` 前缀承载 JSON；`[DONE]` 用于指示流结束。
  
- **Responses API**：
  - 较新的API，部分提供商可能尚未支持，遇到404错误时建议使用 `chat_stream` 替代。
  - 支持高优先级的 `instructions` 参数和推理模型的 `reasoning` 配置。
  - 返回结构包含 `output_text` 便利字段，简化文本提取。

### 与 Ollama 的兼容

Ollama 提供 OpenAI 兼容接口，可直接通过本客户端调用：

- 配置示例：

  ```bash
  # 可选两种写法：
  export API_BASE_URL=http://localhost:11434/v1   # 明确 /v1
  # 或 export API_BASE_URL=http://localhost:11434  # 客户端会自动补 /v1
  unset OPENAI_API_KEY                             # 本地无需鉴权头
  export DEFAULT_MODEL=llama3.1                    # 使用已拉取的模型名
  ```

- 建议使用 `chat_stream`（Chat Completions 流式接口）；部分 Ollama 版本不支持 Responses API。
- 当携带 `tools`/`tool_choice` 等字段导致 400/422 错误时，表示该后端不支持 OpenAI 工具调用协议；请移除函数调用相关参数。
- 若使用多模态/图片能力，请确认模型和后端确实支持；否则将提示尝试去掉 `--image` 或改用支持多模态的提供方。

### 通用注意事项

- `API_BASE_URL` 会被规范化为以 `/v1` 结尾；若已包含 `/v1` 则不重复添加。
- 本模块不主动重试；建议在上层根据需要实现重试与超时处理。
- **调试模式**：使用 `fake` 模型名（不区分大小写）可触发调试输出，两套API均支持，适用于开发和测试场景。
- 新增的 `Role::Developer` 角色在TUI界面显示为蓝色 "DEV" 前缀。

### API选择建议

- **选择流式API**：需要实时响应体验的场景（如交互式聊天、shell命令生成）
- **选择非流式API**：需要完整响应对象、token统计或推理模型的场景
- **开发调试**：两套API均可使用 `fake` 模型进行功能验证

## 多模态支持（图片）

### ✨ 新功能：图片处理支持

项目现已支持多模态对话，可以同时处理文本和图片输入。

### 图片功能特点

- **多图片支持**：可同时处理多张图片 `--image photo1.jpg --image chart.png`
- **格式支持**：JPG、JPEG、PNG、GIF、WebP、BMP
- **智能编码**：自动Base64编码，高质量图片处理
- **与现有功能兼容**：可与文档（`--doc`）、函数调用等功能组合使用

### CLI使用方法

```bash
# 单张图片分析
sgpt --image photo.jpg "描述这张图片"

# 多张图片对比
sgpt --image chart1.png --image chart2.png "比较这两个图表的数据"

# 结合文档和图片
sgpt --doc report.pdf --image diagram.png "根据文档和图表分析市场趋势"

# Shell模式与图片（截图调试）
sgpt --shell --image screenshot.png "根据这个错误截图生成修复命令"

# 代码模式与图片（UI设计）
sgpt --code --image mockup.png "根据这个设计图生成HTML代码"
```

### 技术实现

**1. 多模态消息结构**
```rust
pub enum MessageContent {
    Text(String),                    // 纯文本
    MultiModal(Vec<ContentPart>),    // 文本+图片混合
}

pub enum ContentPart {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
}
```

**2. 图片处理流程**
- 自动格式检测和验证
- Base64编码优化
- 高质量图片参数（detail: "high"）
- 错误处理和用户友好提示

**3. 兼容性保证**
- 向后兼容：纯文本场景零影响
- API透明：现有handler无需修改调用方式
- 错误安全：图片处理失败不影响文本功能

### 使用场景示例

**📊 数据分析**
```bash
sgpt --image sales_chart.png "分析这个销售数据的趋势和异常点"
```

**🐛 调试协助**
```bash
sgpt --shell --image error_screenshot.png "根据这个错误信息生成调试命令"
```

**🎨 设计开发**
```bash
sgpt --code --image ui_mockup.png "根据这个界面设计生成React组件"
```

**📚 文档理解**
```bash
sgpt --doc manual.pdf --image diagram.png "解释文档中这个架构图的含义"
```

### 注意事项

- 图片文件必须存在且格式支持
- 较大图片会增加API请求大小和处理时间
- 某些LLM提供商可能对多模态功能有特殊要求
- 建议对图片进行适当压缩以提高处理效率

# sgpt-rs 代码检索融入 autocoder-nano 架构规划

## 项目目标

在 sgpt-rs 中融入 autocoder-nano 的代码检索和 RAG 功能，实现智能代码上下文检索，提升代码生成和问答的准确性。

## autocoder-nano 代码检索核心步骤分析

### 七阶段代码检索流程

基于 `src/autocoder_nano/index/entry.py:18-164` 的分析：

#### 第一阶段：处理 REST/RAG/Search 资源
- 识别特殊标签文件（REST、RAG、SEARCH）
- 直接加入候选文件列表

#### 第二阶段：构建代码索引
- **核心组件**：`IndexManager` (`src/autocoder_nano/index/index_manager.py:19-418`)
- **功能**：提取代码符号（函数、类、变量、导入语句）
- **关键方法**：`build_index()` 和 `get_all_file_symbols()`

#### 第三阶段：Level 1 过滤（基于查询）
- **核心方法**：`get_target_files_by_query()`
- **机制**：基于 LLM 分析用户查询，匹配相关文件
- **支持特殊语法**：`@` 指定文件路径，`@@` 指定符号名

#### 第四阶段：Level 2 过滤（基于相关文件）
- **核心方法**：`get_related_files()`
- **机制**：分析文件依赖关系，找到相关联文件

#### 第五阶段：相关性验证
- **核心方法**：`verify_file_relevance()`
- **机制**：LLM 评估文件内容与查询的相关性（0-10分）

#### 第六阶段：应用限制条件
- 根据 `index_filter_file_num` 限制返回文件数量
- 优先级排序和去重处理

#### 第七阶段：准备最终输出
- 格式化输出：`##File: {filename}\n{content}\n\n`
- 去重和路径优化显示

## 核心代码实现分析

### 1. 索引管理器（IndexManager）

```python
class IndexManager:
    def __init__(self, args: AutoCoderArgs, source_codes: List[SourceCode], llm: AutoLLM):
        self.args = args
        self.sources = source_codes
        self.llm = llm
        self.index_file = os.path.join(source_dir, ".auto-coder", "index.json")
        self.max_input_length = args.model_max_input_length
        
    def build_index(self):
        """构建或更新索引，使用多线程处理"""
        # 1. 检查缓存的索引文件
        # 2. MD5 校验文件变化
        # 3. LLM 提取符号信息
        # 4. 保存索引到 JSON 文件
        
    @prompt()
    def get_all_file_symbols(self, path: str, code: str) -> str:
        """LLM 提取代码符号：函数、类、变量、导入语句"""
        
    def get_target_files_by_query(self, query: str):
        """基于查询条件查找相关文件"""
        # 1. 分块处理大型项目
        # 2. LLM 分析查询意图
        # 3. 匹配文件路径和符号
        # 4. 返回带原因的文件列表
```

### 2. 过滤管理器（FilterManager）

```python
class FilterManager:
    """提供通用的过滤框架"""
    
    def create_filter(self, field: str, operator: FilterOperator, value: Any):
        """创建单一过滤条件"""
        
    def create_filter_group(self, filters: List, operator: LogicalOperator):
        """创建过滤组合（AND/OR/NOT）"""
        
    def apply_filters(self, items: List[Dict], filter_spec: Union[Filter, FilterGroup]):
        """应用过滤条件到项目列表"""
```

### 3. 文档过滤器（DocFilter）

```python
class DocFilter:
    """专门用于文档相关性过滤"""
    
    def filter_docs_with_threads(self, conversations: List[Dict], documents: List[SourceCode]):
        """多线程并行过滤文档"""
        # 1. 使用 ThreadPoolExecutor 并行处理
        # 2. LLM 判断文档与查询的相关性
        # 3. 解析相关性分数（"yes/no"格式）
        # 4. 按相关性分数排序
```

### 4. 向量存储（DuckDBVectorStore）

```python
class DuckDBVectorStore:
    """基于 DuckDB 的向量化存储和检索"""
    
    def vector_search(self, query_embedding: List[float], similarity_top_k: int):
        """向量相似度搜索"""
        
    def full_text_search(self, query: str, similarity_top_k: int):
        """全文搜索功能"""
```

## Rust 融入方案设计

本节在原有高层设计基础上，补齐与 autocoder-nano 一致的关键实现细节，明确嵌入生成、Token 预算、混合索引/缓存、过滤提示词、模式与 CLI 配置、文件扫描与忽略规则、可观测性与测试基线，确保开发落地无歧义。

### 架构设计

```
sgpt-rs/src/
├── rag/
│   ├── mod.rs                    # RAG 模块入口
│   ├── index_manager.rs          # 索引管理
│   ├── filter_manager.rs         # 过滤管理
│   ├── doc_filter.rs             # 文档过滤
│   ├── vector_store.rs           # 向量存储
│   └── context_manager.rs        # 上下文管理
├── handlers/
│   ├── code_context.rs           # 代码上下文处理器
│   └── rag_search.rs             # RAG 搜索处理器
└── cli.rs                        # 新增 RAG 相关命令参数
```

### 1. 依赖库选择

```toml
[dependencies]
# 现有依赖...
serde_json = "1.0"           # JSON 处理
tokio-postgres = "0.7"       # PostgreSQL 异步客户端
duckdb = { version = "0.8", features = ["bundled"] }  # DuckDB 本地存储
tantivy = "0.19"            # 全文搜索引擎
tree-sitter = "0.20"        # 代码解析
# 语言语法支持（按需精简）
tree-sitter-rust = "0.20"
tree-sitter-python = "0.20"
tree-sitter-javascript = "0.20"
tree-sitter-go = "0.20"
ignore = "0.4"              # 文件过滤
walkdir = "2.0"             # 目录遍历
md5 = "0.7"                 # 文件哈希
rayon = "1.7"               # 并行处理
futures = "0.3"             # 并发/JoinAll
tokenizers = "0.15"         # 使用 tokenizer.json 做字节级分词与计数
anyhow = "1"                # 统一错误处理
thiserror = "1"             # 错误类型
```

说明：
- DuckDB 需确保 vss/fts 扩展可用（通过 INSTALL/LOAD 方式启用）。具体版本以能使用 list_cosine_similarity 为准；如受限，可先行实现“仅全文/子串”检索降级策略。
- 若不使用 Tantivy，可移除并仅保留 DuckDB + VSS；若需要纯文本 FTS，可择一（避免双索引重复维护）。
- `tokenizers` 读取与 autocoder-nano 相同的 `tokenizer.json`，保证 Token 统计一致性。

### 2. 核心结构体定义

```rust
// src/rag/mod.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceCode {
    pub module_name: String,
    pub source_code: String,
    pub tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetFile {
    pub file_path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexItem {
    pub module_name: String,
    pub symbols: String,
    pub last_modified: f64,
    pub md5: String,
}

#[derive(Debug, Clone)]
pub struct RagConfig {
    pub source_dir: String,
    pub index_filter_level: i32,
    pub index_filter_file_num: i32,
    pub verify_file_relevance_score: i32,
    pub model_max_input_length: usize,
    pub skip_build_index: bool,
    pub skip_filter_index: bool,
    // Hybrid Index / DuckDB 设置
    pub enable_hybrid_index: bool,
    pub duckdb_vector_dim: usize,
    pub duckdb_query_top_k: usize,
    pub duckdb_query_similarity: f32, // 0-1 之间，阈值越高越严格
    pub anti_quota_limit_ms: u64,     // 写入/调用 Embeddings 的节流毫秒
    // Token 预算与选择
    pub rag_context_window_limit: usize,
    pub full_text_ratio: f32,  // e.g. 0.6
    pub segment_ratio: f32,    // e.g. 0.35
    pub disable_auto_window: bool,
    pub disable_segment_reorder: bool,
    pub index_filter_workers: usize,  // 过滤并发度
    pub rag_doc_filter_relevance: i32 // 文档相关性阈值（0-10）
}
```

### 2.1 嵌入（Embeddings）API 设计

为与 autocoder-nano 保持一致，需在 `LlmClient` 中新增 Embeddings 能力（或单独 `EmbeddingsClient`）：

- 路径：`POST {BASE}/embeddings`
- 请求：`{ "model": "<emb_model>", "input": ["text1", "text2", ...] }`
- 响应：`{ "data": [{"embedding": [f32; D]}, ...] }`
- 维度：由后端返回或通过 `duckdb_vector_dim` 显式降维到固定值；需做 L2 归一化。
- 速率限制：按照 `anti_quota_limit_ms` 节流，失败重试与指数退避。

实现要点：
- Embedding 正常返回后，如维度与 `duckdb_vector_dim` 不一致，采用固定随机投影/PCA 方式降维（与 autocoder-nano 一致的固定随机种子）。
- 统一对向量做归一化，保证余弦相似度稳定。

### 3. 索引管理器实现

```rust
// src/rag/index_manager.rs
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;
use tree_sitter::{Language, Parser};

pub struct IndexManager {
    config: RagConfig,
    sources: Vec<SourceCode>,
    llm_client: LlmClient,
    index_file_path: String,
}

impl IndexManager {
    pub fn new(config: RagConfig, sources: Vec<SourceCode>, llm_client: LlmClient) -> Self {
        let index_file_path = format!("{}/.auto-coder/index.json", config.source_dir);
        Self {
            config,
            sources,
            llm_client,
            index_file_path,
        }
    }

    pub async fn build_index(&self) -> Result<HashMap<String, IndexItem>, Box<dyn std::error::Error>> {
        // 1. 读取现有索引缓存
        let mut index_data = self.load_existing_index().await?;
        
        // 2. 检查文件变化（MD5对比）
        let changed_files = self.detect_changed_files(&index_data).await?;
        
        // 3. 并行处理变更文件
        let tasks: Vec<_> = changed_files
            .into_iter()
            .map(|source| self.build_index_for_source(source))
            .collect();
        
        let results = futures::future::join_all(tasks).await;
        
        // 4. 更新索引数据
        for result in results {
            if let Ok(item) = result {
                index_data.insert(item.module_name.clone(), item);
            }
        }
        
        // 5. 保存索引到文件
        self.save_index(&index_data).await?;
        
        Ok(index_data)
    }

    async fn build_index_for_source(&self, source: SourceCode) -> Result<IndexItem, Box<dyn std::error::Error>> {
        // 1. 计算文件 MD5
        let md5_hash = md5::compute(source.source_code.as_bytes());
        let md5_str = format!("{:x}", md5_hash);
        
        // 2. 使用 tree-sitter 解析代码符号
        let symbols = self.extract_symbols_with_parser(&source).await?;
        
        // 3. 如果解析失败，回退到 LLM 提取
        let symbols = if symbols.is_empty() {
            self.extract_symbols_with_llm(&source).await?
        } else {
            symbols
        };
        
        // 4. 获取文件修改时间
        let metadata = fs::metadata(&source.module_name).await?;
        let modified_time = metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_secs_f64();
        
        Ok(IndexItem {
            module_name: source.module_name,
            symbols,
            last_modified: modified_time,
            md5: md5_str,
        })
    }

    async fn extract_symbols_with_parser(&self, source: &SourceCode) -> Result<String, Box<dyn std::error::Error>> {
        // 使用 tree-sitter 解析代码结构
        let language = match Path::new(&source.module_name).extension().and_then(|s| s.to_str()) {
            Some("rs") => tree_sitter_rust::language(),
            Some("py") => tree_sitter_python::language(),
            Some("js") | Some("ts") => tree_sitter_javascript::language(),
            Some("go") => tree_sitter_go::language(),
            _ => return Ok(String::new()), // 不支持的语言类型
        };
        
        let mut parser = Parser::new();
        parser.set_language(language)?;
        
        let tree = parser.parse(&source.source_code, None)
            .ok_or("Failed to parse source code")?;
        
        // 遍历AST提取符号
        let symbols = self.traverse_ast_for_symbols(tree.root_node(), &source.source_code)?;
        
        Ok(symbols)
    }

    async fn get_target_files_by_query(&self, query: &str) -> Result<Vec<TargetFile>, Box<dyn std::error::Error>> {
        // 1. 分块读取索引数据
        let index_chunks = self.get_index_chunks().await?;
        
        // 2. 并行查询每个分块
        let query_tasks: Vec<_> = index_chunks
            .into_iter()
            .map(|chunk| self.query_chunk_for_files(chunk, query))
            .collect();
        
        let results = futures::future::join_all(query_tasks).await;
        
        // 3. 合并和去重结果
        let mut all_files = Vec::new();
        for result in results {
            if let Ok(files) = result {
                all_files.extend(files);
            }
        }
        
        // 4. 应用文件数量限制
        if self.config.index_filter_file_num > 0 {
            all_files.truncate(self.config.index_filter_file_num as usize);
        }
        
        Ok(all_files)
    }

    async fn query_chunk_for_files(&self, index_chunk: String, query: &str) -> Result<Vec<TargetFile>, Box<dyn std::error::Error>> {
        let system_prompt = r#"
        下面是已知文件以及对应的符号信息：

        现在，请根据用户的问题以及前面的文件和符号信息，寻找相关文件路径。返回结果按如下格式：

        ```json
        {
            "file_list": [
                {
                    "file_path": "path/to/file.py",
                    "reason": "The reason why the file is the target file"
                }
            ]
        }
        ```

        请严格遵循以下步骤：
        1. 识别特殊标记：查找query中的 `@` 符号（文件路径）和 `@@` 符号（符号名）
        2. 匹配文件路径和符号
        3. 分析依赖关系
        4. 考虑文件用途
        "#;

        let user_message = format!("{}\n\n用户的问题是：\n{}", index_chunk, query);
        
        let messages = vec![
            ChatMessage {
                role: Role::System,
                content: system_prompt.to_string(),
                name: None,
                tool_calls: None,
            },
            ChatMessage {
                role: Role::User,
                content: user_message,
                name: None,
                tool_calls: None,
            },
        ];

        let opts = ChatOptions {
            model: self.config.model.clone(),
            temperature: 0.0,
            top_p: 1.0,
            tools: None,
            parallel_tool_calls: false,
            tool_choice: None,
            max_tokens: Some(2048),
        };

        let mut stream = self.llm_client.chat_stream(messages, opts);
        let mut response = String::new();
        
        while let Some(event) = futures_util::StreamExt::next(&mut stream).await {
            match event? {
                StreamEvent::Content(text) => response.push_str(&text),
                StreamEvent::Done => break,
                _ => {}
            }
        }

        // 解析 JSON 响应
        let file_list: serde_json::Value = serde_json::from_str(&response.trim())?;
        let files = file_list["file_list"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|item| {
                Some(TargetFile {
                    file_path: item["file_path"].as_str()?.to_string(),
                    reason: item["reason"].as_str()?.to_string(),
                })
            })
            .collect();

        Ok(files)
    }
}
```

### 3.1 Hybrid Index（DuckDB + JSONL 缓存）设计细节

- 缓存文件：`{source_dir}/.cache/nano_storage_speedup.jsonl`
  - 行式 JSON，每行对应一个 `CacheItem`：
    - `file_path`: 绝对路径
    - `relative_path`: 相对 `source_dir` 路径
    - `content`: `[SourceCode]`（含 `module_name`, `source_code`, `tokens`, `metadata`）
    - `modify_time`: 文件 mtime（float）
    - `md5`: 文件 MD5
- DuckDB 表结构：`rag(_id VARCHAR, file_path VARCHAR, content TEXT, raw_content TEXT, vector FLOAT[], mtime FLOAT)`
  - `_id` 由 `module_name + chunk_idx` 唯一标识
  - `vector` 存储归一化的嵌入；需先 `INSTALL/LOAD vss` 扩展
- 构建流程：
  1) 扫描文件 → MD5 对比 → 过滤未变更文件
  2) 小文件合并 / 大文件切分（见 4. Token 预算）
  3) 生成/更新 JSONL 缓存
  4) 批量写入 DuckDB（分批、并发、进度输出）
- 增量更新：定期触发 `trigger_update()` 比对当前文件集与缓存，构造删除/新增/更新事件队列，按事件更新 DuckDB 与 JSONL。

### 4. CLI 参数扩展

```rust
// src/cli.rs - 新增参数
#[derive(Parser, Debug, Clone)]
pub struct Cli {
    // ... 现有参数

    /// Enable code context retrieval using RAG.
    #[arg(long = "rag")]
    pub rag: bool,

    /// Set source directory for code indexing.
    #[arg(long = "source-dir")]
    pub source_dir: Option<String>,

    /// Set index filter level (0-2).
    #[arg(long = "filter-level", default_value_t = 1)]
    pub filter_level: i32,

    /// Maximum number of files to include in context.
    #[arg(long = "max-files", default_value_t = 10)]
    pub max_files: i32,

    /// Relevance score threshold (0-10).
    #[arg(long = "relevance-threshold", default_value_t = 6)]
    pub relevance_threshold: i32,

    /// Build index for source code.
    #[arg(long = "build-index")]
    pub build_index: bool,

    /// Query code index.
    #[arg(long = "query-index")]
    pub query_index: Option<String>,

    /// Enable hybrid index and vector search (DuckDB+VSS)
    #[arg(long = "enable-hybrid-index", default_value_t = true)]
    pub enable_hybrid_index: bool,

    /// Vector dimension (after optional projection)
    #[arg(long = "duckdb-vector-dim", default_value_t = 1024)]
    pub duckdb_vector_dim: usize,

    /// Vector similarity top-k
    #[arg(long = "duckdb-query-top-k", default_value_t = 200)]
    pub duckdb_query_top_k: usize,

    /// Vector similarity threshold (0-1)
    #[arg(long = "duckdb-query-similarity", default_value_t = 0.6)]
    pub duckdb_query_similarity: f32,

    /// RAG doc relevance threshold (0-10)
    #[arg(long = "rag-doc-filter-relevance", default_value_t = 5)]
    pub rag_doc_filter_relevance: i32,

    /// Concurrency for relevance filtering
    #[arg(long = "index-filter-workers", default_value_t = 5)]
    pub index_filter_workers: usize,

    /// Token window limit for RAG
    #[arg(long = "rag-context-window-limit", default_value_t = 120000)]
    pub rag_context_window_limit: usize,

    /// Full text ratio within token window
    #[arg(long = "full-text-ratio", default_value_t = 0.6)]
    pub full_text_ratio: f32,

    /// Segment ratio within token window
    #[arg(long = "segment-ratio", default_value_t = 0.35)]
    pub segment_ratio: f32,

    /// Disable auto small-merge/large-split
    #[arg(long = "disable-auto-window", default_value_t = false)]
    pub disable_auto_window: bool,

    /// Disable second-round segment reorder
    #[arg(long = "disable-segment-reorder", default_value_t = false)]
    pub disable_segment_reorder: bool,

    /// Throttle to avoid provider quota (ms)
    #[arg(long = "anti-quota-limit-ms", default_value_t = 250)]
    pub anti_quota_limit_ms: u64,

    /// Contexts-only mode: output only relevant contexts
    #[arg(long = "contexts-only", default_value_t = false)]
    pub contexts_only: bool,
}
```

### 5. 代码上下文处理器

```rust
// src/handlers/code_context.rs
pub struct CodeContextHandler {
    config: RagConfig,
    llm_client: LlmClient,
}

impl CodeContextHandler {
    pub async fn run(
        query: &str,
        source_dir: &str,
        model: &str,
        config: &Config,
        rag_config: RagConfig,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut handler = Self::new(rag_config, config)?;
        
        // 1. 扫描源代码文件
        let sources = handler.scan_source_files(source_dir).await?;
        
        // 2. 构建索引
        let index_manager = IndexManager::new(handler.config.clone(), sources.clone(), handler.llm_client.clone());
        let _index_data = index_manager.build_index().await?;
        
        // 3. 执行多级过滤
        let target_files = handler.multi_stage_filtering(query, &index_manager).await?;
        
        // 4. 构建上下文字符串
        let context = handler.build_context_string(&sources, &target_files).await?;
        
        Ok(context)
    }

    async fn multi_stage_filtering(
        &self,
        query: &str,
        index_manager: &IndexManager,
    ) -> Result<Vec<TargetFile>, Box<dyn std::error::Error>> {
        let mut final_files = Vec::new();
        
        // Stage 1: Query-based filtering
        if self.config.index_filter_level >= 1 {
            let target_files = index_manager.get_target_files_by_query(query).await?;
            final_files.extend(target_files);
        }
        
        // Stage 2: Related files filtering
        if self.config.index_filter_level >= 2 && !final_files.is_empty() {
            let file_paths: Vec<String> = final_files.iter().map(|f| f.file_path.clone()).collect();
            let related_files = index_manager.get_related_files(&file_paths).await?;
            final_files.extend(related_files);
        }
        
        // Stage 3: Relevance verification
        let verified_files = self.verify_file_relevance(query, &final_files).await?;
        
        Ok(verified_files)
    }

    async fn verify_file_relevance(
        &self,
        query: &str,
        files: &[TargetFile],
    ) -> Result<Vec<TargetFile>, Box<dyn std::error::Error>> {
        let verification_tasks: Vec<_> = files
            .iter()
            .map(|file| self.verify_single_file_relevance(query, file))
            .collect();
        
        let results = futures::future::join_all(verification_tasks).await;
        
        let mut verified_files = Vec::new();
        for result in results {
            if let Ok(Some(file)) = result {
                verified_files.push(file);
            }
        }
        
        // 按相关性分数排序
        verified_files.sort_by(|a, b| {
            b.reason.split(':').nth(1)
                .and_then(|s| s.split(',').next())
                .and_then(|s| s.trim().parse::<i32>().ok())
                .unwrap_or(0)
                .cmp(&a.reason.split(':').nth(1)
                    .and_then(|s| s.split(',').next())
                    .and_then(|s| s.trim().parse::<i32>().ok())
                    .unwrap_or(0))
        });
        
        Ok(verified_files)
    }

    async fn build_context_string(
        &self,
        sources: &[SourceCode],
        target_files: &[TargetFile],
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut context = String::new();
        let mut processed_files = std::collections::HashSet::new();
        
        for target_file in target_files {
            if processed_files.contains(&target_file.file_path) {
                continue;
            }
            
            if let Some(source) = sources.iter().find(|s| s.module_name == target_file.file_path) {
                context.push_str(&format!("##File: {}\n", source.module_name));
                context.push_str(&source.source_code);
                context.push_str("\n\n");
                processed_files.insert(&target_file.file_path);
            }
        }
        
        Ok(context)
    }
}
```

### 6. 相关性过滤 Prompt 规范（LLM）

- 输入：对话历史 `conversations` + 单文档字符串列表 `documents`（每个格式 `##File: <path>\n<content>`）
- 产出：严格输出 `"yes/<score>"` 或 `"no/<score>"`，其中 `<score>` ∈ [0,10]
- 阈值：通过 `rag_doc_filter_relevance` 控制；并发度 `index_filter_workers`
- 推荐实现：与 autocoder-nano 的 `_check_relevance_with_conversation` 等价的提示词与解析逻辑。

### 7. 模式与对接

- 上下文模式（contexts-only）：仅返回选中文档原文 JSON 序列，用于上层拼接或外部消费。
- 答复模式（answer）：将文档串入新 prompt 问答，输出流式回答，同时回传使用到的文档清单（便于追踪）。
- CLI：通过 `--contexts-only` 切换；其余行为与 autocoder-nano 的 `LongContextRAG.search/stream_chat_oai` 对齐。

### 8. 文件扫描与忽略规则

- 忽略规则优先级：`.serveignore` > `.gitignore` > 默认排除目录（如 `node_modules`, `.git`, `target`, `dist`, `build` 等）。
- 支持扩展白名单：`--required-exts ".rs,.py,.ts,.js,.go,.md"`（doc 层说明，解析由实现读取）。
- 遍历：`walkdir` + `ignore`，支持软链接 `followlinks=true`（谨慎处理避免循环）。

### 9. Token 预算与上下文塑形

- 分配：
  - `full_text_limit = rag_context_window_limit * full_text_ratio`
  - `segment_limit = rag_context_window_limit * segment_ratio`
  - `buff_limit = rag_context_window_limit * (1 - full_text_ratio - segment_ratio)`，需保证非负
- 小文件合并/大文件切分：
  - `single_file_token_limit = full_text_limit - 100`
  - `small_file_token_limit = single_file_token_limit / 4`
  - `small_file_merge_limit = single_file_token_limit / 2`
  - 文档 Token 统计由 `tokenizers` + `tokenizer.json` 实现，路径由 `--tokenizer-path`（或配置）提供
- 二轮筛选：第一轮全文、第二轮段落抽取与重排（可通过 `disable_segment_reorder` 关闭），最终不超过窗口限制。

### 10. 可观测性与进度输出

- 构建/更新：
  - 输出文件总数、待处理/待删除列表、批次进度、预计剩余时间、总时长
  - DuckDB 写入：批大小、并发度、已完成/总批次、异常批次详情
- 检索：
  - 过滤耗时、相关文档数量/列表、第一轮/第二轮文档数、最终发送 Token 总数
- 统一 Printer/Logger 接口，支持安静/详细模式切换。

### 11. 测试与基准

- 单元测试：
  - Token 统计（不同语言/字符集）
  - 小文件合并/大文件切分边界
  - Relevance 解析与阈值裁剪
  - DuckDB I/O 与向量查询（含降维/归一化）
- 集成测试：
  - 小/中/大项目端到端：构建 → 查询 → 过滤 → 上下文拼接
  - 增量更新：修改/删除/新增文件后索引更新正确性
- 基准：
  - N=1k/5k/20k 文件规模下的构建/更新/查询延迟与内存占用目标

## 集成实施计划

### 阶段1：基础架构搭建（第1-2周）
- [ ] 创建 RAG 模块目录结构
- [ ] 实现基础结构体定义
- [ ] 集成必要的依赖库
- [ ] 实现文件扫描和索引缓存

### 阶段2：核心功能实现（第3-4周）
- [ ] 实现 IndexManager 核心逻辑（含 JSONL 缓存与增量更新）
- [ ] 集成 tree-sitter 代码解析与 LLM 回退
- [ ] 实现 Embeddings API 与 DuckDB VSS 写入/查询
- [ ] 实现多级过滤流程与 contexts-only/answer 两种模式

### 阶段3：性能优化（第5周）
- [ ] 实现并行处理和异步操作
- [ ] 优化索引存储和检索性能
- [ ] 实现智能缓存机制
- [ ] 添加进度显示和错误处理

### 阶段4：用户体验（第6周）
- [ ] 完善 CLI 参数和帮助文档
- [ ] 实现交互式索引构建
- [ ] 添加详细的日志和调试信息
- [ ] 编写使用示例和测试用例

### 阶段5：测试和文档（第7周）
- [ ] 编写单元测试和集成测试
- [ ] 性能基准测试
- [ ] 完善用户文档
- [ ] 准备发布版本

## 使用示例

```bash
# 构建代码索引
sgpt --build-index --source-dir ./my-project

# 启用 RAG 模式进行代码问答
sgpt --rag --source-dir ./my-project "如何实现用户认证功能？"

# 指定过滤级别和文件数量
sgpt --rag --filter-level 2 --max-files 5 "找到所有数据库相关的文件"

# 查询索引中的特定符号
sgpt --query-index "@@authenticate" --source-dir ./my-project

# 结合现有功能使用
sgpt -s --rag "生成一个数据库连接的shell脚本" --source-dir ./my-project

# 仅返回上下文（不直接回答）
sgpt --rag --contexts-only --source-dir ./my-project '{"query":"列出数据库相关配置","only_contexts":true}'
```

## 预期效果

1. **智能上下文检索**：自动识别与查询相关的代码文件
2. **多级过滤机制**：从粗筛到精筛，确保结果相关性
3. **高性能处理**：并行索引构建，快速检索响应
4. **灵活配置**：支持多种过滤级别和自定义参数
5. **良好用户体验**：清晰的进度显示和错误提示

通过这个规划，sgpt-rs 将获得与 autocoder-nano 相媲美的代码检索和上下文管理能力，同时保持 Rust 的性能优势。

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    config::Config,
    external::tavily::TavilyClient,
    llm::{ChatMessage, ChatOptions, LlmClient, Role, StreamEvent},
    printer::MarkdownPrinter,
};

#[derive(Debug, Serialize, Deserialize)]
struct SearchQuery {
    query: String,
    purpose: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchPlan {
    queries: Vec<SearchQuery>,
}

#[derive(Debug)]
struct SearchResult {
    query: String,
    results: Vec<SearchItem>,
}

#[derive(Debug)]
struct SearchItem {
    title: String,
    url: String,
    snippet: String,
}

pub struct EnhancedSearchHandler {
    llm_client: LlmClient,
    tavily_client: TavilyClient,
    markdown_enabled: bool,
}

impl EnhancedSearchHandler {
    pub fn new(config: &Config, md_enabled: bool) -> Result<Self> {
        let llm_client = LlmClient::from_config(config)?;
        let tavily_client = TavilyClient::from_config(config)?;

        Ok(Self {
            llm_client,
            tavily_client,
            markdown_enabled: md_enabled,
        })
    }

    pub async fn run(
        query: &str,
        model: &str,
        temperature: Option<f32>,
        top_p: Option<f32>,
        config: &Config,
        md_enabled: bool,
    ) -> Result<()> {
        let mut handler = Self::new(config, md_enabled)?;
        
        println!("🔍 Step 1: Analyzing intent and building search queries...");
        let search_plan = handler.analyze_intent_and_build_queries(query, model, temperature, top_p).await?;
        
        println!("📊 Generated {} search queries:", search_plan.queries.len());
        for (i, sq) in search_plan.queries.iter().enumerate() {
            println!("  {}. {} ({})", i + 1, sq.query, sq.purpose);
        }
        
        println!("\n🔎 Step 2: Executing multi-dimensional search...");
        let search_results = handler.execute_multi_search(&search_plan.queries).await?;
        
        println!("📝 Step 3: Analyzing results and generating comprehensive answer...\n");
        handler.generate_final_answer(query, &search_results, model, temperature, top_p).await?;

        Ok(())
    }

    async fn analyze_intent_and_build_queries(
        &self,
        user_query: &str,
        model: &str,
        temperature: Option<f32>,
        top_p: Option<f32>,
    ) -> Result<SearchPlan> {
        let system_prompt = r#"You are a search query planning expert. Your task is to analyze the user's question and create 3 different search queries that will help gather comprehensive information to answer their question.

For each search query, provide:
1. The actual search query string
2. A brief purpose explaining what aspect this query covers

Return your response as JSON in this exact format:
{
  "queries": [
    {"query": "search term 1", "purpose": "covers main topic"},
    {"query": "search term 2", "purpose": "covers related aspect"},
    {"query": "search term 3", "purpose": "covers context/background"}
  ]
}

Guidelines:
- Make queries specific and focused
- Cover different angles: main topic, related concepts, recent developments
- Use keywords that are likely to find relevant results
- Keep queries concise but informative"#;

        let user_message = format!("Please analyze this question and create 3 search queries: {}", user_query);

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
            model: model.to_string(),
            temperature: temperature.unwrap_or(0.0),
            top_p: top_p.unwrap_or(1.0),
            tools: None,
            parallel_tool_calls: false,
            tool_choice: None,
            max_tokens: Some(1024), // Increased for search queries parsing
        };

        let mut stream = self.llm_client.chat_stream(messages, opts);
        let mut response = String::new();
        while let Some(ev) = futures_util::StreamExt::next(&mut stream).await {
            match ev? {
                StreamEvent::Content(t) => response.push_str(&t),
                StreamEvent::Done => break,
                _ => {},
            }
        }

        // Parse the JSON response
        let search_plan: SearchPlan = serde_json::from_str(&response.trim())
            .map_err(|e| anyhow::anyhow!("Failed to parse search plan JSON: {}", e))?;

        if search_plan.queries.len() != 3 {
            bail!("Expected exactly 3 search queries, got {}", search_plan.queries.len());
        }

        Ok(search_plan)
    }

    async fn execute_multi_search(&self, queries: &[SearchQuery]) -> Result<Vec<SearchResult>> {
        let mut results = Vec::new();
        
        for query in queries {
            println!("  Searching: {}", query.query);
            match self.tavily_client.search(&query.query).await {
                Ok(value) => {
                    let search_items = self.parse_tavily_results(&value);
                    results.push(SearchResult {
                        query: query.query.clone(),
                        results: search_items,
                    });
                }
                Err(e) => {
                    println!("  ⚠️  Search failed for '{}': {}", query.query, e);
                    results.push(SearchResult {
                        query: query.query.clone(),
                        results: Vec::new(),
                    });
                }
            }
        }
        
        Ok(results)
    }

    fn parse_tavily_results(&self, value: &Value) -> Vec<SearchItem> {
        let mut items = Vec::new();
        
        if let Some(results) = value.get("results").and_then(|v| v.as_array()) {
            for item in results {
                let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let snippet = item
                    .get("snippet")
                    .or_else(|| item.get("content"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                
                items.push(SearchItem { title, url, snippet });
            }
        }
        
        items
    }

    async fn generate_final_answer(
        &mut self,
        user_query: &str,
        search_results: &[SearchResult],
        model: &str,
        temperature: Option<f32>,
        top_p: Option<f32>,
    ) -> Result<()> {
        let system_prompt = r#"You are a helpful assistant that provides comprehensive answers based on web search results. 

Your task:
1. Analyze the provided search results
2. Synthesize information from multiple sources
3. Provide a well-structured, informative answer to the user's question
4. Include relevant details and context
5. Cite sources when appropriate using the format [Source: URL]

Guidelines:
- Be accurate and factual
- Organize information clearly
- Highlight key points
- Provide context and background when helpful
- If information is conflicting, mention different perspectives"#;

        // Format search results for the prompt
        let mut context = String::new();
        context.push_str("Search Results:\n\n");
        
        for (i, result) in search_results.iter().enumerate() {
            context.push_str(&format!("Query {}: {}\n", i + 1, result.query));
            for (j, item) in result.results.iter().enumerate() {
                context.push_str(&format!("{}. {}\n", j + 1, item.title));
                context.push_str(&format!("   URL: {}\n", item.url));
                context.push_str(&format!("   Content: {}\n", item.snippet));
                context.push_str("\n");
            }
            context.push_str("\n");
        }

        let user_message = format!(
            "Based on the search results below, please answer this question: {}\n\n{}",
            user_query, context
        );

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
            model: model.to_string(),
            temperature: temperature.unwrap_or(0.0),
            top_p: top_p.unwrap_or(1.0),
            tools: None,
            parallel_tool_calls: false,
            tool_choice: None,
            max_tokens: Some(4096), // Much larger for comprehensive final answer
        };

        let mut stream = self.llm_client.chat_stream(messages, opts);
        let mut assistant_text = String::new();

        while let Some(chunk) = futures_util::StreamExt::next(&mut stream).await {
            match chunk {
                Ok(StreamEvent::Content(content)) => {
                    assistant_text.push_str(&content);
                    if !self.markdown_enabled {
                        print!("{}", content);
                    }
                }
                Ok(StreamEvent::Done) => break,
                Ok(_) => {}, // Other events
                Err(e) => {
                    eprintln!("Stream error: {}", e);
                    break;
                }
            }
        }

        if self.markdown_enabled && !assistant_text.is_empty() {
            MarkdownPrinter::default().print(&assistant_text);
        } else if !self.markdown_enabled {
            println!(); // Add final newline for non-markdown
        }
        Ok(())
    }
}
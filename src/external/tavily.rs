use anyhow::{bail, Result};
use reqwest::{Client, StatusCode};
use serde_json::Value;

use crate::config::Config;

const DEFAULT_TAVILY_BASE: &str = "https://api.tavily.com";

pub struct TavilyClient {
    client: Client,
    base: String,
    api_key: String,
}

impl TavilyClient {
    pub fn from_config(cfg: &Config) -> Result<Self> {
        let api_key = cfg
            .get("TVLY_API_KEY")
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("Missing TVLY_API_KEY. Set it in env or ~/.config/sgpt_rs/.sgptrc"))?;

        // Optional: allow override via TAVILY_API_BASE; default to official endpoint
        let base = cfg
            .get("TAVILY_API_BASE")
            .unwrap_or_else(|| DEFAULT_TAVILY_BASE.to_string());

        // Honor REQUEST_TIMEOUT if present; default 60s
        let timeout_secs = cfg
            .get("REQUEST_TIMEOUT")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60);

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()?;

        Ok(Self { client, base, api_key })
    }

    pub async fn search(&self, query: &str) -> Result<Value> {
        let url = format!("{}/search", self.base.trim_end_matches('/'));
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&serde_json::json!({ "query": query }))
            .send()
            .await?;

        match resp.status() {
            StatusCode::OK => Ok(resp.json::<Value>().await?),
            status => {
                let text = resp.text().await.unwrap_or_default();
                bail!("Tavily search failed: {} - {}", status, text)
            }
        }
    }
}

// Convenience helper when you don't want to manage a client explicitly.
#[allow(dead_code)]
pub async fn search_with_config(cfg: &Config, query: &str) -> Result<Value> {
    let client = TavilyClient::from_config(cfg)?;
    client.search(query).await
}


//! Request cache (TBD) and chat session persistence.

use std::{ fs, path::PathBuf };

use anyhow::Result;
// serde traits not needed directly here; use serde_json helpers

use crate::{ config::Config, llm::ChatMessage };

#[allow(dead_code)]
pub struct Cache;

#[derive(Debug, Clone)]
pub struct ChatSession {
    length: usize,
    storage_path: PathBuf,
}

impl ChatSession {
    pub fn from_config(cfg: &Config) -> Self {
        let len = cfg
            .get("CHAT_CACHE_LENGTH")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(100);
        let path = cfg.chat_cache_path();
        let _ = fs::create_dir_all(&path);
        Self { length: len, storage_path: path }
    }

    fn file_path(&self, chat_id: &str) -> PathBuf {
        self.storage_path.join(chat_id)
    }

    pub fn exists(&self, chat_id: &str) -> bool {
        self.file_path(chat_id).exists()
    }

    pub fn invalidate(&self, chat_id: &str) {
        let _ = fs::remove_file(self.file_path(chat_id));
    }

    pub fn read(&self, chat_id: &str) -> Result<Vec<ChatMessage>> {
        let p = self.file_path(chat_id);
        if !p.exists() {
            return Ok(Vec::new());
        }
        let text = fs::read_to_string(p)?;
        let msgs: Vec<ChatMessage> = serde_json::from_str(&text)?;
        Ok(msgs)
    }

    pub fn write(&self, chat_id: &str, mut messages: Vec<ChatMessage>) -> Result<()> {
        // Retain the first message (system role), truncate the rest to at most `length`.
        if messages.len() > 1 {
            let keep = self.length;
            let len = messages.len();
            let over = len.saturating_sub(keep) as isize; // how many over total length
            let start = if over > 0 { 1 + over as usize } else { 1 };
            let mut truncated = Vec::with_capacity(1 + keep);
            truncated.push(messages.remove(0));
            let slice = messages.into_iter().skip(start - 1).collect::<Vec<_>>();
            truncated.extend(slice);
            messages = truncated;
        }

        let p = self.file_path(chat_id);
        fs::write(p, serde_json::to_string(&messages)?)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn list(&self) -> Vec<PathBuf> {
        if let Ok(read_dir) = fs::read_dir(&self.storage_path) {
            let mut files: Vec<PathBuf> = read_dir.filter_map(|e| e.ok().map(|e| e.path())).collect();
            files.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
            files
        } else {
            Vec::new()
        }
    }
}

#[derive(Debug, Clone)]
pub struct RequestCache {
    length: usize,
    cache_path: PathBuf,
}

impl RequestCache {
    pub fn from_config(cfg: &Config) -> Self {
        let len = cfg
            .get("CACHE_LENGTH")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(100);
        let path = cfg.cache_path();
        let _ = fs::create_dir_all(&path);
        Self { length: len, cache_path: path }
    }

    pub fn key_for(
        &self,
        base_url: &str,
        model: &str,
        temperature: f32,
        top_p: f32,
        messages: &Vec<ChatMessage>,
    ) -> String {
        let payload = serde_json::json!({
            "base_url": base_url,
            "model": model,
            "temperature": temperature,
            "top_p": top_p,
            "messages": messages,
        });
        let data = serde_json::to_vec(&payload).unwrap_or_default();
        let digest = md5::compute(data);
        format!("{:x}", digest)
    }

    pub fn get(&self, key: &str) -> Option<String> {
        let p = self.cache_path.join(key);
        fs::read_to_string(p).ok()
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        let p = self.cache_path.join(key);
        fs::write(p, value)?;
        self.prune()?;
        Ok(())
    }

    fn prune(&self) -> Result<()> {
        let mut entries: Vec<_> = fs::read_dir(&self.cache_path)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
        if entries.len() > self.length {
            let to_delete = entries.len() - self.length;
            for i in 0..to_delete {
                let _ = fs::remove_file(entries[i].path());
            }
        }
        Ok(())
    }
}

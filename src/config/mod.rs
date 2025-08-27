use std::{
    collections::HashMap,
    env,
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use directories::BaseDirs;

#[derive(Debug, Clone)]
pub struct Config {
    inner: HashMap<String, String>,
    pub config_path: PathBuf,
}

impl Config {
    pub fn load() -> Self {
        let mut map = default_map();
        let config_path = default_config_path();

        // Read .sgptrc if exists
        if config_path.exists() {
            if let Ok(file) = fs::File::open(&config_path) {
                let reader = BufReader::new(file);
                for line in reader.lines().flatten() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((k, v)) = line.split_once('=') {
                        map.insert(k.trim().to_string(), v.trim().to_string());
                    }
                }
            }
        }

        // Overlay environment variables (take precedence)
        for (k, v) in env::vars() {
            if is_config_key(&k) {
                map.insert(k, v);
            }
        }

        Self { inner: map, config_path }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        // ENV first
        if let Ok(v) = env::var(key) {
            return Some(v);
        }
        self.inner.get(key).cloned()
    }

    pub fn get_bool(&self, key: &str) -> bool {
        self.get(key)
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    }

    pub fn get_usize(&self, key: &str) -> Option<usize> {
        self.get(key).and_then(|v| v.parse::<usize>().ok())
    }

    pub fn get_path(&self, key: &str) -> Option<PathBuf> {
        self.get(key).map(PathBuf::from)
    }

    pub fn chat_cache_path(&self) -> PathBuf {
        PathBuf::from(self.get("CHAT_CACHE_PATH").unwrap())
    }

    pub fn cache_path(&self) -> PathBuf {
        PathBuf::from(self.get("CACHE_PATH").unwrap())
    }

    pub fn roles_path(&self) -> PathBuf {
        PathBuf::from(self.get("ROLE_STORAGE_PATH").unwrap())
    }

    pub fn functions_path(&self) -> PathBuf {
        PathBuf::from(self.get("OPENAI_FUNCTIONS_PATH").unwrap())
    }
}

fn is_config_key(k: &str) -> bool {
    // Accept known keys or SGPT_*/OPENAI_* for forward-compat
    const KEYS: &[&str] = &[
        "OPENAI_API_KEY",
        "API_BASE_URL",
        "CHAT_CACHE_PATH",
        "CACHE_PATH",
        "CHAT_CACHE_LENGTH",
        "CACHE_LENGTH",
        "REQUEST_TIMEOUT",
        "DEFAULT_MODEL",
        "DEFAULT_COLOR",
        "ROLE_STORAGE_PATH",
        "DEFAULT_EXECUTE_SHELL_CMD",
        "DISABLE_STREAMING",
        "CODE_THEME",
        "OPENAI_FUNCTIONS_PATH",
        "OPENAI_USE_FUNCTIONS",
        "SHOW_FUNCTIONS_OUTPUT",
        "PRETTIFY_MARKDOWN",
        "USE_LITELLM",
        "SHELL_INTERACTION",
        "OS_NAME",
        "SHELL_NAME",
    ];

    KEYS.contains(&k) || k.starts_with("SGPT_") || k.starts_with("OPENAI_")
}

fn default_config_path() -> PathBuf {
    let base = BaseDirs::new()
        .map(|b| b.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("~/.config"));
    base.join("shell_gpt").join(".sgptrc")
}

fn default_map() -> HashMap<String, String> {
    let mut m = HashMap::new();
    // Paths
    let base = BaseDirs::new()
        .map(|b| b.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("~/.config"));
    let sgpt_dir = base.join("shell_gpt");
    let temp = env::temp_dir().join("shell_gpt");

    m.insert(
        "CHAT_CACHE_PATH".into(),
        temp.join("chat_cache").to_string_lossy().into_owned(),
    );
    m.insert(
        "CACHE_PATH".into(),
        temp.join("cache").to_string_lossy().into_owned(),
    );
    m.insert("ROLE_STORAGE_PATH".into(), sgpt_dir.join("roles").to_string_lossy().into_owned());
    m.insert(
        "OPENAI_FUNCTIONS_PATH".into(),
        sgpt_dir.join("functions").to_string_lossy().into_owned(),
    );

    // Numbers
    m.insert("CHAT_CACHE_LENGTH".into(), "100".into());
    m.insert("CACHE_LENGTH".into(), "100".into());
    m.insert("REQUEST_TIMEOUT".into(), "60".into());

    // Strings
    m.insert("DEFAULT_MODEL".into(), "gpt-4o".into());
    m.insert("DEFAULT_COLOR".into(), "magenta".into());
    m.insert("CODE_THEME".into(), "dracula".into());
    m.insert("API_BASE_URL".into(), "default".into());
    m.insert("OS_NAME".into(), "auto".into());
    m.insert("SHELL_NAME".into(), "auto".into());

    // Bools as strings
    m.insert("DEFAULT_EXECUTE_SHELL_CMD".into(), "false".into());
    m.insert("DISABLE_STREAMING".into(), "false".into());
    m.insert("OPENAI_USE_FUNCTIONS".into(), "true".into());
    m.insert("SHOW_FUNCTIONS_OUTPUT".into(), "false".into());
    m.insert("PRETTIFY_MARKDOWN".into(), "true".into());
    m.insert("USE_LITELLM".into(), "false".into());
    m.insert("SHELL_INTERACTION".into(), "true".into());

    m
}


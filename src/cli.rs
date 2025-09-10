use clap::{ArgGroup, Parser};

#[derive(Parser, Debug, Clone)]
#[command(name = "sgpt", about = "ShellGPT Rust CLI", version)]
#[command(group(ArgGroup::new("mode").args(["shell", "describe_shell", "code", "search", "enhanced_search"]).multiple(false)))]
#[command(group(ArgGroup::new("chat_mode").args(["chat", "repl"]).multiple(false)))]
#[command(group(ArgGroup::new("md_switch").args(["md", "no_md"]).multiple(false)))]
#[command(group(ArgGroup::new("interaction_switch").args(["interaction", "no_interaction"]).multiple(false)))]
#[command(group(ArgGroup::new("cache_switch").args(["cache", "no_cache"]).multiple(false)))]
#[command(group(ArgGroup::new("functions_switch").args(["functions"]).multiple(false)))]
pub struct Cli {
    /// The prompt to generate completions for.
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,

    /// Large language model to use.
    #[arg(long)]
    pub model: Option<String>,

    /// Randomness of generated output.
    #[arg(long, default_value_t = 0.0, value_parser = clap::value_parser!(f32))]
    pub temperature: f32,

    /// Limits highest probable tokens (words).
    #[arg(long = "top-p", default_value_t = 1.0, value_parser = clap::value_parser!(f32))]
    pub top_p: f32,

    /// Prettify Markdown output (buffer then render at end).
    ///
    /// Note: default/--chat/--repl all use SSE streaming under the hood.
    /// - With `--md` (or `PRETTIFY_MARKDOWN=true`, default), output is buffered and printed as Markdown after completion.
    /// - Use `--no-md` (or set `PRETTIFY_MARKDOWN=false`) for realtime streaming to the terminal.
    #[arg(long)]
    pub md: bool,
    /// Disable Markdown prettifying (print chunks as they arrive).
    ///
    /// This enables realtime streaming in default/--chat/--repl modes.
    #[arg(long = "no-md")]
    pub no_md: bool,

    /// Generate and execute shell commands.
    #[arg(short = 's', long)]
    pub shell: bool,

    /// Override target shell for command generation (auto|powershell|cmd|bash|zsh|fish|sh).
    #[arg(long = "target-shell")]
    pub target_shell: Option<String>,

    /// Interactive mode for --shell option.
    #[arg(long)]
    pub interaction: bool,
    /// Disable interactive mode for --shell option.
    ///
    /// Shell mode: if explicitly provided, the generated command will be executed automatically
    /// (skips the confirmation menu). In non-TTY auto no-interaction cases, commands are not executed.
    #[arg(long = "no-interaction")]
    pub no_interaction: bool,

    /// Describe a shell command.
    #[arg(short = 'd', long = "describe-shell")]
    pub describe_shell: bool,

    /// Generate only code.
    #[arg(short = 'c', long = "code")]
    pub code: bool,

    /// Use Tavily to search the web for the prompt.
    #[arg(long = "search")]
    pub search: bool,

    /// Use enhanced search with multi-step analysis and comprehensive results.
    #[arg(short = 'e', long = "enhanced-search")]
    pub enhanced_search: bool,

    /// Process document files (.md, .txt) and use their content as context.
    /// Can be used multiple times: --doc file1.md --doc file2.txt
    #[arg(long = "doc", action = clap::ArgAction::Append)]
    pub doc: Vec<String>,

    /// Enable function calls (disabled by default).
    #[arg(long)]
    pub functions: bool,

    /// Open $EDITOR to provide a prompt.
    #[arg(long)]
    pub editor: bool,

    /// Cache completion results.
    #[arg(long)]
    pub cache: bool,
    /// Disable caching.
    #[arg(long = "no-cache")]
    pub no_cache: bool,

    /// Follow conversation with id, use "temp" for quick session.
    #[arg(long)]
    pub chat: Option<String>,

    /// Start a REPL (Read–eval–print loop) session.
    #[arg(long)]
    pub repl: Option<String>,

    /// Show all messages from provided chat id.
    #[arg(long = "show-chat")]
    pub show_chat: Option<String>,

    /// List all existing chat ids.
    #[arg(short = 'l', long = "list-chats", visible_alias = "lc")]
    pub list_chats: bool,

    /// System role for GPT model.
    #[arg(long)]
    pub role: Option<String>,

    /// Create role.
    #[arg(long = "create-role")]
    pub create_role: Option<String>,

    /// Show role.
    #[arg(long = "show-role")]
    pub show_role: Option<String>,

    /// List roles.
    #[arg(short = 'r', long = "list-roles", visible_alias = "lr")]
    pub list_roles: bool,

    /// Install shell integration (hidden).
    #[arg(long = "install-integration", hide = true)]
    pub install_integration: bool,

    /// Install default functions (hidden).
    #[arg(long = "install-functions", hide = true)]
    pub install_functions: bool,
}

impl Cli {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }
}

use clap::{ArgGroup, Parser};

#[derive(Parser, Debug, Clone)]
#[command(name = "sgpt", about = "ShellGPT Rust CLI", version)]
#[command(group(ArgGroup::new("mode").args(["shell", "describe_shell", "code"]).multiple(false)))]
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

    /// Prettify markdown output.
    #[arg(long)]
    pub md: bool,
    /// Disable markdown output.
    #[arg(long = "no-md")]
    pub no_md: bool,

    /// Generate and execute shell commands.
    #[arg(short = 's', long)]
    pub shell: bool,

    /// Interactive mode for --shell option.
    #[arg(long)]
    pub interaction: bool,
    /// Disable interactive mode for --shell option.
    #[arg(long = "no-interaction")]
    pub no_interaction: bool,

    /// Describe a shell command.
    #[arg(short = 'd', long = "describe-shell")]
    pub describe_shell: bool,

    /// Generate only code.
    #[arg(short = 'c', long = "code")]
    pub code: bool,

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

# sgpt-rs

English | [中文](README_zh.md)

This project is inspired by https://github.com/TheR1D/shell_gpt and is developed using Rust with assistance from Claude-Code and Codex, aiming to reduce configuration complexity.

## Quick Start

```bash
git clone https://github.com/xuzhougeng/sgpt-rs
cargo build --release
```

Move the compiled binary file to your executable directory.

```bash
mv target/release/sgpt ~/.local/bin
```

Alternatively, you can download pre-compiled binary files from <https://github.com/xuzhougeng/sgpt-rs/releases/>.

Edit ~/.config/sgpt_rs/.sgptrc to set DeepSeek as the default model.

```yaml
API_BASE_URL=https://api.deepseek.com
OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
DEFAULT_MODEL=deepseek-chat
```

You can also configure directly in environment variables or in ~/.zshrc, ~/.bashrc:

```bash
export API_BASE_URL=https://api.deepseek.com
export OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
export DEFAULT_MODEL=deepseek-chat
```

Generally supports all OpenAI API-compatible models.

Use sgpt directly to confirm it works properly:

```
$ sgpt
Hello! This is ShellGPT. How can I assist you with your shell command needs today? 😊
```

## Usage Examples

### Conversation

> By default, it waits for output completion before rendering. If you need streaming output, use the `--no-md` parameter or set the environment variable `export PRETTIFY_MARKDOWN=false`

Each interaction is a single round without context:

```bash
sgpt "say hi in one word"
Hi
```

Multi-turn conversation (start/continue session), parameter `--chat <id>`:

```bash
sgpt --chat test 'You are now Xiao Ai'
sgpt --chat test 'Who are you?'
```

View and manage sessions:

```bash
# View session history
sgpt --show-chat test

# View all sessions (and their paths)
sgpt --list-chats
```

Temporary session (don't save history):

```bash
sgpt --chat temp "This round won't be saved"
```

Start a "persistent interactive multi-turn session", parameter `--repl <id>`:

```bash
sgpt --repl test2
```

### SHELL

Generate shell commands, `--shell`:

```bash
sgpt --shell 'count rs files'
```

This will pop up a prompt `[E]xecute, [M]odify, [D]escribe, [A]bort:` for you to choose an action:

- [E]xecute: Execute
- [M]odify: Found the command is wrong, give more hints
- [D]escribe: Explain this command
- [A]bort: Exit directly without execution

This behavior can be set through the following two parameters or via SHELL_INTERACTION:

- `--interaction`: Manual confirmation required for command execution, default behavior
- `--no-interaction`: No interaction needed, run directly

Adding `--repl <id>` on top of shell enters interactive command line. After getting the command line, you can use 'e' to execute:

```bash
sgpt --shell --repl temp
Entering REPL mode, press Ctrl+C to exit.
Shell REPL shortcuts: e=execute, r=repeat, d=describe, p=print, m=modify; type exit() to quit.
> find rust file count
find . -name "*.rs" -type f | wc -l
> e
22
>  
```

### Windows & PowerShell Support

- Specify target shell: Use `--target-shell` to force generation of specific shell commands.
  - Available values: `auto`, `powershell`, `cmd`, `bash`, `zsh`, `fish`, `sh`
  - Examples:
    - Generate PowerShell commands: `sgpt -s --target-shell powershell "view files containing foo in current directory"`
    - Generate CMD commands: `sgpt -s --target-shell cmd "print PATH and exit"`
- Interactive execution: On Windows, will use PowerShell for execution based on `--target-shell` or auto-detection (otherwise fallback to CMD).
- Generation prompt optimization: When targeting PowerShell, prompts guide the model to prioritize PowerShell native commands (like `Get-ChildItem`, `Select-String`) and use `;` to connect multi-step commands (instead of `&&`).

## Document Processing

Support for directly processing document files, using file content as context for conversations:

```bash
# Single file
sgpt --doc document.md "your question"

# Multiple files
sgpt --doc file1.md --doc file2.txt --doc file3.md "your question"
```

**Supported File Types:**
- `.md` - Markdown files
- `.txt` - Plain text files  
- `.rst` - reStructuredText files
- `.log` - Log files
- Files without extension

This feature is equivalent to `cat xxx.md yyy.md | sgpt 'xxx'` but more convenient with direct file path usage.

TODO: Currently just passes all content as input to LLM, may implement individual file editing later.

## Web Search Features

Support for web searching using Tavily, with two search modes available:

### Configuration

- Environment variable configuration:
  - `export TVLY_API_KEY=tvly_xxxxxxxxxxxxx`
  - Optional: `export TAVILY_API_BASE=https://api.tavily.com`
- Or add in `~/.config/sgpt_rs/.sgptrc`:
  - `TVLY_API_KEY=tvly_xxxxxxxxxxxxx`
  - `TAVILY_API_BASE=https://api.tavily.com`

### Basic Search

Directly returns search results with titles, URLs, and snippets:

```bash
sgpt --search "Who is Leo Messi?"
echo "recent Rust release" | sgpt --search
```

### Enhanced Search 🚀

Three-step intelligent search process for comprehensive analysis:

```bash
# Full parameter
sgpt --enhanced-search "Who is Leo Messi?"

# Or use shorthand
sgpt -e "Who is Leo Messi?"
```

Enhanced search workflow:
1. **Intent Analysis**: AI analyzes the question and builds 3 sets of search queries from different angles
2. **Multi-dimensional Retrieval**: Executes multiple searches in parallel to gather comprehensive information
3. **Comprehensive Answer**: Generates detailed synthesis based on search results (supports up to 4096 tokens for detailed responses)

The program prioritizes outputting result titles, URLs, and summaries. If the structure doesn't contain common fields, it will output in JSON format as-is.

## Detailed Documentation

- `doc/Config.md`: Parameter descriptions for configurable options in .config/sgpt_rs/.sgptrc
- `doc/Role.md`: Detailed description of role configuration related parameters  
- `doc/Model.md`: Built-in OpenAI Chat Completions compatible streaming client with tool-calling support. Covers configuration, data structures, streaming events, error hints and usage examples

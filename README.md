# sgpt-rs

English | [中文](README_zh.md)

This project is inspired by https://github.com/TheR1D/shell_gpt and is developed using Rust.

## Quick Start

```bash
git clone https://github.com/xuzhougeng/sgpt-rs
cargo build --release
```

Move the compiled binary file to your executable directory.

```bash
mv target/release/sgpt ~/.local/bin
```

Edit ~/.config/sgpt_rs/.sgptrc to set DeepSeek as the default model.

```yaml
API_BASE_URL=https://api.deepseek.com
OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
DEFAULT_MODEL=deepseek-chat
```

You can also set directly in environment variables.

```bash
export API_BASE_URL=https://api.deepseek.com
export OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
export DEFAULT_MODEL=deepseek-chat
```

Use sgpt directly.

```
$ sgpt
Hello! This is ShellGPT. How can I assist you with your shell command needs today? 😊
```

Send a message.

```
$ sgpt "say hi in one word"
Hi
```

## Document Processing

Support for directly processing document files, using file content as context for conversations:

```bash
# Process document with question
sgpt --doc README.md "What is this project about?"

# Process document only (equivalent to cat README.md | sgpt)
sgpt --doc notes.txt

# Combine with other parameters
sgpt --doc changelog.log "What changed recently?" --md
```

**Supported File Types:**
- `.md` - Markdown files
- `.txt` - Plain text files  
- `.rst` - reStructuredText files
- `.log` - Log files
- Files without extension

This feature is equivalent to `cat xxx.md | sgpt 'xxx'` but more convenient with direct file path usage.

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
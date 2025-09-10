# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is `sgpt-rs`, a Rust rewrite of ShellGPT - a command-line tool for interacting with language models. It provides multiple modes including shell command generation, code generation, conversational chat, and REPL sessions.

## Build and Development Commands

- **Build (debug)**: `cargo build`
- **Build (release)**: `cargo build --release`
- **Run locally**: `cargo run -- <args>` (e.g., `cargo run -- --help`)
- **Format code**: `cargo fmt --all`
- **Lint code**: `cargo clippy --all-targets -- -D warnings`
- **Run tests**: `cargo test`
- **Clean build artifacts**: `cargo clean`
- **Install locally**: After release build, move `target/release/sgpt` to your PATH (e.g., `~/.local/bin`)

## Architecture Overview

The application is structured around a handler-based architecture:

### Core Modules
- **`src/main.rs`**: Entry point with argument parsing, configuration loading, and routing to appropriate handlers
- **`src/cli.rs`**: Command-line interface definition using Clap with argument groups and validation
- **`src/handlers/`**: Different operational modes:
  - `default.rs`: Standard conversational mode
  - `shell.rs`: Shell command generation with optional interaction
  - `code.rs`: Code-only generation
  - `describe.rs`: Shell command explanation
  - `chat.rs`: Persistent chat sessions
  - `repl.rs`: Interactive REPL sessions
  - `enhanced_search.rs`: Enhanced search functionality

### Supporting Systems
- **`src/config/`**: Configuration management (env vars + `~/.config/sgpt_rs/.sgptrc`)
- **`src/llm/`**: Language model integration and API communication
- **`src/cache/`**: Chat session persistence and caching
- **`src/printer/`**: Output formatting, including markdown rendering
- **`src/role/`**: System role management (default and custom roles)
- **`src/functions/`**: Function calling capabilities
- **`src/integration/`**: Shell integration (bash/zsh completion)
- **`src/external/`**: Third-party integrations (Tavily search)
- **`src/utils/`**: Common utilities including:
  - `command.rs`: Shell command execution across platforms
  - `document.rs`: Document reading and processing for multiple file formats
  - `pdf.rs`: PDF text extraction utilities

### Key Patterns
- Uses `anyhow::Result` for error handling throughout
- Async/await with Tokio for HTTP requests and streaming responses
- Configuration precedence: CLI args → environment variables → config file → defaults
- stdin/stdout handling with pipe support and TTY detection
- Markdown rendering with `termimad` when enabled

### Configuration
Configuration is loaded from:
1. Environment variables: `API_BASE_URL`, `OPENAI_API_KEY`, `DEFAULT_MODEL`
2. Config file: `~/.config/sgpt_rs/.sgptrc` (YAML format)
3. CLI arguments override both

The application supports multiple LLM providers through configurable API base URLs (OpenAI, DeepSeek, etc.).

## Code Conventions

- Follow Rust 2021 idioms and `rustfmt` defaults
- Use `snake_case` for functions/modules, `CamelCase` for types, `SCREAMING_SNAKE_CASE` for constants
- Keep handler-specific code in `src/handlers/` modules
- Use explicit error propagation with `?` operator
- Commit messages follow Conventional Commits format (`feat:`, `fix:`, `chore:`, etc.)
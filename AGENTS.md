# AGENTS.md

Guidance for AI coding agents (e.g., Codex CLI, ChatGPT) contributing to this repository. Keep changes focused, minimal, and aligned with project conventions.

---

# Repository Guidelines

## Project Structure & Module Organization
- `src/`: Rust sources. Key modules: `main.rs` (entry), `cli.rs` (CLI args), `handlers/` (command modes), `llm/`, `config/`, `printer/`, `utils/`, `cache/`, `integration/`.
- `Cargo.toml`: crate metadata and dependencies.
- `target/`: build artifacts (ignored by Git).
- Config: `~/.config/sgpt_rs/.sgptrc` or environment variables.

## Build, Test, and Development Commands
- Build (debug/release): `cargo build` / `cargo build --release`.
- Run locally: `cargo run -- <args>` (e.g., `cargo run -- --help`).
- Format: `cargo fmt --all`.
- Lint: `cargo clippy --all-targets -- -D warnings`.
- Clean: `cargo clean`.
- Install locally (example): after release build, move `target/release/sgpt` into your `PATH` (e.g., `~/.local/bin`).

## Coding Style & Naming Conventions
- Follow Rust 2021 idioms and `rustfmt` defaults (4-space indent, line width per tool).
- Use `snake_case` for functions/modules, `CamelCase` for types/traits, `SCREAMING_SNAKE_CASE` for constants.
- Prefer explicit error types via `anyhow::Result` at boundaries; propagate with `?`.
- Keep modules focused; colocate handler-specific code in `src/handlers/`.

## Testing Guidelines
- Framework: standard Rust tests.
- Unit tests: colocate with modules using `#[cfg(test)]` in `src/*`.
- Integration tests: add files under `tests/` (creates a separate crate).
- Run all tests: `cargo test`.
- Aim to cover new behavior in PRs; include happy-path and basic error cases.

## Commit & Pull Request Guidelines
- Commit style: Conventional Commits observed (`feat:`, `fix:`, `ci:`, `docs:`, `chore:`) with optional scopes (e.g., `feat(handlers): ...`). Keep messages imperative and concise.
- PR checklist:
  - Clear description of change and rationale; link related issues.
  - Include usage notes or CLI examples when behavior changes.
  - Ensure `cargo fmt` and `cargo clippy` pass; `cargo test` is green.
  - For user-visible changes, update `README.md` as needed.

## Security & Configuration Tips
- API configuration via env or `~/.config/sgpt_rs/.sgptrc`:
  - `API_BASE_URL`, `OPENAI_API_KEY`, `DEFAULT_MODEL`.
- Never commit secrets. Use local env files or CI secrets.
- When adding network calls (`reqwest`), use `rustls-tls` (already enabled) and handle timeouts/retries thoughtfully.

---

## Agent Workflow (Codex CLI)

- Preambles: Before running tools, write a 1–2 sentence note describing the next action(s).
- Planning: For multi-step work, maintain a lightweight plan using the `update_plan` tool with clear, sequential steps.
- Editing files: Always use `apply_patch` to add/update files. Keep changes scoped; avoid unrelated refactors.
- Shell usage:
  - Prefer `rg`/`rg --files` to search; read files in chunks (<=250 lines).
  - Respect sandbox: filesystem is workspace-limited; network is restricted.
  - Request escalation only when necessary and explain why.
- Validation: When appropriate, run `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` to verify changes.
- Error handling: Propagate with `anyhow::Result` and `?`. Don’t introduce panics in library-like code paths.
- Scope discipline: Fix the task at hand; do not alter unrelated code or rename files unless necessary.
- Output style: Be concise and action-oriented in messages. Use short status updates during longer work.

### Common Commands

- Build: `cargo build` / `cargo build --release`
- Run: `cargo run -- <args>` (e.g., `cargo run -- --help`)
- Format: `cargo fmt --all`
- Lint: `cargo clippy --all-targets -- -D warnings`
- Test: `cargo test`

### Architecture Pointers

- Entry: `src/main.rs`; CLI: `src/cli.rs`
- Handlers: `src/handlers/` (mode-specific logic)
- Config: `src/config/` (env + `~/.config/sgpt_rs/.sgptrc`)
- LLM/API: `src/llm/`
- Output: `src/printer/`
- Cache/Sessions: `src/cache/`
- Integrations: `src/integration/`
- Utilities: `src/utils/` (I/O, command exec, doc/PDF helpers)

### PR & Commit Hygiene

- Commits: Conventional Commits (`feat:`, `fix:`, `docs:`, `chore:` …). Imperative, concise.
- PRs: Explain intent and scope, include usage examples if behavior changes, ensure format/lint/tests pass, and update `README.md` for user-facing changes.

### Security Notes

- Do not log or commit secrets. Use environment variables or local configs.
- Prefer `rustls` TLS; set sensible HTTP timeouts and consider retries with backoff for new network code.

---

For a higher-level project overview, also see `README.md` and `CLAUDE.md`.

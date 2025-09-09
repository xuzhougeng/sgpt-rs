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

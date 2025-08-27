# sgpt-rs

English | [中文](README_zh.md)

This project is inspired by https://github.com/TheR1D/shell_gpt and implemented in Rust.

## Quick Start

1) Set environment variables (overrides `.sgptrc`):

```
export API_BASE_URL=https://api.deepseek.com
export OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
export DEFAULT_MODEL=deepseek-chat
```

2) Build and view help:

```
cargo build
cargo run -- --help
```

3) One-shot prompt (streams tokens to stdout):

```
cargo run -- "say hi in one word"
```

4) Chat sessions (persisted):

```
# start or continue a session
cargo run -- --chat mychat "say hi"
cargo run -- --chat mychat "and now say bye"
```

Chat history is stored as JSON at `${TMPDIR}/shell_gpt/chat_cache/<chat_id>` by default
(for example: `/tmp/shell_gpt/chat_cache/mychat`). Override with `CHAT_CACHE_PATH`.

5) REPL (temporary session):

```
cargo run -- --repl temp
>>> Hello in 3 words
Hello there friend
>>> """
... First line
... Second line
... """
<model output>
>>> exit()
```

6) Show/list chats (colored/Markdown):

```
cargo run -- --list-chats
cargo run -- --show-chat mychat        # colored, role-separated
cargo run -- --show-chat mychat --md   # Markdown render (termimad)
```

7) Stdin piping and delimiter (`__sgpt__eof__`):

```
printf "context from stdin\n" | cargo run -- "and argument prompt"

# For REPL, text before __sgpt__eof__ is used as the init prompt;
# the rest becomes interactive input from the terminal/tty.
printf "This is init\n__sgpt__eof__\nNext line from tty\nexit()\n" | cargo run -- --repl temp
```

8) Model and parameters:

```
# model resolves as: CLI --model > DEFAULT_MODEL (env/config) > gpt-4o
cargo run -- --model deepseek-chat "hello"

# Set sampling parameters
cargo run -- --temperature 0.5 --top-p 0.9 "short answer please"
```

## Shell Mode (interactive and auto-exec)

Typical flow: generate command → prompt `[E]xecute, [M]odify, [D]escribe, [A]bort`.

```
# interactive execution
cargo run -- -s "create a temp file named x and write hello"
# press e to execute, d to describe, m to modify and regenerate
```

Default auto-execute (Enter = execute):

```
export DEFAULT_EXECUTE_SHELL_CMD=true
cargo run -- -s "echo hello"
# pressing Enter at the prompt executes the command
```

Non-interactive mode (print only, do not execute):

```
cargo run -- -s "list files" --no-interaction
```

TTY detection: when stdin is not a TTY (e.g., piped), it automatically switches to non-interactive and only prints.

Note: commands can have side effects — execute carefully.

## Function Calling (tools)

Install the default function (local JSON tool definition):

```
cargo run -- --install-functions
# installs to ~/.config/shell_gpt/functions/execute_shell_command.json
```

By default, tools are allowed in Default/Chat modes (disable with `--no-functions`). If your backend does not support OpenAI tools (or returns 4xx), disable tool calling.

Define custom tools in JSON (example fields):

```
{
  "name": "execute_shell_command",
  "description": "Executes a shell command and returns the output.",
  "parameters": { "type": "object", "properties": {"cmd": {"type":"string"}}, "required":["cmd"] },
  "exec": { "program": "/bin/sh", "args_template": ["-c", "{{cmd}}"], "stdin": false, "timeout_sec": 60 }
}
```

More tool examples (put JSON files under `~/.config/shell_gpt/functions/`):

1) Open a browser link (`open_url.json`)

```
{
  "name": "open_url",
  "description": "Open a URL in default browser.",
  "parameters": {
    "type": "object",
    "properties": { "url": {"type": "string"} },
    "required": ["url"]
  },
  "exec": {
    "program": "/bin/sh",
    "args_template": ["-c", "xdg-open '{{url}}' >/dev/null 2>&1 || open '{{url}}'"] ,
    "stdin": false,
    "timeout_sec": 10
  }
}
```

2) Read a text file (`read_file.json`)

```
{
  "name": "read_file",
  "description": "Read a text file and return content.",
  "parameters": {
    "type": "object",
    "properties": { "path": {"type": "string"} },
    "required": ["path"]
  },
  "exec": {
    "program": "/bin/sh",
    "args_template": ["-c", "cat '{{path}}'"] ,
    "stdin": false,
    "timeout_sec": 10
  }
}
```

3) Write a text file (`write_file.json`, use with care)

```
{
  "name": "write_file",
  "description": "Write text content to a file (overwrite).",
  "parameters": {
    "type": "object",
    "properties": {
      "path": {"type": "string"},
      "content": {"type": "string"}
    },
    "required": ["path", "content"]
  },
  "exec": {
    "program": "/bin/sh",
    "args_template": ["-c", "printf '%s' '{{content}}' > '{{path}}'"] ,
    "stdin": false,
    "timeout_sec": 10
  }
}
```

Safety note: tools may have side effects. Prefer read-only operations and clearly constrain behavior in descriptions; disable tool calling when needed (`--no-functions`).

## REPL Shell Mode

In REPL, the assistant acts as a Shell agent and supports shortcuts:

- `e`: execute the current generated command
- `d`: describe the current command
- `m`: provide modification instructions and regenerate a new command
- `r`: repeat the last generated command
- `p`: print the last generated command
- Any other input: generate a new command from your text

Example:

```
cargo run -- --repl temp --shell
>>> list current folder
ls -l
>>> d
<describe the output and options of ls -l>
>>> m
Modify with instructions: add human readable sizes
ls -lh
>>> e
<execution result of ls -l>
>>> exit()
```

Disable REPL Shell interaction (treat e/d/m/r/p as plain input):

```
cargo run -- --repl temp --shell --no-interaction
```

## Notes and Status

- Markdown rendering: Default/Chat/REPL render in blocks when `--md` is on (termimad); otherwise stream tokens.
- Sessions and cache: sessions at `CHAT_CACHE_PATH`, request cache at `CACHE_PATH`; responses with `tool_calls` are not cached.
- `API_BASE_URL` automatically appends `/v1` if missing; `OPENAI_API_KEY` is sent as `Authorization: Bearer`.

Refer to top-level Plan.md for milestones and next steps.

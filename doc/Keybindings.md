# Keybindings and Input Handling

This document summarizes how Codex TUI processes keyboard input (shortcuts) and where the logic lives, followed by a Mermaid flowchart of the end‑to‑end path from terminal events to concrete handlers.

## Design Overview

- Layered handling with clear responsibilities:
  - TUI event source: converts crossterm events to `TuiEvent`.
  - App layer: intercepts global navigation/overlay/backtrack, then routes to `ChatWidget`.
  - ChatWidget layer: handles session‑wide shortcuts (Ctrl+C, Ctrl+V, queued messages), then routes to bottom pane.
  - Bottom pane / ChatComposer: popup routing (slash commands, `@` file search), submission rules, paste burst.
  - TextArea editor: Emacs‑style editing keys (word navigation, word deletion, line kill, etc.).
- Popups have first dibs when visible; otherwise input flows into the editor.
- Text editing treats special placeholders (large paste and images) as atomic elements so cursor/edits don’t split them.

## Where Things Live

- TUI event pump: `tui/src/tui.rs` (emits `TuiEvent::{Key, Paste, Draw, ...}`)
- App router: `tui/src/app.rs::{handle_tui_event, handle_key_event}`
- Widget router: `tui/src/chatwidget.rs::handle_key_event`
- Bottom pane / composer: `tui/src/bottom_pane/mod.rs`, `.../chat_composer.rs`
- Editor core: `tui/src/bottom_pane/textarea.rs`
- Approval modal keys: `tui/src/user_approval_widget.rs`
- Pager overlay keys: `tui/src/pager_overlay.rs`

## Global/Widget Shortcuts (examples)

- Ctrl+C: interrupt/quit hint (`ChatWidget::on_ctrl_c`).
- Ctrl+V: paste image from clipboard (`ChatWidget::handle_key_event` → `attach_image`).
- Ctrl+T: open transcript overlay (`App::handle_key_event`).
- Esc / Esc‑Esc backtrack (when composer empty): `App::handle_key_event` backtrack helpers.
- In popups (command/file): Up/Down to navigate, Enter/Tab to accept, Esc to close.

## Composer / Submission Rules

- Enter submits unless in paste‑burst context (then inserts newline) or popups consume it.
- Ctrl+D with empty composer exits (handled in `ChatComposer`).
- Paste handling (including large‑paste placeholder and image placeholders) is summarized in `docs/Paste.md`.

## Editor (TextArea) Emacs‑style Keys (non‑exhaustive)

- Word navigation:
  - Alt+Left / Alt+b (or Ctrl+Left): move to beginning of previous word.
  - Alt+Right / Alt+f (or Ctrl+Right): move to end of next word.
- Line navigation:
  - Home / End: beginning/end of line.
  - Ctrl+A / Ctrl+E: beginning/end of line with cross‑line behavior at BOL/EOL.
  - Up / Down: visual line aware; falls back to logical lines.
- Deletion:
  - Backspace / Ctrl+H: delete previous atomic boundary.
  - Delete / Ctrl+D: delete next atomic boundary.
  - Alt+Backspace / Ctrl+Alt+H / Ctrl+W: delete previous word.
  - Alt+Delete: delete next word.
  - Ctrl+U / Ctrl+K: kill to beginning / end of line.
- Terminal quirks:
  - Some terminals send `^B`/`^F` as C0 bytes without CONTROL; handled to avoid inserting control chars.

## Mermaid: Event → Handler Flow

```mermaid
flowchart TD
  subgraph Terminal
    CE[crossterm KeyEvent]
    PE[Paste Event]
  end

  CE --> TE[TUI: event_stream → TuiEvent::Key]
  PE --> TE2[TUI: event_stream → TuiEvent::Paste]

  TE --> A[App::handle_tui_event]
  TE2 --> A

  A -->|global keys (Esc/Esc‑Esc, Ctrl+T)| A1[Handled at App]
  A -->|else| W[ChatWidget::handle_key_event]

  W -->|Ctrl+C, Ctrl+V, Alt+Up (queue pop)| W1[Handled at ChatWidget]
  W -->|else| BP[BottomPane::handle_key_event]

  BP -->|popup active?| P{Popup}
  P -- Slash → CP[Command popup handles keys]
  P -- File → FP[File search popup handles keys]
  P -- None → CC[ChatComposer::handle_key_event_without_popup]

  CP -->|accept/close| CC
  FP -->|accept path| CC

  CC -->|editing keys| TA[TextArea::input]
  CC -->|Enter| SUB{Submit?}
  SUB -- Yes → S1[Compose submission]
  SUB -- No  → CC

  TA -->|move/delete/kill word, line, etc.| TA
```

## Notes

- Popups short‑circuit editing until they close.
- Large paste / image placeholders are inserted via `insert_element()`; editor treats them as atomic for navigation and deletion.
- Word boundaries and deletion are Unicode‑aware and cooperate with elements.


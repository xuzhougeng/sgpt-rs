# Paste Handling in Codex TUI

This document explains how the TUI handles pasted content, including the large‑paste placeholder feature and the non‑bracketed paste burst aggregator. It also shows a call‑flow diagram for the main paths.

## Overview

- Large pastes are not inserted verbatim; they are replaced in the composer with a compact placeholder of the form `[Pasted Content N chars]` and remembered internally for later expansion on submit.
- Normal‑sized pastes are inserted directly unless they are recognized as image file paths, in which case an image placeholder element is inserted and the image is attached for the next submission.
- Rapid "non‑bracketed" paste sequences are detected by a heuristic (`PasteBurst`) that buffers fast characters; upon flush the buffered string is handled via the same large/normal paste path.

## Key Constants and Data

- Threshold: `LARGE_PASTE_CHAR_THRESHOLD = 1000` (characters)
- Placeholder mapping: `pending_pastes: Vec<(placeholder: String, actual: String)>`
- Image attachments: stored with `placeholder` and `path`, inserted via `insert_element()`

Locations:
- Paste entrypoint: `tui/src/app.rs` → `TuiEvent::Paste(pasted)` → `ChatWidget::handle_paste()` → `ChatComposer::handle_paste()`
- Composer logic: `tui/src/bottom_pane/chat_composer.rs`
- Paste burst aggregator: `tui/src/bottom_pane/paste_burst.rs`

## Behavior Details

1) Handling a paste (explicit paste event or burst flush):
- Count characters: `char_count = pasted.chars().count()`
- If `char_count > LARGE_PASTE_CHAR_THRESHOLD`:
  - Create placeholder: `[Pasted Content {char_count} chars]`
  - Insert as an element: `textarea.insert_element(&placeholder)`
  - Record mapping: `pending_pastes.push((placeholder, pasted))`
- Else if content looks like an image path (`.png`, `.jpg`, `.jpeg`):
  - Compute dimensions; insert image placeholder element: `[image {width}x{height} {format}]`
  - Track for submission via `attach_image()`
- Else: insert text directly into the textarea

2) Submission (Enter key):
- If any `pending_pastes` exist, expand them before submitting:
  - Replace each placeholder in the text with its `actual` content
  - Clear `pending_pastes` and submit the expanded text
- Otherwise, normal submit rules apply (respecting paste‑burst Newline behavior)

3) Edits and deletion:
- After each edit, remove any `(placeholder, actual)` entries whose placeholders are no longer present in the composer text
- When Backspace/Delete is used at a placeholder boundary, remove the placeholder text atomically and drop the corresponding mapping (and image attachment, for image placeholders)

## Mermaid Flowchart

```mermaid
flowchart TD
  A[TuiEvent::Paste(pasted)] --> B[ChatWidget::handle_paste]
  B --> C[ChatComposer::handle_paste(pasted)]
  C --> D{chars.count() > THRESHOLD?}
  D -- yes --> E[Make "[Pasted Content N chars]" placeholder]
  E --> F[textarea.insert_element(placeholder)]
  F --> G[pending_pastes.push((placeholder, pasted))]
  D -- no --> H{Looks like image path?}
  H -- yes --> I[image::image_dimensions]
  I --> J[attach_image(); insert_element("[image WxH TYPE]")]
  H -- no --> K[textarea.insert_str(pasted)]

  %% Non-bracketed paste burst path
  L[KeyEvent::Char ... fast stream] --> M[PasteBurst buffers]
  M -->|flush_if_due| N[ChatComposer::handle_paste(buffered)]
  N --> D

  %% Submit path
  S[KeyEvent::Enter] --> T{pending_pastes.is_empty()?}
  T -- no --> U[Replace placeholders with actuals]
  U --> V[Submit expanded text]
  T -- yes --> W[Normal submit rules]

  %% Deletion path
  X[Backspace/Delete near placeholder] --> Y[try_remove_any_placeholder_at_cursor]
  Y --> Z[Remove placeholder text and drop mapping]
```

## Code References

- Threshold and main handler:
  - `tui/src/bottom_pane/chat_composer.rs` → `LARGE_PASTE_CHAR_THRESHOLD`
  - `ChatComposer::handle_paste`
  - `ChatComposer::handle_key_event_without_popup` (submit path)
  - `ChatComposer::try_remove_any_placeholder_at_cursor` (deletion)
- Paste burst:
  - `tui/src/bottom_pane/paste_burst.rs` and calls from `ChatComposer` (`on_plain_char`, `flush_if_due`, `flush_before_modified_input`)
- Paste event source:
  - `tui/src/app.rs` → `TuiEvent::Paste(pasted)`
- Image handling:
  - `ChatComposer::handle_paste_image_path`
  - `ChatComposer::attach_image` (inserts `[image WxH TYPE]` element)


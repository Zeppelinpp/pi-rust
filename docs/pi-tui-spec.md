# pi-tui Rewrite Specification

This document specifies the TypeScript `pi-tui` implementation from `/tmp/pi-mono/packages/tui` for translation to Rust. It is intended to be directly usable by a Rust developer doing the rewrite.

---

## 1. High-Level Architecture and Module Layout

### Core Files (TypeScript → Rust modules)

| TS File | Role |
|---------|------|
| `src/terminal.ts` | `Terminal` trait + `ProcessTerminal` implementation. Handles raw mode, Kitty protocol negotiation, bracketed paste, Windows VT input, write logging, and cleanup. |
| `src/tui.ts` | `TUI` struct (extends `Container`). Differential rendering engine, overlay stack, focus management, hardware cursor IME positioning, render throttling. |
| `src/keys.ts` | `KeyId` type, `matches_key()`, `parse_key()`, `decode_kitty_printable()`, Kitty/modifyOtherKeys parser, legacy sequence matching. |
| `src/keybindings.ts` | `Keybinding` ID registry, `KeybindingsManager`, global default definitions (`TUI_KEYBINDINGS`). |
| `src/stdin-buffer.ts` | `StdinBuffer` — splits batched stdin into individual complete escape sequences, handles bracketed paste extraction, timeout-based flush. |
| `src/utils.ts` | `visible_width`, `truncate_to_width`, `wrap_text_with_ansi`, `slice_by_column`, `extract_segments`, grapheme width via `Intl.Segmenter` equivalent, ANSI SGR tracking. |
| `src/terminal-image.ts` | Terminal capability detection, Kitty/iTerm2 image encoding, image dimension parsing (PNG/JPEG/GIF/WebP), cell-size queries. |
| `src/autocomplete.ts` | `AutocompleteProvider` trait, `CombinedAutocompleteProvider` (slash commands + file paths), `fd`-based fuzzy file search. |
| `src/editor-component.ts` | Small trait for custom editor components. |
| `src/components/editor.ts` | `Editor` component — multi-line text editor with word-wrap, vertical scrolling, autocomplete integration, undo, kill-ring, history, bracketed paste, large-paste markers. |
| `src/components/input.ts` | `Input` component — single-line text input with horizontal scrolling. |
| `src/components/markdown.ts` | `Markdown` component — renders Markdown to ANSI-styled terminal output (headings, lists, code blocks, blockquotes, tables, inline styles). |
| `src/components/image.ts` | `Image` component — renders terminal images via Kitty/iTerm2 protocols with fallbacks. |
| `src/components/box.ts` | `Box` component — padding + optional background color around children. |
| `src/components/text.ts` | `Text` component — multi-line wrapped text with padding and optional background. |
| `src/components/select-list.ts` | `SelectList` component — scrollable selectable list with primary/description columns. |
| `src/components/loader.ts` | `Loader` component — animated spinner using braille characters. |
| `src/components/cancellable-loader.ts` | `CancellableLoader` — loader with a stop action. |
| `src/components/truncated-text.ts` | `TruncatedText` — single-line text truncated with ellipsis. |
| `src/components/settings-list.ts` | `SettingsList` — labeled rows with current values. |
| `src/components/spacer.ts` | `Spacer` — empty space component. |
| `src/kill-ring.ts` | Emacs-style kill ring for cut/yank. |
| `src/undo-stack.ts` | Simple undo stack. |
| `src/fuzzy.ts` | Fuzzy matching/filtering for autocomplete. |

### Important Dependencies in TypeScript

- `marked` — Markdown lexer/parser
- `get-east-asian-width` — East Asian width lookup
- `xterm.js` (`@xterm/headless`) — Virtual terminal for testing
- `Intl.Segmenter` — Grapheme cluster segmentation

### Rust Crate Mapping Recommendations

Use these Rust crates to replace the above:

| Capability | Suggested Crate |
|------------|-----------------|
| Terminal raw mode / cursor / clear | `crossterm` |
| High-level TUI widgets (optional) | `ratatui` — **but only for widget primitives**; the custom differential renderer should be preserved because `ratatui` does not do incremental/differential rendering out of the box |
| Markdown parsing | `pulldown-cmark` |
| Grapheme segmentation / width | `unicode-segmentation` + `unicode-width` |
| Fuzzy matching | `nucleo` or `skim` matcher, or a simple custom scorer |
| Kitty graphics protocol | Custom (spec is simple OSC/APC sequences) |
| Image dimensions | `image` crate or small format-specific parsers |
| ANSI parsing in tests | `vte` or `xterm.js` via Node bindings for snapshot parity |

---

## 2. Rendering System

### 2.1 Differential Rendering (the heart of pi-tui)

The `TUI` struct maintains:
- `previous_lines: Vec<String>` — what was rendered last frame
- `previous_width`, `previous_height`
- `cursor_row`, `hardware_cursor_row`
- `max_lines_rendered`
- `previous_viewport_top`

On `request_render()`, a throttled render is scheduled (minimum 16 ms between renders). `do_render()` executes:

1. **Render components** to `new_lines`.
2. **Composite overlays** into `new_lines` if any are visible.
3. **Extract cursor position** by searching for `CURSOR_MARKER` in visible viewport lines, then strip it.
4. **Apply line resets** (`"\x1b[0m\x1b]8;;\x07"`) to every non-image line to prevent style/link bleed.

#### Full render triggers
- First render
- Terminal **width** changed
- Terminal **height** changed (except in Termux — see below)
- Content shrank below `max_lines_rendered` and `clear_on_shrink` is true and no overlays active
- Differential would need to touch lines above prior viewport top

#### Differential (fast-path) algorithm
1. Find `first_changed` and `last_changed` indices by comparing `new_lines` vs `previous_lines`.
2. If no changes → only reposition hardware cursor.
3. If all changes are in deleted lines → move cursor to end of new content, clear extra lines with `"\x1b[2K"` without scrolling.
4. Otherwise, move cursor to `first_changed` using ANSI cursor-up/down relative to `hardware_cursor_row`, then:
   - For each changed line: `"\r\x1b[2K"` (clear line) + new line content.
   - If previous had more lines than new, clear the excess.

All output is wrapped in **synchronized update** (`"\x1b[?2026h"` / `"\x1b[?2026l"`) to eliminate tearing.

### 2.3 Flicker-Free Output Techniques

- **Synchronized updates** (`CSI ? 2026`) around every frame.
- **Never clear the whole screen** on the fast path — only rewrite changed lines.
- **Cursor movement** uses relative `CSI A/B` from the last known hardware cursor row instead of absolute positioning.
- **Line resets appended** to every line prevent underline/color bleeding into padding.
- **Viewport tracking** avoids moving the cursor when changes are outside the visible area.
- **Termux special case**: Height changes do **not** trigger full redraws because the software keyboard causes continuous resize events; replaying the whole history would flood the terminal.

---

## 3. Terminal Components

All components implement:

```rust
pub trait Component {
    fn render(&self, width: u16) -> Vec<String>;
    fn handle_input(&mut self, data: &str); // optional
    fn invalidate(&mut self);
    fn wants_key_release(&self) -> bool { false }
}

pub trait Focusable: Component {
    fn set_focused(&mut self, focused: bool);
}
```

### 3.1 Editor Component (`components/editor.rs`)

A multi-line text editor with rich editing support.

**State model:**
```rust
struct EditorState {
    lines: Vec<String>,
    cursor_line: usize,
    cursor_col: usize,
}
```

**Key features:**
- **Word-wrap layout**: `word_wrap_line()` splits logical lines into visual chunks at word boundaries, handling paste markers as atomic units.
- **Vertical scrolling**: Editor height is `max(5, rows * 0.3)`. Scroll offset adjusts so the cursor line is always visible.
- **Paste markers**: Large pastes (>10 lines or >1000 chars) are collapsed to `[paste #N +X lines]` or `[paste #N X chars]`. Markers are treated as single grapheme clusters for navigation/deletion.
- **Autocomplete integration**: Slash commands (`/`), `@` file references. Uses `SelectList` inline below the editor border.
- **Prompt history**: Up/Down arrows cycle through history when editor is empty or on first/last visual line. History is capped at 100 entries, deduplicated.
- **Undo**: Fish-style coalescing (consecutive word chars group; whitespace captures state before itself).
- **Kill ring**: Emacs yank/yank-pop/delete-to-line-start/delete-word.
- **Sticky column**: `preferred_visual_col` is preserved when moving cursor vertically through wrapped lines. Complex decision table ensures natural up/down behavior across rewrapped lines.
- **Scroll indicators**: Top/bottom borders show `─── ↑ N more ─` / `─── ↓ N more ─` when scrolled.
- **IME hardware cursor**: Emits `CURSOR_MARKER` when focused so `TUI` can position the real cursor.
- **Cursor rendering**: Inverse-video fake cursor (`ESC[7m…ESC[0m`) replaces the grapheme at cursor; a space is shown when cursor is at end-of-line.

**Important layout function:**
```rust
fn layout_text(&self, content_width: u16) -> Vec<LayoutLine>
```
Maps logical lines + cursor position into visual wrapped lines.

### 3.2 Markdown Renderer (`components/markdown.rs`)

Uses `marked.lexer()` in TS; in Rust use `pulldown-cmark`.

**Rendering pipeline:**
1. Parse markdown into tokens.
2. `render_token()` dispatches on token type → styled ANSI strings.
3. `render_inline_tokens()` applies inline styles (bold, italic, code, links, strikethrough).
4. `wrap_text_with_ansi()` wraps the styled output to `content_width`.
5. Add horizontal padding, then optionally apply full-width background color.
6. Add vertical padding (empty lines with background).

**Supported elements:**
- Headings (depth 1–6), with extra underline/bold for H1
- Paragraphs
- Code blocks with optional syntax highlighting (theme provides `highlight_code`)
- Unordered/ordered lists (nested)
- Blockquotes (left `│ ` border, italic styling)
- Horizontal rules
- Inline: bold, italic, codespan, links (OSC 8 hyperlinks not used here — just styled ANSI), strikethrough
- **Tables**: width-aware cell wrapping, auto-calculated column widths, box-drawing borders (`┌─┬─┐` style). Falls back to raw markdown if terminal too narrow.

**Style prefix tracking:** After every inline style ANSI reset (`ESC[0m`), the renderer re-applies the current paragraph/heading style prefix so formatting does not drop back to default text style unexpectedly.

### 3.3 Image Display (`components/image.rs` + `terminal-image.rs`)

**Capability detection** (`detect_capabilities`) checks env vars:
- `KITTY_WINDOW_ID` → Kitty protocol
- `GHOSTTY_RESOURCES_DIR` / `TERM_PROGRAM=ghostty` → Kitty protocol
- `WEZTERM_PANE` → Kitty protocol
- `ITERM_SESSION_ID` → iTerm2 protocol

**Cell size query:**
- TUI sends `CSI 16 t` on start.
- Terminal responds with `CSI 6 ; height_px ; width_px t`.
- Response is consumed in `TUI::consume_cell_size_response()`; cell dimensions are cached and all components are invalidated.

**Image component render:**
```rust
fn render(&self, width: u16) -> Vec<String> {
    if supports_images {
        let rows = calculate_image_rows(dimensions, max_width_cells, cell_dims);
        let sequence = match protocol {
            Kitty => encode_kitty(base64, columns, rows, image_id),
            ITerm2 => encode_iterm2(base64, width, "auto", preserve_aspect),
        };
        // Return (rows-1) empty lines + last line with cursor-up prefix + sequence
    } else {
        vec![fallback_text]
    }
}
```

`is_image_line()` is used by the renderer to skip ANSI resets and width checks on lines containing image sequences.

**Important:** Multi-row Kitty images output cursor-up escape sequences on the last line so the terminal draws the image at the correct starting row.

---

## 4. Session Branching Visualization

The TUI package itself **does not contain tree/branch rendering logic**. The TypeScript `pi-tui` is a generic terminal UI library. The session tree visualization is implemented in the *consumer* package (`pi-mono/packages/agent` or similar).

However, the TUI primitives used for branching visualization are:
- `Markdown` for message content
- `Box` for grouping messages
- `Text` / `TruncatedText` for labels and summaries
- `SelectList` or custom list components for branch switchers
- `Image` for screenshot attachments

**For the Rust rewrite:** The branching tree UI should be built *on top of* `pi-tui` in the agent crate, not inside `pi-tui`. The TUI crate should remain a general-purpose library.

---

## 5. Real-Time Streaming Display

The streaming display is handled by the consumer (agent), but the TUI supports it via:

1. **Incremental `set_text()` on `Markdown` or `Text` components** — the component invalidates its cache and on next render only the new/changed lines are recomputed.
2. **Differential rendering** ensures that appending a few lines at the bottom of the output does not cause a full redraw. The fast path detects appended lines and renders only from the previous end onward.
3. **No full-screen clear on append** — the `appended_lines` fast path moves the cursor to the previous last line and emits `"\r\n"` + new content.
4. **Throttling** (`MIN_RENDER_INTERVAL_MS = 16`) prevents burning CPU on high-frequency streaming updates.

**Pattern used by consumer:**
- Hold a `Markdown` component.
- Append streaming token to an internal buffer.
- Call `markdown.set_text(buffer)`.
- Call `tui.request_render()`.

---

## 6. Input Handling and Keybindings

### 6.1 Stdin Buffer (`stdin-buffer.rs`)

`StdinBuffer` accumulates stdin data, extracts complete escape sequences, and emits either:
- `data` event — one complete key sequence
- `paste` event — content between bracketed paste markers `ESC[200~` … `ESC[201~`

It handles:
- CSI, OSC, DCS, APC sequences
- Old-style mouse sequences
- SS3 sequences
- Timeout flush (default 10 ms) for incomplete sequences

### 6.2 Key Parser (`keys.rs`)

Supports three input layers:

1. **Kitty keyboard protocol** (`CSI u` sequences)
   - Flags 1 (disambiguate), 2 (event types: press/repeat/release), 4 (alternate keys/base layout)
   - `decode_kitty_printable()` extracts plain text characters sent as CSI-u so they are not misinterpreted as control sequences.

2. **xterm modifyOtherKeys**
   - `CSI 27 ; mod ; keycode ~`

3. **Legacy escape sequences**
   - Arrow keys, function keys, shifted/ctrl variants, alt prefixes (`ESC + char`)

Special handling:
- Caps Lock / Num Lock are masked out (`LOCK_MASK = 64 + 128`).
- Base layout key is used as fallback for non-Latin keyboard layouts *only* when the codepoint is not already a known Latin letter or symbol (prevents Dvorak/Colemak false matches).
- `is_key_release()` / `is_key_repeat()` do a fast substring check (`:3u`, `:2u`, etc.) but skip bracketed-paste content.

### 6.3 Keybindings Manager (`keybindings.rs`)

- Registry of `Keybinding` string IDs (e.g., `"tui.editor.cursorUp"`).
- Each binding maps to one or more `KeyId`s.
- User overrides supported; conflicts are tracked.
- Default bindings include Emacs-style navigation (`ctrl+a`, `ctrl+e`, `ctrl+k`, `ctrl+u`, `alt+f`, `alt+b`, etc.) and standard arrows/enter/tab.

### 6.4 Focus Management

- `TUI::set_focus(component)` sets the focused component.
- Focused `Focusable` components receive `handle_input()` calls.
- Key release events are filtered out unless `wants_key_release()` returns true.
- Overlays can capture focus. The topmost visible non-`nonCapturing` overlay receives input if it has focus.
- If a focused overlay becomes invisible (e.g., via resize or `visible()` callback), focus automatically redirects to the next visible overlay or the pre-focus component.

---

## 7. Integration with Agent Runtime

The TUI library is **agnostic** to the agent runtime. Integration happens through these patterns in the consuming crate:

1. **Event loop** — The agent spawns a background task that reads agent events and updates shared state.
2. **UI thread** — The main thread runs `tui.start()`, which blocks on stdin input, schedules renders on resize, and routes keys to the focused component.
3. **Cross-thread updates** — Agent events append text to `Markdown` components, add/remove `Image` components, or update `Loader` status. After mutation, `tui.request_render()` is called (it is thread-safe in spirit; in Rust this will need a channel or `Arc<Mutex<TUI>>` + `mio`/`crossterm` event loop integration).

**Important TUI method for agents:**
```rust
pub fn request_render(&mut self, force: bool);
```

If `force = true`, it discards the previous frame buffer and triggers an immediate full redraw on the next tick. This is useful after large external state changes.

---

## 8. Testing Strategy

### 8.1 Snapshot Testing with Virtual Terminal

The TypeScript tests use `@xterm/headless` (xterm.js) as a virtual terminal:
- `VirtualTerminal` implements the `Terminal` interface.
- After writes, `flush()` ensures the xterm.js internal buffer is updated.
- `get_viewport()` and `get_scroll_buffer()` return actual terminal buffer lines for assertions.
- This validates not just that the Rust code *thinks* it output the right ANSI sequences, but that a real terminal emulator *interprets* them correctly.

**Rust equivalent options:**
- Keep a `Node`/`xterm.js` virtual terminal via `napi-rs` or subprocess for high-fidelity snapshot tests.
- Or use `vte` crate to simulate a terminal parser in pure Rust (lighter, faster, but less canonical than xterm.js).

### 8.2 Unit Tests

- `editor.test.ts` — history, undo, word wrapping, paste markers, autocomplete behavior.
- `keys.test.ts` — every `matchesKey` permutation for Kitty, modifyOtherKeys, legacy sequences.
- `stdin-buffer.test.ts` — partial sequence accumulation, paste extraction.
- `markdown.test.ts` — token rendering, table wrapping, inline style restoration.
- `truncate-to-width.test.ts`, `wrap-ansi.test.ts` — utility edge cases.
- `tui-render.test.ts` — resize handling, differential vs full redraw, overlay compositing, style leaks.

### 8.3 Regression Tests

Notable regressions already covered in TS that must be preserved:
- `regression-regional-indicator-width` — regional indicator symbols must have width 2.
- `tui-overlay-style-leak` — overlay borders must not leak background/underline into surrounding content.
- `bug-regression-isimageline-startswith-bug` — `isImageLine` must detect sequences anywhere in the line, not just at start.

---

## 9. Engineering Patterns Worth Preserving in Rust

### 9.1 Preserve the Differential Renderer

Do **not** replace `TUI::do_render()` with ratatui’s built-in `Terminal::draw()`. Ratatui redraws the entire internal buffer every frame. Pi-tui’s differential engine is its primary performance advantage for streaming output.

**Recommended architecture:**
- Use `ratatui` or raw `crossterm` for *input* and *cursor* APIs.
- Keep the custom `TUI` struct that computes line diffs and writes ANSI directly via `crossterm::queue!` + `std::io::Write`.
- Use ratatui widgets *only* if you can adapt them to return `Vec<String>` lines (they can; `ratatui::widgets::Widget::render` draws to a `Buffer`, but you can also ignore ratatui widgets and port the TS components directly).

### 9.2 Component Cache Invalidation

Every component caches its rendered output keyed by `(text, width)` or `(child_lines, width)`. The cache is invalidated explicitly via `invalidate()`. In Rust this is naturally:

```rust
struct Markdown {
    text: String,
    cached_text: Option<String>,
    cached_width: Option<u16>,
    cached_lines: Option<Vec<String>>,
    // ...
}
```

### 9.3 ANSI-Aware Text Utilities

Port `wrap_text_with_ansi`, `truncate_to_width`, `slice_by_column`, `extract_segments`, and `visible_width` **exactly** as specified. They are the foundation of correct layout. In Rust:
- Use `unicode_segmentation::UnicodeSegmentation::graphemes` instead of `Intl.Segmenter`.
- Use `unicode_width::UnicodeWidthChar` instead of `get-east-asian-width`.
- Keep the `AnsiCodeTracker` to preserve SGR state across line breaks.

### 9.4 Kitty Keyboard Protocol

Crossterm recently added Kitty keyboard protocol support, but if it is not sufficient, the TS `keys.rs` logic can be ported almost verbatim — it is pure string matching.

### 9.5 Terminal Capability Detection

Translate `detectCapabilities()` directly from environment-variable inspection. No dynamic capability querying is done beyond cell-size (`CSI 16 t`).

---

## 10. Gaps vs Current pi-rust Placeholder

### Current State (`/Users/ruipu/projects/Rusting/pi-rust/crates/pi-tui/`)

- `Cargo.toml` — empty dependency list.
- `src/lib.rs` — placeholder `add(a, b)` function with a trivial test.

### Missing (complete rewrite needed)

The Rust crate currently contains **zero** of the actual TUI functionality. The following must be implemented from scratch:

1. **Terminal abstraction** (`Terminal` trait + `ProcessTerminal` impl).
2. **Stdin buffering / key parsing** (`StdinBuffer`, `keys.rs`).
3. **ANSI text utilities** (`utils.rs` — width, wrap, truncate, slice, extract).
4. **Differential renderer** (`TUI` struct with `do_render()` logic).
5. **All components:** `Editor`, `Input`, `Markdown`, `Image`, `Box`, `Text`, `SelectList`, `Loader`, etc.
6. **Keybindings manager** (`KeybindingsManager`, `TUI_KEYBINDINGS`).
7. **Autocomplete provider** (`CombinedAutocompleteProvider`, `fd` integration).
8. **Terminal image support** (`terminal-image.rs` — capability detection, Kitty/iTerm2 encoding, dimension parsers).
9. **Virtual terminal for testing** (xterm.js integration or `vte`-based replacement).
10. **Comprehensive test suite** porting all `.test.ts` files.

### Suggested Dependency Additions to `Cargo.toml`

```toml
[dependencies]
crossterm = "0.28"
ratatui = { version = "0.29", optional = true }
pulldown-cmark = "0.12"
unicode-segmentation = "1.12"
unicode-width = "0.2"
regex = "1"
# For image dimension parsing (or write small custom parsers to avoid full image crate)
image = { version = "0.25", default-features = false, features = ["png", "jpeg", "gif", "webp"] }
# Optional: tokio/async runtime if the agent integration is async
# tokio = { version = "1", features = ["full"] }

[dev-dependencies]
# If keeping xterm.js tests via Node bridge:
# napi = "2"
# Or use vte in Rust:
vte = "0.15"
```

---

## 11. Notable Surprises for the Rewriter

1. **Differential rendering is hand-rolled and essential.** Do not assume ratatui handles this. Streaming chat output would flicker or consume excessive CPU without it.
2. **Kitty image rendering uses empty lines + cursor-up.** The `Image` component returns `(rows - 1)` empty strings and one final line containing `ESC[<rows-1>A<kitty_sequence>`. The TUI treats these empty lines literally and just clears them; the terminal composes the actual image.
3. **Paste markers are grapheme-aware.** `segmentWithMarkers()` hooks into `Intl.Segmenter` so that `[paste #1 +123 lines]` behaves as a single atomic unit during cursor movement and deletion. Any Rust grapheme iterator must support this merging logic.
4. **Sticky column logic is elaborate.** The `computeVerticalMoveColumn()` decision table has 7 cases. Getting up/down arrow behavior right across wrapped lines requires exact porting of this logic.
5. **Termux height-change exception is real.** Without it, every keyboard show/hide on Android replays the entire scrollback.
6. **IME hardware cursor positioning uses `CURSOR_MARKER`.** This is an APC sequence (`ESC _pi:c BEL`) that is stripped by `extractCursorPosition()` before output. The real cursor is then moved to that (row, col) with relative ANSI sequences.
7. **The Markdown table renderer is custom and complex.** It does width-aware column distribution, word wrapping inside cells, and box-drawing borders. Do not attempt to simply print `pulldown-cmark` HTML output; you need to intercept table events and render them manually.

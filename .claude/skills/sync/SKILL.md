---
name: pi-rust-doc-updater
description: >
  Update pi-rust project progress tracking and usage documentation after a crate
  or feature is completed. Use when new code has been merged into crates/ or src/,
  and the README progress checkboxes, Roadmap, or usage docs (e.g. docs/pi-ai-usage.md)
  may be out of sync. Also use when a new provider, example, or public API has been
  added and needs user-facing documentation.
---

# pi-rust Doc Updater

This skill guides updating project records and usage docs for the pi-rust workspace.

## Goals
1. Keep README progress sections accurate after feature/package completion.
2. Create or update usage docs in `docs/` when new public APIs or examples land.
3. Ensure all docs align with the original [pi-mono](https://github.com/badlogic/pi-mono) design and engineering patterns.

## Workflow

### Step 1: Detect what changed
- Read `README.md` progress and roadmap sections.
- Check `git status` or `git diff HEAD~1` to see which crates/files were recently modified.
- Identify newly completed items (new providers, examples, core types, tests, etc.).

### Step 2: Update README progress
- Flip checkboxes from `[ ]` to `[x]` for items that are now implemented.
- Add new checklist items if a feature was completed but not previously listed.
- Keep the wording aligned with the original pi-mono vocabulary and design.

### Step 3: Update or create usage docs
- If a new crate or provider shipped, ensure there is a corresponding `docs/<crate>-usage.md`.
- Usage docs should include:
  - A short description of what the crate/provider does.
  - A minimal runnable Rust example (or reference to `crates/<crate>/examples/`).
  - Key types and how to construct them.
- Example pattern: see `docs/pi-ai-usage.md` for the `OpenAICompatibleProvider`.
- When updating existing usage docs, append or modify sections rather than rewriting the whole doc unless requested.

### Step 4: Align with original repo
- Reference `/tmp/pi-mono/` and relevant `docs/*-spec.md` files.
- Ensure API naming, examples, and concepts match the TypeScript original where applicable.
- If the Rust implementation intentionally diverges, note the difference briefly.

### Step 5: Finish
- Present a brief summary of what changed and why.
- Do not rewrite code for the user; update docs only.

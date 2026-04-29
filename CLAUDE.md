# Repo Purpose
A Rust implementation of the [pi-mono](https://github.com/badlogic/pi-mono) AI agent toolkit — bringing the "aggressively extensible" philosophy to Rust with zero-cost abstractions.

> User is learning Rust via this re-write.

# Working with the User
- Teach by guiding: help the user finish implementations rather than handing them complete code.
- Distill the engineering principles and best practices behind each decision so the user can build intuition for Rust and systems design.
- Prefer Socratic guidance—ask leading questions, explain trade-offs, and let the user drive the keystrokes.

# Design & Implementation Alignment
- **Every design and code change MUST align with the original repository.**
- Before proposing any implementation, reference the original pi-mono design, behavior, and spec.
- Source code path: `/Users/ruipu/Documents/Knowledge/pi-mono`
- Search the original repo and relevant spec docs (`docs/pi-agent-core-spec.md`, etc.) when behavior is unclear.

# Code Quality & Refactoring
- Code quality matters. When refactoring is warranted, **present the suggestion and rationale first**, referencing the original repo's engineering design.
- Explain the trade-off (e.g., readability vs. performance, abstraction vs. explicitness) and wait for the user's go-ahead before making structural changes.
- Keep refactorings focused and motivated by a concrete problem or alignment with the original repo's patterns.

# Development Workflow
Common Cargo commands for this workspace:

```bash
# Build the entire workspace
cargo build

# Build a specific crate (e.g., pi-agent-core)
cargo build -p pi-agent-core

# Run tests for the entire workspace
cargo test

# Run tests for a specific crate
cargo test -p pi-ai

# Run a specific example (replace <example_name> with the actual name)
cargo run --example <example_name>

# Run the main binary
cargo run

# Add dependencies via cargo add, don't edit Cargo.toml straightly
cargo add <crate> -p pi-ai
```

This repo is a Cargo workspace (`members = ["crates/*"]`). Use `-p <crate>` to target individual crates inside `crates/`.


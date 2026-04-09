# pi-rust

A Rust implementation of the [pi-mono](https://github.com/badlogic/pi-mono) AI agent toolkit — bringing the "aggressively extensible" philosophy to Rust with zero-cost abstractions.

> **Original**: [badlogic/pi-mono](https://github.com/badlogic/pi-mono) by Mario Zechner  
> **Philosophy**: Minimal core, maximum extensibility. Build your AI agent, your way.

## Overview

pi-rust is a Rust port of the pi-mono monorepo, providing modular components for building AI-powered agents. While the original is TypeScript-based, this implementation leverages Rust's performance and type safety for production-grade agent systems.

## Architecture

```
pi-rust/
├── crates/
│   ├── pi-agent-core    # Agent runtime (placeholder)
│   ├── pi-ai            # Unified multi-provider LLM API
│   └── pi-tui           # Terminal UI library (placeholder)
├── docs/
│   └── pi-ai-usage.md   # Usage examples
└── src/
    └── main.rs          # Application entry point
```

### Crates

| Crate | Description | Original TS Equivalent |
|-------|-------------|----------------------|
| `pi-ai` | Unified LLM client with multi-provider support | `@mariozechner/pi-ai` |
| `pi-agent-core` | Stateful agent runtime, event streaming, tool execution | `@mariozechner/pi-agent-core` |
| `pi-tui` | Terminal UI components (editor, markdown, image display) | `@mariozechner/pi-tui` |

## Progress

### pi-ai
- [x] `LLMProvider` trait for pluggable providers
- [x] Core types: `Message`, `Role`, `GenerateRequest`, `GenerateResponse`, `GenerateOptions`, `Usage`
- [x] `MockProvider` for unit testing
- [x] `OpenAICompatibleProvider` for OpenAI-compatible endpoints
- [x] Example usage of `OpenAICompatibleProvider`
- [ ] Streaming responses
- [ ] Tool calling
- [ ] Automatic model discovery and capability detection
- [ ] Cross-provider handoffs mid-conversation
- [ ] OAuth support for subscription-based services

### pi-agent-core
- [ ] Event-driven agent lifecycle
- [ ] Tool execution with validation and error handling
- [ ] State management and message history
- [ ] Steering (interrupt mid-execution) and follow-up queues

### pi-tui
- [ ] Differential rendering for flicker-free output
- [ ] Terminal components: editor, markdown renderer, image display
- [ ] Session branching visualization
- [ ] Real-time streaming display

## Philosophy

1. **Aggressively Extensible**: Core is minimal, everything else is built on top
2. **Your Agent, Your Way**: Don't dictate workflow, provide building blocks
3. **Type Safety**: Leverage Rust's type system for reliable agent systems
4. **Performance**: Zero-cost abstractions for high-throughput scenarios

## Quick Start

### Prerequisites

- Rust 1.85+ (2024 edition)
- Cargo

### Build

```bash
cargo build --release
```

### Run Tests

```bash
cargo test --workspace
```

### Run the OpenAI-Compatible Example

```bash
cd crates/pi-ai
API_KEY=your_api_key BASE_URL=https://api.openai.com MODEL=gpt-4o cargo run --example openai_compatible_demo
```

## Development

### Workspace Structure

This project uses Cargo workspaces. Each crate in `crates/` is independently versioned.

```bash
# Build specific crate
cargo build -p pi-ai

# Test specific crate
cargo test -p pi-ai
```

### Roadmap

- [x] Core LLM client trait and OpenAI-compatible provider (`pi-ai`)
- [ ] Agent runtime with tool system (`pi-agent-core`)
- [ ] Terminal UI framework (`pi-tui`)
- [ ] Coding agent CLI (combining all crates)
- [ ] Web UI components (future)
- [ ] Extension/plugin system (future)

## Comparison with Original

| Feature | pi-mono (TS) | pi-rust |
|---------|--------------|---------|
| Language | TypeScript | Rust |
| Runtime | Node.js | Native |
| Memory Safety | GC | Compile-time guarantees |
| Concurrency | Async/await | Tokio + fearless concurrency |
| Extensions | npm packages | Cargo crates / dynamic libs |
| Philosophy | Aggressively extensible | Same |

## License

MIT OR Apache-2.0

## Acknowledgments

- Original [pi-mono](https://github.com/badlogic/pi-mono) by [Mario Zechner](https://github.com/badlogic)
- Inspired by the "aggressively extensible" philosophy

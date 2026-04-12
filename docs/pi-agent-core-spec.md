# pi-agent-core Specification (Rust Rewrite)

This document describes the TypeScript implementation of `@mariozechner/pi-agent-core` (`/tmp/pi-mono/packages/agent`) and specifies what a Rust rewrite in `pi-rust/crates/pi-agent-core` must preserve.

---

## 1. High-Level Architecture and Module Layout

### Source Files (TypeScript)

| File | Role |
|------|------|
| `src/index.ts` | Public API exports: `Agent`, `agentLoop`, `agentLoopContinue`, `streamProxy`, and all types. |
| `src/agent.ts` | The `Agent` class — stateful wrapper that owns the transcript, queues, event subscribers, and run lifecycle. |
| `src/agent-loop.ts` | Low-level `runAgentLoop` and `runAgentLoopContinue` functions. Implements the core turn loop, streaming, and tool execution. Also exports stream-based wrappers `agentLoop` and `agentLoopContinue`. |
| `src/proxy.ts` | `streamProxy` — a drop-in `StreamFn` that proxies SSE requests through a backend server, reconstructing `AssistantMessageEvent`s client-side. |
| `src/types.ts` | All domain types: `AgentState`, `AgentEvent`, `AgentLoopConfig`, `AgentTool`, `AgentMessage`, etc. |

### Suggested Rust Module Layout

```
crates/pi-agent-core/src/
  lib.rs           # Public re-exports
  agent.rs         # Agent struct and lifecycle
  loop.rs          # Core run loop (run_agent_loop, run_agent_loop_continue)
  events.rs        # AgentEvent enum, EventStream equivalent
  tools.rs         # AgentTool trait, execution, validation wrappers
  state.rs         # AgentState, AgentContext, MutableAgentState
  queues.rs        # PendingMessageQueue and QueueMode
  proxy.rs         # stream_proxy implementation
```

---

## 2. Agent Lifecycle

### 2.1 Creation

The `Agent` is constructed with `AgentOptions`:

```typescript
class Agent {
  constructor(options: AgentOptions = {})
}
```

Key options:
- `initialState` — partial `AgentState` (no runtime fields like `pendingToolCalls` or `isStreaming`)
- `convertToLlm` — **required** bridge from `AgentMessage[]` to LLM `Message[]`
- `transformContext` — optional async transform on `AgentMessage[]` before conversion
- `streamFn` — defaults to `streamSimple` from `pi-ai`
- `getApiKey` — dynamic key resolver (e.g. for OAuth refresh)
- `beforeToolCall` / `afterToolCall` — hooks around tool execution
- `steeringMode` / `followUpMode` — queue drain semantics (`"all"` or `"one-at-a-time"`)
- `sessionId`, `thinkingBudgets`, `transport`, `maxRetryDelayMs`, `toolExecution`

### 2.2 Run Loop

A "run" is a single invocation of the LLM loop. There are three entry points:

1. **`prompt(input, images?)`** — starts a new run from user input. Throws if a run is already active.
2. **`continue()``** — resumes from existing transcript without injecting a new user message. Throws if:
   - No messages exist
   - Last message is `assistant` (unless steering/follow-up queues are drained)
   - A run is already active
3. **`steer(message)` / `followUp(message)`** — queue messages for injection mid-run or post-run.

### 2.3 Pause, Resume, Stop, Teardown

- **`agent.abort()`** — aborts the active run via `AbortController`. The `streamFn` receives the abort `signal` and is expected to encode the abort as an `error` stream event. Safe to call when idle (no-op).
- **`agent.waitForIdle()`** — returns a promise that resolves after the active run and all awaited event listeners settle.
- **`agent.reset()`** — clears `messages`, runtime state (`isStreaming`, `streamingMessage`, `pendingToolCalls`, `errorMessage`), and both queues. Does not reset static config.
- Runs are mutually exclusive; `prompt()` and `continue()` throw while `isStreaming` is true.

---

## 3. Event System

### 3.1 Event Stream / Observable

Two layers exist:

1. **Low-level `agentLoop` / `agentLoopContinue`** return an `EventStream<AgentEvent, AgentMessage[]>`.
   - `EventStream` implements `AsyncIterable<T>`.
   - Events are pushed into an internal queue; waiting consumers resolve via pending promises.
   - Completion is detected when an `agent_end` event is pushed, or when `end(result)` is called.
   - The stream result is retrievable via `stream.result(): Promise<AgentMessage[]>`.

2. **High-level `Agent.subscribe(listener)`** registers listeners that are `await`ed in registration order for every emitted event.
   - Listeners receive `(event: AgentEvent, signal: AbortSignal)`.
   - `agent_end` listeners are still part of run settlement; `waitForIdle()` does not resolve until they finish.
   - Listeners act as a barrier: in the `Agent` class, tool execution does not start until `message_end` for the assistant message has been fully processed by all listeners.

### 3.2 Event Types

```typescript
type AgentEvent =
  // Lifecycle
  | { type: "agent_start" }
  | { type: "agent_end"; messages: AgentMessage[] }
  // Turn lifecycle
  | { type: "turn_start" }
  | { type: "turn_end"; message: AgentMessage; toolResults: ToolResultMessage[] }
  // Message lifecycle
  | { type: "message_start"; message: AgentMessage }
  | { type: "message_update"; message: AgentMessage; assistantMessageEvent: AssistantMessageEvent }
  | { type: "message_end"; message: AgentMessage }
  // Tool lifecycle
  | { type: "tool_execution_start"; toolCallId: string; toolName: string; args: any }
  | { type: "tool_execution_update"; toolCallId: string; toolName: string; args: any; partialResult: any }
  | { type: "tool_execution_end"; toolCallId: string; toolName: string; result: any; isError: boolean };
```

**Semantics:**
- `message_start` / `message_end` are emitted for user messages, assistant messages, and tool result messages.
- `message_update` is **only** emitted for assistant messages during streaming.
- `turn_end` carries the assistant message for that turn and all `ToolResultMessage`s produced in the turn.
- `agent_end` carries all messages produced during the run.

### 3.3 Event Ordering Guarantees

For `prompt("hello")` with no tools:
```
agent_start
  turn_start
    message_start (user)
    message_end   (user)
    message_start (assistant)
    message_update... (0+ times)
    message_end   (assistant)
  turn_end
agent_end
```

With tools (parallel mode):
```
agent_start
  turn_start
    message_start/end (user)
    message_start/end (assistant with toolCalls)
    tool_execution_start (id=1)
    tool_execution_start (id=2)
    tool_execution_end   (id=1)   -- in source order
    message_start/end (toolResult 1)
    tool_execution_end   (id=2)
    message_start/end (toolResult 2)
  turn_end
  turn_start
    message_start/end (assistant follow-up)
  turn_end
agent_end
```

---

## 4. Tool System

### 4.1 Tool Definition Schema

```typescript
interface AgentTool<TParameters extends TSchema = TSchema, TDetails = any> extends Tool<TParameters> {
  label: string;
  prepareArguments?: (args: unknown) => Static<TParameters>;
  execute: (
    toolCallId: string,
    params: Static<TParameters>,
    signal?: AbortSignal,
    onUpdate?: AgentToolUpdateCallback<TDetails>,
  ) => Promise<AgentToolResult<TDetails>>;
}
```

- `label` is for UI display.
- `prepareArguments` is a compatibility shim run **before** schema validation.
- `execute` receives the validated params. It should **throw on failure**, not return error content.
- `onUpdate` allows tools to stream partial progress (e.g. file search progress bars).

### 4.2 Execution Flow

For each assistant message containing tool calls:

1. **Emit** `tool_execution_start` for every tool call (in source order).
2. **Preflight** each tool call:
   - Look up tool by name; if missing, immediate error outcome.
   - Call `prepareArguments` if present.
   - Validate arguments via `validateToolArguments` (AJV + TypeBox; mutates args in-place for coercion).
   - Call `config.beforeToolCall`. If it returns `{ block: true }`, emit immediate error outcome.
3. **Execute** based on `toolExecution` mode:
   - `"sequential"`: execute one by one.
   - `"parallel"` (default): preflight all sequentially, then run allowed tools concurrently; emit final results in **assistant source order**.
4. **Finalize** each executed call:
   - Call `config.afterToolCall` to allow overriding `content`, `details`, or `isError`.
   - Emit `tool_execution_end`.
   - Emit `message_start` / `message_end` for the `ToolResultMessage`.

### 4.3 Validation

Validation in the TS implementation is performed by `validateToolArguments` from `@mariozechner/pi-ai`:
- Uses AJV with `allErrors: true`, `strict: false`, `coerceTypes: true`.
- In browser-extension/CSP environments where `new Function()` is forbidden, validation is skipped and raw arguments are returned.
- Arguments are `structuredClone`d before validation so AJV can mutate them safely.
- Validation errors are formatted as a multi-line string with paths and messages.

### 4.4 Error Handling

- Missing tool → immediate error result with `isError: true`.
- Validation failure → immediate error result.
- `beforeToolCall` blocks → immediate error result.
- `execute` throws → error result containing `error.message`.
- All errors produce a `ToolResultMessage` with `isError: true` and text content explaining the failure.

### 4.5 Timeouts

There is **no built-in tool timeout** in `pi-agent-core`. Timeouts are expected to be handled by:
- The tool implementation itself.
- The `AbortSignal` passed to `execute`, which is triggered by `agent.abort()`.

---

## 5. State Management and Message History

### 5.1 AgentState

```typescript
interface AgentState {
  systemPrompt: string;
  model: Model<any>;
  thinkingLevel: ThinkingLevel;
  set tools(tools: AgentTool<any>[]);
  get tools(): AgentTool<any>[];
  set messages(messages: AgentMessage[]);
  get messages(): AgentMessage[];
  readonly isStreaming: boolean;
  readonly streamingMessage?: AgentMessage;
  readonly pendingToolCalls: ReadonlySet<string>;
  readonly errorMessage?: string;
}
```

- `tools` and `messages` use accessors so that assignment copies the top-level array.
- Direct mutation of the returned array is allowed (e.g. `agent.state.messages.push(msg)`).
- Runtime fields (`isStreaming`, `streamingMessage`, `pendingToolCalls`, `errorMessage`) are readonly from the public interface but mutable internally.

### 5.2 Conversation State Maintenance

- During a run, `agent-loop.ts` maintains a mutable `currentContext: AgentContext` that holds the full transcript.
- The loop appends messages to `currentContext.messages` as they arrive/stream.
- The `Agent` class mirrors this into its own `_state.messages` via `processEvents`.
- `streamingMessage` is updated on every `message_start` / `message_update` and cleared on `message_end` / `agent_end`.
- `pendingToolCalls` is a `Set<string>` of `toolCallId`s updated on `tool_execution_start` / `tool_execution_end`.
- `errorMessage` is populated from the assistant message's `errorMessage` field on `turn_end` (for errors/aborts).

### 5.3 Persistence and Resume

There is **no built-in persistence** in `pi-agent-core`. Persistence is the caller's responsibility:
- Save `agent.state.messages`, `systemPrompt`, `model`, and `tools` externally.
- To resume, reconstruct an `Agent` with `initialState` containing the saved values.

---

## 6. Steering / Interrupt Mechanism

### 6.1 Queues

Two queues exist:
- **Steering queue** — messages injected **after** the current assistant turn finishes (i.e. after all tool calls for that turn complete).
- **Follow-up queue** — messages injected **only when the agent would otherwise stop** (no more tool calls, no steering messages).

Both queues support a `QueueMode`:
- `"one-at-a-time"` (default): `drain()` returns at most one message.
- `"all"`: `drain()` returns all queued messages.

### 6.2 Loop Integration

Inside `runLoop`:

```typescript
// Outer loop: continues when follow-ups arrive
while (true) {
  let hasMoreToolCalls = true;

  // Inner loop: process tool calls and any steering that arrived
  while (hasMoreToolCalls || pendingMessages.length > 0) {
    // Inject pending messages before the next LLM call
    if (pendingMessages.length > 0) { ... }

    const message = await streamAssistantResponse(...);
    // ... handle tool calls ...

    pendingMessages = (await config.getSteeringMessages?.()) || [];
  }

  const followUps = (await config.getFollowUpMessages?.()) || [];
  if (followUps.length > 0) {
    pendingMessages = followUps;
    continue;
  }

  break;
}
```

**Key behaviors:**
- Steering messages are polled **after** `turn_end` (i.e. after all tool results are produced).
- They are appended to the context as new user/toolResult messages and trigger a new turn.
- Follow-ups are only checked when the inner loop is exhausted.
- `Agent.continue()` can also drain steering/follow-up queues when the last message is `assistant`.

---

## 7. Integration with LLM Layer (pi-ai)

### 7.1 Stream Function Contract

```typescript
type StreamFn = (
  ...args: Parameters<typeof streamSimple>
) => ReturnType<typeof streamSimple> | Promise<ReturnType<typeof streamSimple>>;
```

Contract:
- Must **not throw** for request/model/runtime failures.
- Must return an `AssistantMessageEventStream`.
- Failures must be encoded in the stream via protocol events and a final `AssistantMessage` with `stopReason: "error"` or `"aborted"` and `errorMessage`.

### 7.2 Per-Call Pipeline

For each assistant turn, the agent performs:

1. `transformContext(messages, signal?)` (optional) → `AgentMessage[]`
2. `convertToLlm(messages)` → `Message[]`
3. Build `Context = { systemPrompt, messages: Message[], tools?: Tool[] }`
4. Resolve API key: `getApiKey(provider)` falls back to `config.apiKey`
5. Call `streamFn(model, context, { ...config, apiKey: resolved, signal })`
6. Iterate `AssistantMessageEvent`s from the stream:
   - `start` → push partial message to context, emit `message_start`
   - `*_delta` / `*_start` / `*_end` → update context's last message, emit `message_update`
   - `done` / `error` → resolve final message via `response.result()`, update context, emit `message_end`

### 7.3 Retries / Backoff

There is **no retry logic inside pi-agent-core**. If the `streamFn` throws (violating contract), the `Agent` class catches it in `handleRunFailure`, synthesizes an error assistant message, appends it to state, and emits `agent_end`.

The `maxRetryDelayMs` option is forwarded to the `streamFn`/provider layer; `pi-ai` providers may use it to cap server-requested retry delays.

---

## 8. Testing Strategy

### 8.1 Unit Tests

`test/agent.test.ts` — tests the `Agent` class in isolation using a `MockAssistantStream`:
- Construction with default/custom state
- Subscribe / unsubscribe
- Async subscriber settlement (`waitForIdle`)
- Abort signal propagation
- Queueing (`steer`, `followUp`)
- Error cases (prompt while streaming, continue validation)
- Forwarding of session IDs to `streamFn`

`test/agent-loop.test.ts` — tests `agentLoop` / `agentLoopContinue` directly:
- Event sequences and message accumulation
- `convertToLlm` and `transformContext` ordering
- Tool call execution (sequential and parallel)
- `beforeToolCall` / `afterToolCall` hooks
- `prepareArguments` integration
- Steering message injection timing
- Continue semantics

### 8.2 Integration / E2E Patterns

`test/e2e.test.ts` uses `registerFauxProvider` from `pi-ai` to create a deterministic mock LLM:
- Basic prompt
- Tool execution with real tool callbacks
- Abort during streaming
- Lifecycle event assertions
- Multi-turn conversation state retention
- Thinking block preservation
- `continue()` from user message and tool result

### 8.3 Mock Patterns Worth Preserving

```typescript
class MockAssistantStream extends EventStream<AssistantMessageEvent, AssistantMessage> {
  constructor() {
    super(
      (event) => event.type === "done" || event.type === "error",
      (event) => event.type === "done" ? event.message : event.error,
    );
  }
}
```

This pattern is central to testing: push events via `queueMicrotask`, then await the stream.

---

## 9. Engineering Patterns Worth Preserving

1. **Barriers between phases.** In the `Agent` class, `processEvents` awaits all subscribers before the loop proceeds. This ensures that by the time `beforeToolCall` runs, agent state already reflects the assistant `message_end`.

2. **Stream-first error handling.** The `StreamFn` contract encodes all failures into the event stream rather than throwing. This keeps the loop simple and event-driven.

3. **Separation of `AgentMessage` and `Message`.** The agent operates on a superset of LLM messages, allowing app-specific custom types. The bridge (`convertToLlm`) is explicit and required.

4. **Copy-on-assign for collections.** `tools` and `messages` accessors copy the top-level array on assignment, preventing external mutation from accidentally corrupting internal state.

5. **Parallel tool execution with ordered emission.** Tool calls are prepared sequentially (for validation and hooks), executed in parallel, but final results are emitted in assistant source order. This gives UIs deterministic sequencing.

6. **Abort signal propagation.** The same `AbortSignal` is passed to `streamFn`, `transformContext`, `beforeToolCall`, `afterToolCall`, and tool `execute`. Coordinated cancellation is critical.

7. **Immediate vs prepared tool outcomes.** The preflight phase returns either a `PreparedToolCall` or an `ImmediateToolCallOutcome`. This unifies error paths (missing tool, validation failure, blocked) before execution begins.

---

## 10. Gaps vs Current pi-rust Placeholder

### Current State (`crates/pi-agent-core`)

As of the snapshot:

```rust
// crates/pi-agent-core/src/lib.rs
pub fn add(left: u64, right: u64) -> u64 {
    left + right
}
```

```toml
# crates/pi-agent-core/Cargo.toml
[package]
name = "pi-agent-core"
version = "0.1.0"
edition = "2024"

[dependencies]
```

### What Is Missing

| Area | Missing in pi-rust |
|------|-------------------|
| **Public types** | No `AgentState`, `AgentEvent`, `AgentMessage`, `AgentTool`, `AgentLoopConfig`, `AgentContext`, `AgentOptions` equivalents. |
| **Event stream** | No `EventStream<T, R>` or async-iterable event channel. Need a Rust equivalent (e.g. a `tokio::sync::mpsc` + `Stream` wrapper, or a custom async iterator with a `.result()` awaitable). |
| **Agent struct** | No stateful `Agent` with `prompt`, `continue`, `steer`, `followUp`, `abort`, `waitForIdle`, `reset`, `subscribe`. |
| **Core loop** | No `run_agent_loop` / `run_agent_loop_continue` functions. |
| **Tool system** | No tool definition trait, execution orchestration, validation bridge, or `before`/`after` hooks. |
| **Queueing** | No `PendingMessageQueue` with `"all"` / `"one-at-a-time"` semantics. |
| **Proxy** | No `stream_proxy` equivalent. |
| **LLM integration** | No `StreamFn` abstraction or integration with the `pi-ai` Rust equivalent (`crates/pi-ai`). |
| **Tests** | Only a placeholder unit test for `add(2, 2)`. |

### Dependencies to Add

- Whatever async runtime the project uses (likely `tokio`).
- `futures` or `tokio-stream` for async iterator / stream utilities.
- A schema validation library to replace AJV + TypeBox (e.g. `schemars` + `jsonschema`, or a custom derive-based validator).
- `serde` / `serde_json` for message serialization.
- `thiserror` for error types.

### Suggested First Steps for Rust Implementation

1. Define the `AgentEvent` enum and a stream/result abstraction equivalent to `EventStream`.
2. Define `AgentMessage` as an enum extension over the `pi-ai` `Message` types.
3. Define the `AgentTool` trait and `AgentToolResult` struct.
4. Build `run_agent_loop` as an async function with `tokio` channels or a similar event sink.
5. Wrap the loop in an `Agent` struct with run lifecycle management and subscriber barriers.
6. Add `PendingMessageQueue` and wire it into the loop for steering/follow-up.
7. Port the test suite using mock streams and a faux provider equivalent.

---

*Document generated from analysis of `/tmp/pi-mono/packages/agent` for the Rust rewrite in `pi-rust`.*

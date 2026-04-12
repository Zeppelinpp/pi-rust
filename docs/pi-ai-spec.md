# pi-ai Specification (from pi-mono/ai)

This document is a comprehensive spec for rewriting the TypeScript `pi-mono/packages/ai` package in Rust. It is intended to be directly usable by a Rust developer.

---

## 1. High-Level Architecture and Module Layout

### Original (TypeScript) Source Layout

```
src/
  index.ts                    -- Public exports
  types.ts                    -- Core domain types (Message, Model, Usage, etc.)
  stream.ts                   -- High-level API: stream(), complete(), streamSimple(), completeSimple()
  api-registry.ts             -- Provider registry keyed by API slug
  models.ts                   -- Model registry helpers, calculateCost(), supportsXhigh()
  models.generated.ts         -- Auto-generated model catalog (massive const)
  env-api-keys.ts             -- Environment variable -> API key resolution
  cli.ts                      -- CLI entrypoint (not core lib)
  bedrock-provider.ts         -- Re-export stub for optional bedrock package
  oauth.ts                    -- Stub re-export
  providers/
    register-builtins.ts      -- Lazy loader that registers all built-in providers
    simple-options.ts         -- Budget/thinking helpers for simple mode
    transform-messages.ts     -- Cross-provider message normalization (tool call IDs, thinking blocks, orphaned synthetic tool results)
    openai-completions.ts     -- OpenAI Chat Completions provider (many compat providers use this API)
    openai-responses.ts       -- OpenAI Responses API provider
    openai-responses-shared.ts-- Shared message/tool conversion & stream processing for Responses APIs
    openai-codex-responses.ts -- Codex-specific Responses wrapper
    azure-openai-responses.ts -- Azure Responses wrapper
    anthropic.ts              -- Anthropic Messages API provider
    mistral.ts                -- Mistral conversations API provider
    google.ts                 -- Google Generative AI (Gemini) provider
    google-shared.ts          -- Shared Google message/tool conversion helpers
    google-gemini-cli.ts      -- Google Gemini CLI provider
    google-vertex.ts          -- Google Vertex AI provider
    amazon-bedrock.ts         -- AWS Bedrock ConverseStream provider
    faux.ts                   -- In-memory test provider
    github-copilot-headers.ts -- Dynamic header builder for Copilot
  utils/
    event-stream.ts           -- EventStream<T,R> generic async iterable, AssistantMessageEventStream
    json-parse.ts             -- parseStreamingJson (partial-json fallback)
    sanitize-unicode.ts       -- sanitizeSurrogates (remove unpaired surrogates)
    validation.ts             -- validateToolCall / validateToolArguments with AJV
    overflow.ts               -- isContextOverflow detector
    hash.ts                   -- shortHash helper
    oauth/                    -- OAuth helpers per provider
```

### Rust Target Layout (suggested)

```
crates/pi-ai/
  src/
    lib.rs                    -- Re-export public types
    types.rs                  -- Domain types aligned to TS types
    message.rs                -- Message, Role, content blocks
    stream.rs                 -- EventStream, AssistantMessageEventStream
    provider.rs               -- LLMProvider trait
    api_registry.rs           -- ApiProvider registry
    models.rs                 -- Model catalog & cost calc
    env_api_keys.rs           -- Key resolution
    providers/
      mod.rs
      mock.rs                 -- MockProvider (already stubbed)
      faux.rs                 -- FauxProvider for tests
      openai_compatible.rs    -- OpenAI Completions catch-all
      openai_responses.rs     -- OpenAI Responses API
      anthropic.rs            -- Anthropic Messages
      mistral.rs              -- Mistral
      google.rs               -- Google GenAI
      google_vertex.rs        -- Google Vertex
      google_gemini_cli.rs    -- Gemini CLI
      bedrock.rs              -- AWS Bedrock
      common/
        simple_options.rs     -- buildBaseOptions, adjustMaxTokensForThinking
        transform_messages.rs -- Cross-provider normalization
        overflow.rs           -- Context overflow detection
        json_parse.rs         -- Streaming JSON parser
        unicode.rs            -- sanitizeSurrogates
        validation.rs         -- Tool-call AJV equivalent
```

---

## 2. Core Types and Their Relationships

### 2.1 Content Blocks

```typescript
// Text produced by the model
interface TextContent {
  type: "text";
  text: string;
  textSignature?: string;       // Provider-specific replay signature (OpenAI Responses)
}

// Reasoning / thinking block
interface ThinkingContent {
  type: "thinking";
  thinking: string;
  thinkingSignature?: string;   // e.g. OpenAI reasoning item ID, Anthropic signature
  redacted?: boolean;           // True when the thought was encrypted/redacted
}

// Inline image (base64)
interface ImageContent {
  type: "image";
  data: string;                 // base64
  mimeType: string;             // e.g. "image/png"
}

// Tool invocation requested by the model
interface ToolCall {
  type: "toolCall";
  id: string;
  name: string;
  arguments: Record<string, any>;
  thoughtSignature?: string;    // Google-specific opaque signature
}
```

Rust equivalent suggestion:

```rust
#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text { text: String, text_signature: Option<String> },
    Thinking { thinking: String, thinking_signature: Option<String>, redacted: bool },
    Image { data: String, mime_type: String },
    ToolCall { id: String, name: String, arguments: serde_json::Value, thought_signature: Option<String> },
}
```

### 2.2 Messages

```typescript
type Message = UserMessage | AssistantMessage | ToolResultMessage;

interface UserMessage {
  role: "user";
  content: string | (TextContent | ImageContent)[];
  timestamp: number; // ms since epoch
}

interface AssistantMessage {
  role: "assistant";
  content: (TextContent | ThinkingContent | ToolCall)[];
  api: Api;
  provider: Provider;
  model: string;
  responseId?: string;
  usage: Usage;
  stopReason: StopReason;
  errorMessage?: string;
  timestamp: number;
}

interface ToolResultMessage<TDetails = any> {
  role: "toolResult";
  toolCallId: string;
  toolName: string;
  content: (TextContent | ImageContent)[];
  details?: TDetails;
  isError: boolean;
  timestamp: number;
}
```

The current Rust `pi-ai` has a very simplified `Message { role: Role, content: String }`. **This must be expanded** to support block-level content.

### 2.3 Context and Tools

```typescript
interface Tool<TParameters extends TSchema = TSchema> {
  name: string;
  description: string;
  parameters: TParameters; // TypeBox JSON Schema
}

interface Context {
  systemPrompt?: string;
  messages: Message[];
  tools?: Tool[];
}
```

### 2.4 Usage

```typescript
interface Usage {
  input: number;         // non-cached, non-write prompt tokens
  output: number;        // completion tokens (includes reasoning if provider reports it)
  cacheRead: number;     // cache hit tokens
  cacheWrite: number;    // tokens written to cache this request
  totalTokens: number;
  cost: {
    input: number;
    output: number;
    cacheRead: number;
    cacheWrite: number;
    total: number;
  };
}
```

### 2.5 Model Descriptor

```typescript
interface Model<TApi extends Api> {
  id: string;
  name: string;
  api: TApi;                  // e.g. "openai-completions"
  provider: Provider;         // e.g. "openai", "openrouter"
  baseUrl: string;
  reasoning: boolean;         // Whether model supports reasoning/thinking
  input: ("text" | "image")[];
  cost: {
    input: number;            // $/M tokens
    output: number;
    cacheRead: number;
    cacheWrite: number;
  };
  contextWindow: number;
  maxTokens: number;
  headers?: Record<string, string>;
  compat?: TApi extends "openai-completions" ? OpenAICompletionsCompat : never;
}
```

### 2.6 Stop Reasons

```typescript
type StopReason = "stop" | "length" | "toolUse" | "error" | "aborted";
```

---

## 3. Provider Trait / Interface Design

### Original TypeScript Abstraction

Providers are **not classes**; they are objects registered in a global map keyed by `api` string:

```typescript
interface ApiProvider<TApi extends Api, TOptions extends StreamOptions> {
  api: TApi;
  stream: StreamFunction<TApi, TOptions>;
  streamSimple: StreamFunction<TApi, SimpleStreamOptions>;
}
```

High-level entry points (`src/stream.ts`):

```typescript
export function stream<TApi extends Api>(
  model: Model<TApi>,
  context: Context,
  options?: ProviderStreamOptions,
): AssistantMessageEventStream;

export async function complete<TApi extends Api>(...): Promise<AssistantMessage>
```

The registry lives in `api-registry.ts`:

- `registerApiProvider(provider, sourceId?)`
- `getApiProvider(api)`
- `unregisterApiProviders(sourceId)`
- `clearApiProviders()`

### Engineering Pattern to Preserve

1. **Registry pattern** decouples provider discovery from usage.
2. **Lazy loading** (`register-builtins.ts`) defers heavy SDK imports until first use.
3. **API-first dispatch** — the `api` field on `Model` determines which provider code path runs, not the `provider` field. Multiple providers can share the same `api`.

### Rust Direction

Current Rust has a single trait:

```rust
#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn generate(&self, req: GenerateRequest) -> Result<GenerateResponse, AIError>;
    fn name(&self) -> &'static str;
}
```

**What must change:**

- The trait should accept `Model`, `Context`, and options.
- Return an `AssistantMessageEventStream` (async stream of events), not a one-shot response.
- Add `stream_simple` (or a unified method with options enum) to mirror the TS API.
- Alternatively, keep a unified trait and let implementations expose both `generate` (non-streaming) and `stream` variants.

Suggested trait evolution:

```rust
#[async_trait]
pub trait ApiProvider: Send + Sync {
    fn api(&self) -> &'static str;

    fn stream(
        &self,
        model: &Model,
        context: &Context,
        options: StreamOptions,
    ) -> AssistantMessageEventStream;

    fn stream_simple(
        &self,
        model: &Model,
        context: &Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream;
}
```

`AssistantMessageEventStream` in Rust should be a wrapper around an `mpsc`-style channel or `tokio::sync::broadcast` that implements `Stream<Item = AssistantMessageEvent>` and can yield a final `AssistantMessage` via `.result()`.

---

## 4. Concrete Provider Implementations

### 4.1 OpenAI Completions (`openai-completions.ts`)

**API:** `openai-completions`  
**Used by:** openai, azure-openai-responses (indirectly), github-copilot, xai, groq, cerebras, openrouter, vercel-ai-gateway, zai, minimax, opencode, kimi-coding, etc.

**Key behaviors:**

- Uses the `openai` SDK with `baseURL` and `dangerouslyAllowBrowser: true`.
- Auth from `options.apiKey || getEnvApiKey(model.provider)`.
- **Compat auto-detection** (`detectCompat`) based on `provider` and `baseUrl`:
  - `supportsStore`, `supportsDeveloperRole`, `supportsReasoningEffort`, `supportsUsageInStreaming`, `maxTokensField`, `requiresToolResultName`, `requiresAssistantAfterToolResult`, `requiresThinkingAsText`, `thinkingFormat`, `zaiToolStream`, `supportsStrictMode`.
- Message conversion (`convertMessages`):
  - Normalizes tool call IDs via `transformMessages`.
  - Maps `systemPrompt` -> system message (developer role for reasoning models when `supportsDeveloperRole`).
  - User messages -> string or content-part array.
  - Assistant messages -> `content` string (never array for OpenAI Completions; joined text blocks) plus `tool_calls`.
  - Thinking blocks either mapped to custom signature attrs or converted to text.
  - Tool results -> `role: "tool"`. Images from tool results are emitted as a follow-up user message with image blocks if `requiresAssistantAfterToolResult` is true.
- Tool conversion (`convertTools`): `type: "function"`, includes `strict: false` conditionally.
- Streaming:
  - Reads `chunk.choices[0].delta`.
  - Handles `content`, `reasoning_content` / `reasoning` / `reasoning_text`.
  - Handles `tool_calls` deltas.
  - Normalizes usage from `chunk.usage` or `choice.usage`.
  - Stop reason mapping from `choice.finish_reason`.

**Rust rewrite notes:**

- Use `reqwest-eventsource` or a raw SSE parser (the TS code streams via the official OpenAI SDK).
- The compat table must be ported exactly; many subtle provider bugs are fixed there.
- `parseStreamingJson` is used to parse partial tool-call arguments.

### 4.2 OpenAI Responses (`openai-responses.ts` + `openai-responses-shared.ts`)

**API:** `openai-responses`  
**Used by:** openai, github-copilot, openai-codex

**Key behaviors:**

- Uses `client.responses.create({ ...params, stream: true })`.
- Params:
  - `input` is `ResponseInput` (converted from messages).
  - `prompt_cache_key` / `prompt_cache_retention` for cache control.
  - `store: false`.
  - Reasoning via `reasoning: { effort, summary }` when enabled.
- Message conversion (`convertResponsesMessages`):
  - Tool call IDs use pipe format: `{call_id}|{item_id}`. Normalization is provider-aware (only certain providers allowed to preserve the pipe).
  - Text blocks are replayed as `ResponseOutputMessage` with a stable `id` and optional `phase`.
  - Thinking is replayed as `ResponseReasoningItem`.
  - Tool results become `function_call_output` with `call_id`.
- Shared stream processing (`processResponsesStream`):
  - Handles events: `response.created`, `response.output_item.added`, `response.reasoning_summary_part.added`, `response.reasoning_summary_text.delta`, `response.content_part.added`, `response.output_text.delta`, `response.refusal.delta`, `response.function_call_arguments.delta`, `response.function_call_arguments.done`, `response.output_item.done`, `response.completed`, `response.failed`, `error`.
  - Cost multiplier for `serviceTier` (`flex` = 0.5x, `priority` = 2x).

**Rust rewrite notes:**

- The Responses API is still OpenAI-specific; if you don’t have a Rust SDK for it, use raw HTTP + SSE.
- Pay careful attention to the pipe-separated tool call ID logic; it is required for cross-turn continuity.

### 4.3 Anthropic Messages (`anthropic.ts`)

**API:** `anthropic-messages`

**Key behaviors:**

- SDK: `@anthropic-ai/sdk`.
- **OAuth stealth mode:** When API key contains `sk-ant-oat`, it sends Bearer auth + Claude Code identity headers (`user-agent`, `x-app`, `anthropic-beta`).
- Tool name normalization for OAuth: canonicalizes to Claude Code tool names (`Read`, `Write`, `Bash`, etc.) using case-insensitive match.
- Cache control:
  - `cacheRetention` -> `ttl: "1h"` for direct Anthropic API.
  - Cache control is added to system prompt blocks and last user message block.
- Extended thinking:
  - Opus 4.6 / Sonnet 4.6 use **adaptive thinking** (`thinking: { type: "adaptive" }`, `output_config: { effort }`).
  - Older models use **budget-based thinking** (`thinking: { type: "enabled", budget_tokens }`).
- `interleavedThinking` beta header for non-adaptive models.
- Stream events:
  - `message_start` -> seed usage.
  - `content_block_start` -> text, thinking, redacted_thinking, tool_use.
  - `content_block_delta` -> text_delta, thinking_delta, input_json_delta, signature_delta.
  - `content_block_stop` -> finalize block.
  - `message_delta` -> stop_reason & usage update.

**Rust rewrite notes:**

- Anthropic has an official Rust SDK? (Verify.) If not, raw HTTP + SSE is fine.
- The OAuth path is unusual but required for parity.
- Budget calculation lives in `simple-options.ts` (`adjustMaxTokensForThinking`).

### 4.4 Mistral (`mistral.ts`)

**API:** `mistral-conversations`

**Key behaviors:**

- SDK: `@mistralai/mistralai`.
- Tool call IDs limited to 9 alphanumeric chars. A normalizer with collision handling (`shortHash`) is used.
- Message conversion very similar to OpenAI Completions but uses Mistral-specific content chunk types.
- Supports `promptMode: "reasoning"` for reasoning models.
- Error formatting extracts `statusCode` and `body` from SDK errors.

### 4.5 Google Generative AI (`google.ts` + `google-shared.ts`)

**API:** `google-generative-ai`

**Key behaviors:**

- SDK: `@google/genai`.
- **Thought signatures:**
  - `thought: true` marks a part as thinking content.
  - `thoughtSignature` is an opaque encrypted payload and can appear on ANY part (text, functionCall). It must be preserved for replay.
  - `isThinkingPart(part)` checks `part.thought === true`.
  - `retainThoughtSignature(existing, incoming)` prevents overwriting with `None`.
  - `SKIP_THOUGHT_SIGNATURE = "skip_thought_signature_validator"` sentinel for unsigned function calls on Gemini 3.
- Tool calls:
  - Google function calls may omit `id`. The code generates one if missing/duplicate (`toolCallCounter`).
  - Some models (Claude via Antigravity, gpt-oss) require explicit IDs (`requiresToolCallId`).
- Model-specific reasoning config:
  - Gemini 3 Pro / Flash / Gemma 4 use `thinkingLevel` (`MINIMAL`, `LOW`, `MEDIUM`, `HIGH`).
  - Gemini 2.x uses `thinkingBudget`.
  - Disabling thinking is model-dependent (some models cannot fully disable).
- Multimodal tool results:
  - Gemini 3+ supports images inside `functionResponse.parts`.
  - For older models, images are split into a separate user message.

### 4.6 Google Vertex / Gemini CLI (`google-vertex.ts`, `google-gemini-cli.ts`)

These reuse `google-shared.ts` for message/tool conversion but differ in auth and client construction:

- **Vertex:** ADC credentials or explicit API key.
- **Gemini CLI:** Uses internal CLI auth path and custom headers.

### 4.7 Amazon Bedrock (`amazon-bedrock.ts`)

**API:** `bedrock-converse-stream`

**Key behaviors:**

- Uses AWS SDK `@aws-sdk/client-bedrock-runtime`.
- Proxy / HTTP1.1 support via `@smithy/node-http-handler` and `proxy-agent`.
- Credential sources: `AWS_PROFILE`, `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`, `AWS_BEARER_TOKEN_BEDROCK`, ECS, IRSA.
- **Prompt caching:**
  - Only for certain Claude models (3.5 Haiku, 3.7 Sonnet, 4.x).
  - Application inference profiles need `AWS_BEDROCK_FORCE_CACHE=1`.
  - Cache points appended to system prompt and last user message.
  - `cacheRetention` maps to `ttl: ONE_HOUR`.
- **Thinking signatures:**
  - Only Anthropic Claude models accept `signature` in `reasoningContent.reasoningText`.
  - For non-Claude models, signature is omitted to avoid validation errors.
- **Adaptive vs budget-based thinking** for Claude models (same mapping as Anthropic direct).
- Consecutive tool results are merged into a single user message (Bedrock requirement).
- Stream items handled:
  - `messageStart`, `contentBlockStart`, `contentBlockDelta`, `contentBlockStop`, `messageStop`, `metadata`, plus SDK exceptions.

### 4.8 Faux Provider (`faux.ts`)

**Critical for testing.**

```typescript
export interface FauxProviderRegistration {
  api: string;
  models: [Model<string>, ...Model<string>[]];
  getModel(modelId?: string): Model<string> | undefined;
  state: { callCount: number };
  setResponses(responses: FauxResponseStep[]): void;
  appendResponses(responses: FauxResponseStep[]): void;
  getPendingResponseCount(): number;
  unregister(): void;
}
```

- Responses can be static `AssistantMessage` objects or factories `(context, options, state, model) => AssistantMessage`.
- Emits realistic token-level deltas using configurable `tokenSize` min/max and optional `tokensPerSecond` pacing.
- Estimates usage via `estimateTokens(text) = ceil(len / 4)`.
- Simulates **prompt caching** per `sessionId` using longest-common-prefix.
- Supports abort signals.

**Rust rewrite notes:**

- This is the backbone of the integration test suite. Port this _before_ real providers.

---

## 5. Streaming Response Design

### TypeScript Event Protocol

```typescript
export type AssistantMessageEvent =
  | { type: "start"; partial: AssistantMessage }
  | { type: "text_start"; contentIndex: number; partial: AssistantMessage }
  | { type: "text_delta"; contentIndex: number; delta: string; partial: AssistantMessage }
  | { type: "text_end"; contentIndex: number; content: string; partial: AssistantMessage }
  | { type: "thinking_start"; contentIndex: number; partial: AssistantMessage }
  | { type: "thinking_delta"; contentIndex: number; delta: string; partial: AssistantMessage }
  | { type: "thinking_end"; contentIndex: number; content: string; partial: AssistantMessage }
  | { type: "toolcall_start"; contentIndex: number; partial: AssistantMessage }
  | { type: "toolcall_delta"; contentIndex: number; delta: string; partial: AssistantMessage }
  | { type: "toolcall_end"; contentIndex: number; toolCall: ToolCall; partial: AssistantMessage }
  | { type: "done"; reason: "stop" | "length" | "toolUse"; message: AssistantMessage }
  | { type: "error"; reason: "aborted" | "error"; error: AssistantMessage };
```

### EventStream Base Class (`utils/event-stream.ts`)

- Generic `AsyncIterable<T>` implementation using an internal queue + waiters.
- `push(event)` enqueues or resolves a waiting consumer.
- `end(result?)` marks done and resolves the final result promise.
- `result()` returns `Promise<R>`.
- `AssistantMessageEventStream` subclasses it with completion predicates.

### Rust Equivalent

Use `tokio::sync::mpsc` or `async-channel` under a wrapper.

```rust
pub struct AssistantMessageEventStream {
    rx: tokio::sync::mpsc::UnboundedReceiver<AssistantMessageEvent>,
    result_rx: tokio::sync::oneshot::Receiver<AssistantMessage>,
}

impl AssistantMessageEventStream {
    pub async fn next(&mut self) -> Option<AssistantMessageEvent> { ... }
    pub async fn result(self) -> AssistantMessage { ... }
}
```

The producer side should be a lightweight handle (`EventStreamHandle`) that implements `Clone`/`Send` so provider code can push events from an async task.

---

## 6. Tool Calling Design

### Tool Definition

```typescript
interface Tool {
  name: string;
  description: string;
  parameters: TSchema; // TypeBox JSON Schema
}
```

In Rust, `parameters` should be `serde_json::Value` (a JSON Schema object). Validation can use `jsonschema` crate instead of AJV.

### Conversion Rules per Provider

| Provider | Tool Format |
|----------|-------------|
| OpenAI Completions | `tools: [{ type: "function", function: { name, description, parameters, strict? } }]` |
| OpenAI Responses | `tools: [{ type: "function", name, description, parameters, strict? }]` |
| Anthropic | `tools: [{ name, description, input_schema: { type: "object", properties, required } }]` |
| Mistral | Same shape as OpenAI Completions |
| Google | `tools: [{ functionDeclarations: [{ name, description, parametersJsonSchema }] }]` |
| Bedrock | `toolConfig: { tools: [{ toolSpec: { name, description, inputSchema: { json } } }], toolChoice }` |

### Normalization (`transform-messages.ts`)

Because providers have incompatible rules, `transformMessages` runs **before** every provider’s `buildParams`:

1. **Thinking block normalization**
   - If `isSameModel` (same api, provider, model id): keep thinking blocks (with signatures).
   - If redacted thinking and NOT same model: drop it.
   - If empty thinking: drop it.
   - Otherwise: convert to plain `TextContent`.

2. **Tool call ID normalization**
   - OpenAI Responses generates IDs like `call_id|item_id` (450+ chars, special chars).
   - Anthropic requires `^[a-zA0-9_-]+$` max 64 chars.
   - Providers can pass a `normalizeToolCallId(id, model, source)` callback.
   - A bidirectional map is built so `toolResult` messages get updated IDs too.

3. **Orphaned tool call repair**
   - If an assistant message emits tool calls but no matching `toolResult` follows (e.g., user interrupts, or stream errors), synthetic error tool results are injected.
   - This prevents API errors on replay.

4. **Errored/aborted assistant message removal**
   - Assistant messages with `stopReason === "error" | "aborted"` are stripped entirely.
   - Partial tool calls or reasoning without following content cause provider errors.

---

## 7. Testing Strategy

### Faux Provider Usage

The TS suite relies heavily on `faux.ts` to test streaming protocol edge cases without network calls.

Key test files:

- `stream.test.ts` -- Protocol-level assertions (event ordering, delta fidelity).
- `abort.test.ts` -- AbortSignal handling across all block types.
- `cross-provider-handoff.test.ts` -- Tool call ID normalization and thinking block conversion.
- `tool-call-without-result.test.ts` -- Synthetic orphaned tool result injection.
- `transform-messages-copilot-openai-to-anthropic.test.ts` -- ID normalization.
- `empty.test.ts` -- Empty message handling.
- `total-tokens.test.ts` -- Usage arithmetic correctness.
- `cache-retention.test.ts` -- Prompt caching behavior.
- `tokens.test.ts` -- Token estimation parity.

### Mock Pattern

The current Rust crate has `MockProvider`. It should be kept but expanded to return `AssistantMessageEventStream` rather than a one-shot string.

### Integration Tests

- `openai_compatible_generate_works` (already exists, `#[ignore]`).
- Each real provider should have a gated integration test behind an env var.

### Fixture Pattern

Put reusable model descriptors in `tests/fixtures/models.rs` so tests don’t depend on the generated catalog.

---

## 8. Engineering Patterns Worth Preserving

1. **Lazy-loaded providers**
   - TS uses dynamic `import()` in `register-builtins.ts`. In Rust, use conditional compilation (`cfg(feature = "...")`) or on-demand module initialization.

2. **Provider-agnostic simple options**
   - `buildBaseOptions(model, options, apiKey)` + `adjustMaxTokensForThinking` centralizes reasoning budget math. Port directly.

3. **Auto-compat detection**
   - The `detectCompat` / `getCompat` logic in `openai-completions.ts` is a goldmine of provider-specific workarounds. Do not omit it.

4. **sanitizeSurrogates**
   - Unicode surrogate cleanup before every provider payload prevents serialization errors.

5. **parseStreamingJson**
   - Incomplete JSON parsing during streaming tool calls. Use `partial-json` equivalent or a best-effort Rust serde fallback.

6. **Error classification (`overflow.ts`)**
   - `isContextOverflow` checks error text regexes + silent overflow via `usage.input > contextWindow`. Required for retry/trim logic upstream.

7. **Cost calculation**
   - `calculateCost(model, usage)` mutates usage in place. Preserve per-model cost fields.

8. **Environment key resolution**
   - `env-api-keys.ts` has non-trivial rules (e.g., Vertex ADC credential checks, Bedrock auth heuristics). Port carefully.

---

## 9. Gaps vs Current pi-rust Implementation

The existing Rust crate (`crates/pi-ai/`) is a **barebones skeleton**. Here is a gap analysis:

| Feature | TS pi-ai | Rust pi-ai | Gap |
|---------|----------|------------|-----|
| **Message model** | Rich blocks (text/thinking/image/toolCall) | `String`-only `Message` | Major |
| **Streaming API** | Full `AssistantMessageEventStream` protocol | None (one-shot `generate` only) | Major |
| **Provider registry** | Global `api-registry.ts` | None | Major |
| **Model catalog** | `models.generated.ts` + registry | None | Major |
| **Cost calc** | `calculateCost()` | None | Major |
| **Tool support** | Define, convert, validate tools | None | Major |
| **Multi-provider** | 10+ providers | `Mock`, `OpenAICompatible` only | Major |
| **OpenAI Completions** | Full compat matrix | Basic non-streaming chat/completions | Major |
| **OpenAI Responses** | Full streaming, reasoning, caching | None | Major |
| **Anthropic** | OAuth, adaptive/budget thinking, caching | None | Major |
| **Mistral** | Full streaming | None | Major |
| **Google** | Thought signatures, multimodal tool results | None | Major |
| **Bedrock** | ConverseStream, caching, proxy support | None | Major |
| **Faux provider** | Extensive test provider | None | Major |
| **transform-messages** | Cross-provider normalization | None | Major |
| **Context overflow** | Regex + silent detection | None | Major |
| **JSON parse streaming** | partial-json fallback | None | Medium |
| **Unicode sanitize** | `sanitizeSurrogates` | None | Medium |
| **Env key resolution** | Complex per-provider rules | Hardcoded in test only | Medium |

### Recommended Rewrite Order

1. **Core types** (`message.rs`, `types.rs`, `stream.rs`) — get the event protocol right.
2. **Faux provider + transform-messages** — enables a rich test harness immediately.
3. **Api registry + Model catalog skeleton** — so higher layers can resolve providers.
4. **OpenAI Completions provider** (with SSE streaming) — highest-value real provider.
5. **Anthropic provider** — second most-used.
6. **OpenAI Responses provider** — required for o-series / reasoning models.
7. **Google, Mistral, Bedrock** — in priority order of downstream usage.
8. **Utilities** (validation, overflow, sanitize, env-api-keys) — port as needed by providers.

---

## 10. Notable Surprises for the Rewriter

- **Tool call IDs are not opaque.** OpenAI Responses uses `call_id|item_id` (450+ chars). Anthropic allows only 64 chars of `[a-zA-Z0-9_-]`. Mistral allows exactly 9 alphanumeric chars. Normalization and bidirectional mapping are mandatory.
- **Thinking signatures are provider-specific and fragile.** Google uses base64 `thoughtSignature` on arbitrary parts. Anthropic uses `signature` on `thinking` blocks. OpenAI Responses serializes the entire `reasoning` item as JSON. Redacted thinking from OpenAI cannot be replayed to a different model.
- **The `stream` vs `complete` split is not just convenience.** The whole architecture assumes streaming is primary; `complete` is `stream(...).result()`. The Rust rewrite should design around streaming first.
- **`RegisterFauxProvider` is the secret weapon.** A huge amount of TS test coverage comes from the faux provider. Investing in a Rust equivalent early pays off massively.
- **Compat shims are load-bearing.** The `OpenAICompletionsCompat` object exists because a dozen "OpenAI-compatible" providers each deviate in small but critical ways. A clean "just use the OpenAI spec" implementation will break in production.

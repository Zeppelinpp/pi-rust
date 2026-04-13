# pi-ai Usage

Example usage of the `pi-ai` crate with the `OpenAICompatibleProvider`.

```rust
use pi_ai::{GenerateRequest, LLMProvider, Message, OpenAICompatibleConfig, OpenAICompatibleProvider};

#[tokio::main]
async fn main() {
    let provider = OpenAICompatibleProvider::new(OpenAICompatibleConfig {
        api_key: "your-api-key".into(),
        base_url: "https://api.openai.com".into(),
    });

    let req = GenerateRequest::new(
        "gpt-4o",
        vec![Message::user("Say hello in one short sentence.")],
    )
    .temperature(0.7)
    .max_tokens(64);

    let resp = provider.generate(req).await.unwrap();
    println!("{}", resp.content);
}
```

See also: `crates/pi-ai/examples/openai_compatible_demo.rs`

## Streaming API

`pi-ai` provides an event-driven streaming layer via `AssistantMessageEventStream`. This is the primary interface for real-time token delivery and will be used by downstream agent runtimes.

```rust
use pi_ai::{AssistantMessageEvent, AssistantMessageEventStream, EventStreamHandle, Message, StopReason};

async fn consume_stream() {
    let (mut stream, handle) = AssistantMessageEventStream::new();

    // Producer task (typically inside a provider implementation)
    tokio::spawn(async move {
        let partial = Message::user("hello");
        handle.push(AssistantMessageEvent::Start { partial: partial.clone() });
        handle.push(AssistantMessageEvent::TextDelta {
            content_index: 0,
            delta: "world".into(),
            partial,
        });
        // ... finalize with Done or Error
    });

    // Consumer: iterate events or skip to the final message
    while let Some(event) = stream.next().await {
        match event {
            AssistantMessageEvent::TextDelta { delta, .. } => print!("{}", delta),
            AssistantMessageEvent::Done { message, .. } => {
                println!("\nStream finished: {:?}", message);
                break;
            }
            _ => {}
        }
    }
}
```

Key types:
- `AssistantMessageEventStream` — async consumer with `next()` and `result()`
- `EventStreamHandle` — cloneable producer handle for pushing events
- `AssistantMessageEvent` — protocol events: `Start`, `TextDelta`, `ThinkingDelta`, `ToolCallDelta`, `Done`, `Error`, etc.

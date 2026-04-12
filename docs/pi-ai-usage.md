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

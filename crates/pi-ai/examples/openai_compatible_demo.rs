use pi_ai::{
    ApiProvider, AssistantMessageEvent, Context, Message, Model,
    OpenAICompatibleConfig, OpenAICompatibleProvider, StreamOptions, Tool,
};

#[tokio::main]
async fn main() {
    let api_key = std::env::var("API_KEY").expect("API_KEY not set");
    let base_url = std::env::var("BASE_URL").expect("BASE_URL not set");
    let model_id = std::env::var("MODEL").expect("MODEL not set");

    let provider = OpenAICompatibleProvider::new(OpenAICompatibleConfig { api_key, base_url });

    let model = Model {
        id: model_id.clone(),
        name: model_id.clone(),
        api: "openai-completions".into(),
        provider: "openai".into(),
        ..Default::default()
    };

    let weather_tool = Tool {
        name: "get_weather".into(),
        description: "Get current weather for a city".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "city": {
                    "type": "string",
                    "description": "The city name, e.g. Beijing"
                }
            },
            "required": ["city"]
        }),
    };

    let context = Context {
        system_prompt: Some("You are a helpful assistant. Use tools when needed.".into()),
        messages: vec![Message::user("What's the weather like in Shanghai?")],
        tools: Some(vec![weather_tool]),
    };

    let options = StreamOptions {
        temperature: Some(0.7),
        ..Default::default()
    };

    let mut stream = provider.stream(&model, &context, options);
    let mut full_text = String::new();
    let mut full_reasoning = String::new();

    println!("--- Streaming start ---");

    while let Some(event) = stream.next().await {
        match event {
            AssistantMessageEvent::Start { .. } => println!("[Start]"),
            AssistantMessageEvent::TextDelta { delta, .. } => {
                print!("{}", delta);
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                full_text.push_str(&delta);
            }
            AssistantMessageEvent::ThinkingDelta { delta, .. } => {
                print!("[thinking: {}]", delta);
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                full_reasoning.push_str(&delta);
            }
            AssistantMessageEvent::ToolCallStart { content_index, .. } => {
                println!("\n[ToolCallStart idx={}]", content_index);
            }
            AssistantMessageEvent::ToolCallDelta { delta, content_index, .. } => {
                println!("[ToolCallDelta idx={}]: {}", content_index, delta);
            }
            AssistantMessageEvent::Done { message, .. } => {
                let reason = match &message {
                    Message::Assistant { stop_reason, .. } => format!("{:?}", stop_reason),
                    _ => "unknown".into(),
                };
                println!("\n[Done] stop_reason={}", reason);
                break;
            }
            AssistantMessageEvent::Error { error, .. } => {
                println!("\n[Error] {:?}", error);
                break;
            }
            other => println!("\n[Other] {:?}", other),
        }
    }

    println!("\n--- Streaming end ---");
    println!("Full text ({} chars): {}", full_text.len(), full_text);
    if !full_reasoning.is_empty() {
        println!("Full reasoning ({} chars): {}", full_reasoning.len(), full_reasoning);
    }
}

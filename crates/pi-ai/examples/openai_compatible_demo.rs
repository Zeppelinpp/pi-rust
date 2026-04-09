use pi_ai::{
    GenerateRequest, LLMProvider, Message, OpenAICompatibleConfig, OpenAICompatibleProvider,
};

#[tokio::main]
async fn main() {
    let api_key = std::env::var("API_KEY").expect("API_KEY not set");
    let base_url = std::env::var("BASE_URL").expect("BASE_URL not set");
    let model = std::env::var("MODEL").expect("MODEL not set");

    let provider = OpenAICompatibleProvider::new(OpenAICompatibleConfig { api_key, base_url });

    let req = GenerateRequest::new(
        model,
        vec![
            Message::system("You are a helpful assistant."),
            Message::user("Introduce yourself briefly."),
        ],
    );

    let resp = provider.generate(req).await.unwrap();
    println!("{}", resp.content);
}

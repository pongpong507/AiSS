// New web server entry point
use axum::{
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
// NOTE: This import assumes llm_gateway is available as a dependency and has a public function named 'generate_content'
use llm_gateway::llm_gateway::generate_content;

#[tokio::main]
async fn main() {
    // Build the router that will handle all incoming web requests
    let app = Router::new()
        .route("/api/generate", post(api_generate_content));

    // Get the host address, defaulting to localhost on port 3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    println!("🚀 Server listening on http://{}", addr);

    // Run the server, binding it to the address
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// Handler for the /api/generate endpoint
async fn api_generate_content(
    // TODO: In a real scenario, we would read the request body (the prompt) here.
) -> String {
    println!("Received request to generate content.");

    // Placeholder call to the core logic
    // llm_gateway::generate_content("Placeholder prompt").await.unwrap_or_default()

    "Successfully hit the /api/generate endpoint! (Logic needs implementation)".to_string()
}
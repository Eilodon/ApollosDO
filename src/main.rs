use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env if present
    let _ = dotenvy::dotenv();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "info,apollos_ui_navigator=debug".to_string()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let digital_agent = std::sync::Arc::new(apollos_ui_navigator::digital_agent::DigitalAgent::new().await?);

    // ADR-012: In-memory session store — no external database dependency
    let sessions = apollos_ui_navigator::session::SessionStore::default();

    let state = apollos_ui_navigator::AppState {
        sessions,
        ws_registry: apollos_ui_navigator::ws_registry::WebSocketRegistry::new(),
        fallback: apollos_ui_navigator::human_fallback::HumanFallbackService::new(),
        digital_agent,
    };

    let router = apollos_ui_navigator::build_router(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}

pub mod agent;
pub mod browser_executor;
pub mod demo_handler;
pub mod digital_agent;
pub mod human_fallback;
pub mod nova_reasoning_client;
pub mod session;
pub mod status_bus;
pub mod types;
pub mod ws_registry;

use axum::{routing::get, Router};

#[derive(Clone)]
pub struct AppState {
    pub sessions: session::SessionStore,
    pub ws_registry: ws_registry::WebSocketRegistry,
    pub fallback: human_fallback::HumanFallbackService,
    pub digital_agent: std::sync::Arc<digital_agent::DigitalAgent>,
}

async fn healthz() -> &'static str {
    "ok"
}

pub fn build_router(state: AppState) -> Router {
    let mut router = Router::new()
        .route("/healthz", get(healthz))
        .with_state(state.clone());

    if std::env::var("DEMO_MODE").as_deref() == Ok("1") {
        tracing::info!("Demo mode enabled — /demo/* routes active");
        router = router.merge(demo_handler::demo_router().with_state(state));
    }

    router
}

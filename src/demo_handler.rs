use axum::{
    extract::{State},
    response::{
        sse::{Event, Sse},
        IntoResponse,
    },
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use std::collections::VecDeque;
use std::sync::Mutex as StdMutex;

use crate::{
    AppState,
    digital_agent::{DigitalResult, DigitalSessionContext},
    session::DigitalAgentHandle,
};
use crate::types::MotionState;
use tokio_util::sync::CancellationToken;

// Global demo session ID — single session cho demo
const DEMO_SESSION_ID: &str = "demo-session-001";

#[derive(Deserialize)]
pub struct StartTaskRequest {
    pub intent: String,
}

#[derive(Serialize)]
pub struct StartTaskResponse {
    pub task_id: String,
    pub status: String,
}

// SSE broadcast channel cho status updates
static STATUS_TX: std::sync::OnceLock<broadcast::Sender<String>> =
    std::sync::OnceLock::new();

fn get_status_tx() -> &'static broadcast::Sender<String> {
    STATUS_TX.get_or_init(|| {
        let (tx, _) = broadcast::channel(64);
        tx
    })
}

// ADR-030: SSE replay buffer for late subscribers
const REPLAY_BUFFER_SIZE: usize = 50;
static REPLAY_BUFFER: std::sync::OnceLock<StdMutex<VecDeque<String>>> =
    std::sync::OnceLock::new();

fn get_replay_buffer() -> &'static StdMutex<VecDeque<String>> {
    REPLAY_BUFFER.get_or_init(|| StdMutex::new(VecDeque::with_capacity(REPLAY_BUFFER_SIZE)))
}

/// ADR-030: Broadcast + buffer for replay
fn broadcast_status(msg: String) {
    let tx = get_status_tx();
    let _ = tx.send(msg.clone());
    if let Ok(mut buf) = get_replay_buffer().lock() {
        if buf.len() >= REPLAY_BUFFER_SIZE {
            buf.pop_front();
        }
        buf.push_back(msg);
    }
}

/// POST /demo/start_task
pub async fn start_task(
    State(state): State<AppState>,
    Json(req): Json<StartTaskRequest>,
) -> impl IntoResponse {
    // Cancel bất kỳ task cũ nào
    state.sessions.cancel_digital_agent(
        DEMO_SESSION_ID,
        crate::session::DigitalAgentCancelReason::ReRegister,
    ).await;

    let intent = req.intent.clone();
    let sessions = state.sessions.clone();
    let ws_registry = state.ws_registry.clone();
    let fallback = state.fallback.clone();
    let agent = state.digital_agent.clone();

    // Emit initial status
    broadcast_status(format!("🚀 Starting: {}", intent));

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    // [FIX] Ensure session exists so get_reply_slot returns the REAL Arc from SessionState
    state.sessions.touch_session(
        DEMO_SESSION_ID,
        None, None, None, None, None, false
    ).await;

    let reply_slot = state.sessions.get_reply_slot(DEMO_SESSION_ID).await;
    let browser_slot = state.sessions.get_browser_executor_slot(DEMO_SESSION_ID).await;

    let ctx = DigitalSessionContext {
        motion_state: MotionState::Stationary,
        session_id: DEMO_SESSION_ID.to_string(),
        ws_registry: ws_registry.clone(),
        fallback: fallback.clone(),
        sessions: sessions.clone(),
        reply_tx_slot: reply_slot,
        browser_executor_slot: browser_slot,
    };

    let task = tokio::spawn(async move {
        // Lưu ý: NovaReasoningClient đã được inject vào `agent` (DigitalAgent)
        let result = agent.execute_with_cancel(&intent, cancel_clone, ctx).await;

        match &result {
            DigitalResult::Done(summary) => {
                broadcast_status(format!("✅ Done: {}", summary));
            }
            DigitalResult::NeedHuman(reason) => {
                broadcast_status(format!("🤝 Escalating to human: {}", reason));
            }
            DigitalResult::Failed(err) => {
                broadcast_status(format!("❌ Failed: {}", err));
            }
        }
        
        // Clean up handle when task finishes
        sessions.clear_digital_agent_handle(DEMO_SESSION_ID).await;
        result
    });

    state.sessions.set_digital_agent_handle(
        DEMO_SESSION_ID,
        DigitalAgentHandle { cancel, task },
    ).await;

    Json(StartTaskResponse {
        task_id: DEMO_SESSION_ID.to_string(),
        status: "started".to_string(),
    })
}

/// POST /demo/trigger_hard_stop
pub async fn trigger_hard_stop(
    State(state): State<AppState>,
) -> impl IntoResponse {

    tracing::warn!(
        "⚠️  DEMO HARD STOP TRIGGERED — cancelling digital agent"
    );
    broadcast_status("⚠️  HARD STOP FIRED — Safety system interrupt".to_string());

    state.sessions.cancel_digital_agent(
        DEMO_SESSION_ID,
        crate::session::DigitalAgentCancelReason::HardStop,
    ).await;

    broadcast_status("🛡️  Digital agent cancelled — Physical safety takes priority".to_string());

    Json(serde_json::json!({
        "status": "hard_stop_fired",
        "session_id": DEMO_SESSION_ID,
        "message": "Digital agent cancelled. Safety directive active."
    }))
}

/// GET /demo/screenshot
pub async fn get_screenshot(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let slot = state.sessions.get_browser_executor_slot(DEMO_SESSION_ID).await;
    
    // Lấy screenshot từ executor hiện tại
    let result = if let Some(executor) = slot.lock().await.as_ref() {
        executor.screenshot().await
    } else {
        Err(anyhow::anyhow!("No active browser"))
    };

    match result {
        Ok(bytes) => (
            [("content-type", "image/png")],
            bytes,
        ).into_response(),
        Err(_) => (
            axum::http::StatusCode::NOT_FOUND,
            "No active browser screenshot available",
        ).into_response(),
    }
}

#[derive(Deserialize)]
pub struct UserReplyRequest {
    pub answer: String,
}

/// POST /demo/user_reply
pub async fn user_reply(
    State(state): State<AppState>,
    Json(req): Json<UserReplyRequest>,
) -> impl IntoResponse {
    let status_tx = get_status_tx();

    let delivered = state
        .sessions
        .send_user_reply(DEMO_SESSION_ID, req.answer.clone())
        .await;

    if delivered {
        let _ = status_tx.send(format!("👤 User replied: {}", req.answer));
        Json(serde_json::json!({
            "status": "delivered",
            "answer": req.answer
        }))
    } else {
        Json(serde_json::json!({
            "status": "no_pending_question",
            "note": "Agent is not waiting for a reply right now"
        }))
    }
}

/// GET /demo/status — ADR-030: Replay buffered messages + live stream
pub async fn status_stream() -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    // Replay buffered history for late subscribers
    let replay = {
        let buf = get_replay_buffer().lock().unwrap_or_else(|e| e.into_inner());
        buf.iter().cloned().collect::<Vec<_>>()
    };

    let replay_stream = tokio_stream::iter(
        replay.into_iter().map(|text| Ok(Event::default().data(text)))
    );

    // Then chain with live broadcast
    let rx = get_status_tx().subscribe();
    let live_stream = BroadcastStream::new(rx)
        .filter_map(|msg| match msg {
            Ok(text) => Some(Ok(Event::default().data(text))),
            Err(_) => None,
        });

    Sse::new(replay_stream.chain(live_stream))
        .keep_alive(axum::response::sse::KeepAlive::default())
}

/// Register demo routes
pub fn demo_router() -> Router<AppState> {
    Router::new()
        .route("/demo/start_task", post(start_task))
        .route("/demo/trigger_hard_stop", post(trigger_hard_stop))
        .route("/demo/user_reply", post(user_reply))
        .route("/demo/screenshot", axum::routing::get(get_screenshot))
        .route("/demo/status", axum::routing::get(status_stream))
}

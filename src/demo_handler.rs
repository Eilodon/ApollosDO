use axum::{
    extract::State,
    response::{
        sse::{Event, Sse},
        Html, IntoResponse,
    },
    routing::get,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::agent::{classify_intent, Intent};
use crate::types::MotionState;
use crate::{
    digital_agent::{DigitalResult, DigitalSessionContext},
    session::DigitalAgentHandle,
    status_bus, AppState,
};
use tokio_util::sync::CancellationToken;

// Global demo session ID — single session cho demo
const DEMO_SESSION_ID: &str = "demo-session-001";

#[derive(Deserialize)]
pub struct StartTaskRequest {
    pub intent: String,
    pub motion_state: Option<String>, // ADR-035: optional, defaults to "stationary"
}

#[derive(Serialize)]
pub struct StartTaskResponse {
    pub task_id: String,
    pub status: String,
}

/// POST /demo/start_task
pub async fn start_task(
    State(state): State<AppState>,
    Json(req): Json<StartTaskRequest>,
) -> impl IntoResponse {
    // ADR-033: Clear replay buffer FIRST — prevents old task history leaking into new task
    // Critical for demo retry scenarios (judge calling start_task multiple times)
    status_bus::clear_replay();

    // ADR-035: Motion-aware intent classification — safety gate
    let motion_state = match req.motion_state.as_deref() {
        Some("running") => MotionState::Running,
        Some("walking_fast") => MotionState::WalkingFast,
        Some("walking_slow") => MotionState::WalkingSlow,
        _ => MotionState::Stationary,
    };

    match classify_intent(&req.intent, motion_state.clone()) {
        Intent::Physical => {
            status_bus::publish(format!(
                "Movement detected. The digital task was blocked for safety. Intent: '{}'",
                req.intent
            ));
            return Json(serde_json::json!({
                "task_id": DEMO_SESSION_ID,
                "status": "physical_safety_mode",
                "message": "You are moving. The digital task was paused for safety.",
                "intent": req.intent,
                "motion_state": req.motion_state
            }))
            .into_response();
        }
        Intent::Digital(_) => {
            // proceed with normal digital task flow below
        }
    }

    // Cancel bất kỳ task cũ nào
    state
        .sessions
        .cancel_digital_agent(
            DEMO_SESSION_ID,
            crate::session::DigitalAgentCancelReason::ReRegister,
        )
        .await;

    let intent = req.intent.clone();
    let sessions = state.sessions.clone();
    let ws_registry = state.ws_registry.clone();
    let fallback = state.fallback.clone();
    let agent = state.digital_agent.clone();

    // Emit initial status
    status_bus::publish(format!("Starting task: {}", intent));

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    // [FIX] Ensure session exists so get_reply_slot returns the REAL Arc from SessionState
    state
        .sessions
        .touch_session(DEMO_SESSION_ID, None, None, None, None, None, false)
        .await;

    let reply_slot = state.sessions.get_reply_slot(DEMO_SESSION_ID).await;
    let browser_slot = state
        .sessions
        .get_browser_executor_slot(DEMO_SESSION_ID)
        .await;

    let ctx = DigitalSessionContext {
        motion_state, // ADR-035: use classified motion state, not hardcoded Stationary
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
                tracing::info!(
                    session_id = DEMO_SESSION_ID,
                    "Demo task completed: {}",
                    summary
                );
                status_bus::publish(format!("Done: {}", summary));
            }
            DigitalResult::NeedHuman(reason) => {
                tracing::warn!(
                    session_id = DEMO_SESSION_ID,
                    "Demo task escalated to human support: {}",
                    reason
                );
                status_bus::publish(format!("Escalating to human support: {}", reason));
            }
            DigitalResult::Failed(err) => {
                tracing::error!(session_id = DEMO_SESSION_ID, "Demo task failed: {}", err);
                status_bus::publish(format!("Failed: {}", err));
            }
        }

        // Clean up handle when task finishes
        sessions.clear_digital_agent_handle(DEMO_SESSION_ID).await;
        result
    });

    state
        .sessions
        .set_digital_agent_handle(DEMO_SESSION_ID, DigitalAgentHandle { cancel, task })
        .await;

    Json(StartTaskResponse {
        task_id: DEMO_SESSION_ID.to_string(),
        status: "started".to_string(),
    })
    .into_response()
}

/// POST /demo/trigger_hard_stop
pub async fn trigger_hard_stop(State(state): State<AppState>) -> impl IntoResponse {
    tracing::warn!("DEMO HARD STOP TRIGGERED — cancelling digital agent");
    status_bus::publish(
        "Hard stop triggered. The safety system interrupted the agent.".to_string(),
    );

    state
        .sessions
        .cancel_digital_agent(
            DEMO_SESSION_ID,
            crate::session::DigitalAgentCancelReason::HardStop,
        )
        .await;

    status_bus::publish(
        "The digital agent was cancelled. Physical safety takes priority.".to_string(),
    );

    Json(serde_json::json!({
        "status": "hard_stop_fired",
        "session_id": DEMO_SESSION_ID,
        "message": "Digital agent cancelled. Safety directive active."
    }))
}

/// GET /demo/screenshot
pub async fn get_screenshot(State(state): State<AppState>) -> impl IntoResponse {
    let slot = state
        .sessions
        .get_browser_executor_slot(DEMO_SESSION_ID)
        .await;

    // Lấy screenshot từ executor hiện tại
    let result = if let Some(executor) = slot.lock().await.as_ref() {
        executor.screenshot().await
    } else {
        Err(anyhow::anyhow!("No active browser"))
    };

    match result {
        Ok(bytes) => ([("content-type", "image/png")], bytes).into_response(),
        Err(_) => (
            axum::http::StatusCode::NOT_FOUND,
            "No active browser screenshot available",
        )
            .into_response(),
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
    let delivered = state
        .sessions
        .send_user_reply(DEMO_SESSION_ID, req.answer.clone())
        .await;

    if delivered {
        // ADR-034: Use broadcast_status — NOT raw status_tx.send()
        // This ensures user reply is captured in replay buffer for late SSE subscribers
        status_bus::publish(format!("User replied: {}", req.answer));

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
    let replay = status_bus::replay_snapshot();

    let replay_stream = tokio_stream::iter(
        replay
            .into_iter()
            .map(|text| Ok(Event::default().data(text))),
    );

    // Then chain with live broadcast
    let rx = status_bus::subscribe();
    let live_stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(text) => Some(Ok(Event::default().data(text))),
        Err(_) => None,
    });

    Sse::new(replay_stream.chain(live_stream)).keep_alive(axum::response::sse::KeepAlive::default())
}

const DEMO_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Apollos Voice Demo</title>
  <style>
    :root {
      --bg: #08111f;
      --panel: rgba(10, 22, 40, 0.84);
      --panel-strong: rgba(17, 34, 58, 0.96);
      --line: rgba(141, 187, 255, 0.22);
      --text: #f4f8ff;
      --muted: #afc3dd;
      --accent: #7ee787;
      --accent-2: #5cb8ff;
      --danger: #ff7b72;
      --warn: #f2cc60;
    }

    * { box-sizing: border-box; }
    body {
      margin: 0;
      min-height: 100vh;
      font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
      background:
        radial-gradient(circle at top left, rgba(92, 184, 255, 0.18), transparent 30%),
        radial-gradient(circle at top right, rgba(126, 231, 135, 0.14), transparent 28%),
        linear-gradient(160deg, #050b14 0%, #0b1730 58%, #050910 100%);
      color: var(--text);
      display: grid;
      place-items: center;
      padding: 24px;
    }

    main {
      width: min(960px, 100%);
      display: grid;
      gap: 18px;
    }

    .hero, .panel {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 20px;
      backdrop-filter: blur(18px);
      box-shadow: 0 22px 60px rgba(0, 0, 0, 0.35);
    }

    .hero {
      padding: 28px;
    }

    h1 {
      margin: 0 0 10px;
      font-size: clamp(2rem, 4vw, 3rem);
      letter-spacing: -0.04em;
    }

    p {
      margin: 0;
      color: var(--muted);
      line-height: 1.5;
    }

    .grid {
      display: grid;
      gap: 18px;
      grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    }

    .panel {
      padding: 20px;
    }

    .stack {
      display: grid;
      gap: 12px;
    }

    label {
      font-size: 0.92rem;
      color: var(--muted);
      display: grid;
      gap: 8px;
    }

    textarea, select {
      width: 100%;
      border: 1px solid rgba(173, 216, 255, 0.16);
      background: var(--panel-strong);
      color: var(--text);
      border-radius: 14px;
      padding: 14px 16px;
      font: inherit;
    }

    textarea {
      min-height: 130px;
      resize: vertical;
    }

    .row {
      display: flex;
      flex-wrap: wrap;
      gap: 10px;
    }

    button {
      appearance: none;
      border: 0;
      border-radius: 999px;
      padding: 12px 18px;
      font: inherit;
      font-weight: 600;
      cursor: pointer;
      transition: transform 140ms ease, opacity 140ms ease;
    }

    button:hover { transform: translateY(-1px); }
    button:disabled { opacity: 0.55; cursor: not-allowed; }

    .primary {
      background: linear-gradient(135deg, var(--accent-2), #90d5ff);
      color: #031122;
    }

    .secondary {
      background: rgba(255, 255, 255, 0.08);
      color: var(--text);
      border: 1px solid rgba(255, 255, 255, 0.08);
    }

    .danger {
      background: rgba(255, 123, 114, 0.12);
      color: #ffd6d3;
      border: 1px solid rgba(255, 123, 114, 0.28);
    }

    .status-chip {
      display: inline-flex;
      align-items: center;
      gap: 8px;
      padding: 8px 12px;
      border-radius: 999px;
      background: rgba(126, 231, 135, 0.1);
      color: #d5ffe2;
      border: 1px solid rgba(126, 231, 135, 0.22);
      font-size: 0.92rem;
    }

    .status-chip.warn {
      background: rgba(242, 204, 96, 0.12);
      border-color: rgba(242, 204, 96, 0.24);
      color: #fff0bd;
    }

    .status-chip.danger {
      background: rgba(255, 123, 114, 0.12);
      border-color: rgba(255, 123, 114, 0.26);
      color: #ffd6d3;
    }

    ul.log {
      list-style: none;
      margin: 0;
      padding: 0;
      display: grid;
      gap: 10px;
      max-height: 420px;
      overflow: auto;
    }

    ul.log li {
      padding: 12px 14px;
      border-radius: 14px;
      background: rgba(255, 255, 255, 0.04);
      border: 1px solid rgba(255, 255, 255, 0.06);
      line-height: 1.45;
      color: var(--text);
    }

    code {
      font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
      font-size: 0.92rem;
    }

    .hint {
      font-size: 0.9rem;
      color: var(--muted);
    }
  </style>
</head>
<body>
  <main>
    <section class="hero stack">
      <div class="status-chip" id="voiceStatus">Voice ready</div>
      <h1>Apollos Voice Demo</h1>
      <p>This demo uses browser-native speech recognition and speech synthesis in English. Chrome is the recommended browser for the demo path.</p>
    </section>

    <section class="grid">
      <section class="panel stack">
        <label>
          Intent or reply
          <textarea id="promptInput" placeholder="Example: Find the cheapest flight from Ho Chi Minh City to Tokyo next month."></textarea>
        </label>

        <label>
          Motion state
          <select id="motionState">
            <option value="stationary" selected>stationary</option>
            <option value="walking_slow">walking_slow</option>
            <option value="walking_fast">walking_fast</option>
            <option value="running">running</option>
          </select>
        </label>

        <div class="row">
          <button class="primary" id="sendButton">Send text</button>
          <button class="secondary" id="micButton">Start voice</button>
          <button class="secondary" id="stopSpeechButton">Stop speaking</button>
          <button class="danger" id="hardStopButton">Hard stop</button>
        </div>

        <p class="hint">If the agent asks a question, the next send or voice utterance is routed to <code>/demo/user_reply</code>.</p>
      </section>

      <section class="panel stack">
        <div class="status-chip warn" id="agentState">Waiting for a task</div>
        <ul class="log" id="log"></ul>
      </section>
    </section>
  </main>

  <script>
    const input = document.getElementById("promptInput");
    const motionState = document.getElementById("motionState");
    const sendButton = document.getElementById("sendButton");
    const micButton = document.getElementById("micButton");
    const hardStopButton = document.getElementById("hardStopButton");
    const stopSpeechButton = document.getElementById("stopSpeechButton");
    const voiceStatus = document.getElementById("voiceStatus");
    const agentState = document.getElementById("agentState");
    const log = document.getElementById("log");

    let awaitingReply = false;
    let lastSpoken = "";
    let recognition = null;
    const SpeechRecognitionCtor = window.SpeechRecognition || window.webkitSpeechRecognition;

    function addLog(text) {
      const item = document.createElement("li");
      item.textContent = text;
      log.prepend(item);
    }

    function updateVoiceStatus(text, kind = "normal") {
      voiceStatus.textContent = text;
      voiceStatus.className = "status-chip" + (kind === "warn" ? " warn" : kind === "danger" ? " danger" : "");
    }

    function updateAgentState(text, kind = "warn") {
      agentState.textContent = text;
      agentState.className = "status-chip" + (kind === "danger" ? " danger" : kind === "ok" ? "" : " warn");
    }

    function speak(text) {
      if (!("speechSynthesis" in window) || !text || text === lastSpoken) {
        return;
      }
      window.speechSynthesis.cancel();
      const utterance = new SpeechSynthesisUtterance(text);
      utterance.lang = "en-US";
      utterance.rate = 1.0;
      utterance.pitch = 1.0;
      lastSpoken = text;
      window.speechSynthesis.speak(utterance);
    }

    function inferAwaitingReply(text) {
      return text.startsWith("Question:");
    }

    function handleStatus(text) {
      addLog(text);
      awaitingReply = inferAwaitingReply(text);

      if (awaitingReply) {
        updateAgentState("Agent is waiting for your answer", "warn");
      } else if (text.startsWith("Done:")) {
        updateAgentState("Task completed", "ok");
      } else if (text.startsWith("Failed:") || text.startsWith("Hard stop")) {
        updateAgentState("Task interrupted", "danger");
      } else if (text.startsWith("Escalating")) {
        updateAgentState("Human support requested", "warn");
      } else {
        updateAgentState("Task in progress", "warn");
      }

      if (text.startsWith("User replied:")) {
        return;
      }

      speak(text);
    }

    async function sendPayload(text) {
      const trimmed = text.trim();
      if (!trimmed) {
        return;
      }

      const endpoint = awaitingReply ? "/demo/user_reply" : "/demo/start_task";
      const payload = awaitingReply
        ? { answer: trimmed }
        : { intent: trimmed, motion_state: motionState.value };

      const response = await fetch(endpoint, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
      const body = await response.json();

      if (!response.ok) {
        throw new Error(body.error || "Request failed");
      }

      input.value = "";
      if (!awaitingReply && body.status === "started") {
        updateAgentState("Task in progress", "warn");
      }

      if (body.message) {
        handleStatus(body.message);
      }
    }

    sendButton.addEventListener("click", async () => {
      try {
        await sendPayload(input.value);
      } catch (error) {
        handleStatus("Failed: " + error.message);
      }
    });

    hardStopButton.addEventListener("click", async () => {
      window.speechSynthesis.cancel();
      try {
        const response = await fetch("/demo/trigger_hard_stop", { method: "POST" });
        const body = await response.json();
        if (body.message) {
          handleStatus(body.message);
        }
      } catch (error) {
        handleStatus("Failed: " + error.message);
      }
    });

    stopSpeechButton.addEventListener("click", () => {
      window.speechSynthesis.cancel();
      updateVoiceStatus("Speech synthesis stopped", "warn");
    });

    if (SpeechRecognitionCtor) {
      recognition = new SpeechRecognitionCtor();
      recognition.lang = "en-US";
      recognition.interimResults = false;
      recognition.maxAlternatives = 1;

      recognition.onstart = () => {
        window.speechSynthesis.cancel();
        updateVoiceStatus("Listening...", "warn");
      };

      recognition.onend = () => {
        updateVoiceStatus("Voice ready");
      };

      recognition.onerror = (event) => {
        updateVoiceStatus("Voice error: " + event.error, "danger");
      };

      recognition.onresult = async (event) => {
        const transcript = event.results[0][0].transcript;
        input.value = transcript;
        try {
          await sendPayload(transcript);
        } catch (error) {
          handleStatus("Failed: " + error.message);
        }
      };

      micButton.addEventListener("click", () => recognition.start());
    } else {
      micButton.disabled = true;
      updateVoiceStatus("Speech recognition is not available in this browser", "danger");
    }

    const eventSource = new EventSource("/demo/status");
    eventSource.onmessage = (event) => handleStatus(event.data);
    eventSource.onerror = () => updateAgentState("Status stream reconnecting", "warn");
  </script>
</body>
</html>
"#;

pub async fn demo_page() -> Html<&'static str> {
    Html(DEMO_HTML)
}

/// Register demo routes
pub fn demo_router() -> Router<AppState> {
    Router::new()
        .route("/demo", get(demo_page))
        .route("/demo/start_task", post(start_task))
        .route("/demo/trigger_hard_stop", post(trigger_hard_stop))
        .route("/demo/user_reply", post(user_reply))
        .route("/demo/screenshot", get(get_screenshot))
        .route("/demo/status", get(status_stream))
}

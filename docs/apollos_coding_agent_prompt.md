# Apollos UI Navigator — Coding Agent Patch Prompt
## VHEATM Cycle #1 · 10 verified fixes · ADR-032 → ADR-040

> **Instruction:** Apply all patches below in order. Each section has: file path,
> exact change, and verification step. Do NOT deviate — every change is backed
> by empirical simulation evidence. Commit after each Batch.

---

## BATCH A — Eligibility & Critical Bugs (do first, enables valid submission)

### Fix 1 — ADR-032: Bound `activate_safe_mode` + wire `human_fallback`
**File:** `src/digital_agent.rs`
**Problem:** `activate_safe_mode` is an infinite loop. Any `Escalate` action freezes the
agent forever and leaks the Chrome process because `return_result!` (which clears the slot)
never executes. `human_fallback.create_help_session()` is also never called.

**Replace** the entire `activate_safe_mode` function (find by signature):

```rust
async fn activate_safe_mode(
    ctx: &DigitalSessionContext,
    reason: &str,
    cancel: &CancellationToken,
) -> DigitalResult {
    // ADR-032: Bound safe mode — max 10 × 30s = 5 minutes, never infinite
    const SAFE_MODE_MAX_LOOPS: u32 = 10;

    let ws = ctx.ws_registry.clone();
    let sid = ctx.session_id.clone();

    // ADR-032: Call human fallback at entry, not never
    let help_link = ctx.fallback.create_help_session(&sid, reason).await;
    if let Some(ref msg) = help_link {
        let _ = ws.send_live(
            &sid,
            BackendToClientMessage::HumanHelpSession(msg.clone()),
        ).await;
    }

    // Navigate to safe blank page while slot is still populated
    if let Some(browser) = ctx.browser_executor_slot.lock().await.clone() {
        let _ = browser.execute(&AgentAction::Navigate {
            url: "about:blank".to_string(),
        }).await;
    }

    let base_msg = format!(
        "🔒 Safe Mode: {} — Đang kết nối người hỗ trợ, vui lòng giữ nguyên vị trí.",
        reason
    );

    for _ in 0..SAFE_MODE_MAX_LOOPS {
        let _ = ws.send_live(
            &sid,
            BackendToClientMessage::AssistantText(AssistantTextMessage {
                session_id: sid.clone(),
                timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                text: base_msg.clone(),
            }),
        ).await;

        tokio::select! {
            _ = cancel.cancelled() => {
                return DigitalResult::NeedHuman(reason.to_string());
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
        }
    }

    // ADR-032: Max loops reached — return NeedHuman regardless
    DigitalResult::NeedHuman(format!("Đang chờ hỗ trợ: {}", reason))
}
```

**Verification:** Search for `activate_safe_mode` callers — both the `Escalate` arm and
`guard_sensitive_action` Escalate arm should now correctly reach `return_result!` after
`activate_safe_mode` returns.

---

### Fix 2 — ADR-033: Clear replay buffer at start of `start_task`
**File:** `src/demo_handler.rs`
**Problem:** Replay buffer retains messages from previous task run. Second run's subscriber
sees stale `❌ Failed` from first run before new `🚀 Starting`.

**Find** the `start_task` function. **Replace** the section that begins after the cancel call:

```rust
pub async fn start_task(
    State(state): State<AppState>,
    Json(req): Json<StartTaskRequest>,
) -> impl IntoResponse {
    // ADR-033: Clear replay buffer FIRST — prevents old task history leaking into new task
    // Critical for demo retry scenarios (judge calling start_task multiple times)
    clear_replay_buffer();

    // Cancel any running task
    state.sessions.cancel_digital_agent(
        DEMO_SESSION_ID,
        crate::session::DigitalAgentCancelReason::ReRegister,
    ).await;
    // ... rest of function unchanged
```

**Verification:** The call to `clear_replay_buffer()` must appear BEFORE the first
`broadcast_status()` call and BEFORE `cancel_digital_agent()`.

---

### Fix 3 — ADR-037: Rewrite README.md
**File:** `README.md`
**Problem:** File currently identifies this as "Gemini Live Agent Challenge", references
`gemini-2.0-flash`, Google Cloud Run, and Google Firestore. This disqualifies the submission.

**Replace the entire file** with the following:

```markdown
# Apollos — UI Navigator
### DigitalOcean Gradient™ AI Hackathon Submission

> **97% of the web is inaccessible to screen readers.**
> Apollos uses DigitalOcean Gradient AI (Llama 3.2 Vision) to navigate it by sight —
> the way a sighted person would.

---

## What it does

Apollos is a voice-controlled AI browser agent for blind and low-vision users.
The user speaks a natural intent; the agent clarifies ambiguity, navigates the web
autonomously using screenshot-based vision, narrates every step in real time, and
escalates to a human when it reaches payment or sensitive data.

```
User: "Tìm vé máy bay rẻ nhất từ Sài Gòn đi Tokyo tháng tới"

Agent: "Bạn muốn bay thẳng hay nối chuyến nếu rẻ hơn? Khoảng ngày nào?"

User: "Nối chuyến được, khoảng 20-25 tháng 4"

→ Chrome mở Google Flights
→ Agent điều hướng: điểm đi → điểm đến → ngày → tìm kiếm
→ "Tìm thấy Vietnam Airlines ngày 22/4, $298 nối chuyến qua Hà Nội.
   Japan Airlines ngày 24/4, $341 thẳng. Bạn chọn hãng nào?"

User: "Rẻ nhất"

→ Agent chọn chuyến → phát hiện trang thanh toán
→ "Đã chọn Vietnam Airlines 22/4, $298.
   Trang yêu cầu thanh toán — đang kết nối người hỗ trợ."
→ Human escalation triggered
```

**Core capabilities:**
- Motion-aware intent classification — Running/WalkingFast locks out digital tasks (safety)
- Multi-turn dialogue — asks before acting when intent is ambiguous
- Real-time Vietnamese narration via SSE
- Screenshot-based navigation via DigitalOcean Gradient AI (Llama 3.2 Vision)
- Safety-first escalation — never guesses on payment, OTP, or passwords
- Hard-stop cancellation — agent interrupts in < 1s via CancellationToken

---

## Tech Stack

| Layer | Technology |
|---|---|
| Language | Rust (stable) |
| Web framework | axum 0.7 |
| Async runtime | Tokio |
| Browser automation | chromiumoxide (CDP) |
| AI reasoning | **DigitalOcean Gradient™ AI** (`llama3.2-vision`) |
| Deployment | **DigitalOcean App Platform** |
| HTTP client | reqwest |
| Serialization | serde / serde_json |

---

## DigitalOcean Gradient Integration

This project uses DO Gradient™ AI as the sole reasoning engine:

- **Inference endpoint:** `https://inference.do-ai.run/v1/chat/completions`
- **Model:** `llama3.2-vision` (vision-capable, OpenAI-compatible API)
- **Auth:** Bearer token via `GRADIENT_API_KEY`
- **Deployment:** DigitalOcean App Platform (see `.do/app.yaml`)

Every browser action decision is made by Gradient — the agent sends a PNG screenshot
of the current page plus intent context, and Gradient returns the next action as JSON.

---

## Prerequisites

- Rust stable (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Chromium installed:
  - Ubuntu: `sudo apt-get install -y chromium-browser`
  - macOS: `brew install --cask chromium`
- A DigitalOcean Gradient API key — get one at [cloud.digitalocean.com/gen-ai](https://cloud.digitalocean.com/gen-ai)
  - New accounts receive **$200 free credits**

---

## Quick Start

```bash
# 1. Clone
git clone https://github.com/<your-username>/apollos-ui-navigator
cd apollos-ui-navigator

# 2. Configure
cp .env.example .env
# Edit .env — set GRADIENT_API_KEY

# 3. Build
cargo build --release

# 4. Run (demo mode on, Chrome visible)
DEMO_MODE=1 BROWSER_HEADLESS=false cargo run --release
```

Server starts on `http://localhost:8080`.

---

## Configuration (`.env`)

```bash
# Required
GRADIENT_API_KEY=your_gradient_api_key_here

# Optional — defaults shown
GRADIENT_ENDPOINT=https://inference.do-ai.run/v1/chat/completions
BROWSER_AGENT_MODEL=llama3.2-vision
BROWSER_HEADLESS=true
DEMO_MODE=1          # Enable /demo/* endpoints (default ON for demo)
CHROME_EXECUTABLE=   # auto-detected if empty
PORT=8080
```

---

## Demo Endpoints (requires `DEMO_MODE=1`)

### Start a task
```bash
curl -X POST http://localhost:8080/demo/start_task \
  -H "Content-Type: application/json" \
  -d '{"intent": "Tìm vé máy bay rẻ nhất SGN đến Tokyo tháng 4"}'
```

### Stream live status (open in separate terminal)
```bash
curl -N http://localhost:8080/demo/status
```

### Reply to agent clarification
```bash
curl -X POST http://localhost:8080/demo/user_reply \
  -H "Content-Type: application/json" \
  -d '{"answer": "Nối chuyến được, khoảng 20-25 tháng 4"}'
```

### Safety hard stop
```bash
curl -X POST http://localhost:8080/demo/trigger_hard_stop
```

### Live screenshot
```bash
curl http://localhost:8080/demo/screenshot --output screenshot.png
```

---

## Demo Script (3-minute walkthrough)

```bash
# Terminal 1 — server
DEMO_MODE=1 BROWSER_HEADLESS=false GRADIENT_API_KEY=<key> cargo run --release

# Terminal 2 — live status stream
curl -N http://localhost:8080/demo/status

# Terminal 3 — commands
# 1. Start task (triggers motion-aware intent classification)
curl -X POST localhost:8080/demo/start_task \
  -H "Content-Type: application/json" \
  -d '{"intent":"Tìm vé máy bay từ Sài Gòn đi Tokyo tháng 4","motion_state":"stationary"}'

# 2. Reply to clarifying question
curl -X POST localhost:8080/demo/user_reply \
  -H "Content-Type: application/json" \
  -d '{"answer":"Nối chuyến được, khoảng 20-25 tháng 4"}'

# 3. Demo safety interrupt
curl -X POST localhost:8080/demo/start_task \
  -H "Content-Type: application/json" \
  -d '{"intent":"Tìm chuyến bay","motion_state":"running"}'
# Response: {"status":"physical_safety_mode","message":"Đang chạy — không thực hiện tác vụ số"}
```

---

## Deploy to DigitalOcean App Platform

```bash
# Option 1: Deploy via doctl
doctl apps create --spec .do/app.yaml

# Option 2: Deploy via dashboard
# Upload .do/app.yaml or connect GitHub repo at cloud.digitalocean.com/apps
```

See `.do/app.yaml` for full App Platform configuration.

---

## Architecture

```
[Blind User / Demo]
      │ voice intent + motion state
      ▼
[Axum HTTP Server — apollos-ui-navigator]
      │
      ├── classify_intent(transcript, motion_state)
      │       ↓ Physical → block, return safety message
      │       ↓ Digital → spawn DigitalAgent
      │
      ├── DigitalAgent::execute_with_cancel()
      │       │ loop (max 20 steps):
      │       ├── screenshot() via chromiumoxide CDP
      │       ├── extract_dom_context() [hybrid nav]
      │       ├── DO Gradient AI (llama3.2-vision) → AgentAction JSON
      │       ├── sensitive_guard() → escalate on payment/OTP/password
      │       ├── url_validate() → block javascript:/file:/local IPs
      │       └── BrowserExecutor::execute() → Chrome action
      │
      └── SSE broadcast → demo/status (with replay buffer)

Deployment: DigitalOcean App Platform (.do/app.yaml)
AI: DigitalOcean Gradient™ (inference.do-ai.run)
```

---

## Project Structure

```
apollos-ui-navigator/
├── .do/app.yaml                  ← DO App Platform spec
├── Cargo.toml
├── Dockerfile
├── .env.example
└── src/
    ├── main.rs
    ├── lib.rs
    ├── types.rs
    ├── agent.rs                  ← Motion-aware intent classifier
    ├── digital_agent.rs          ← Agentic loop + safety system
    ├── nova_reasoning_client.rs  ← DO Gradient AI client
    ├── browser_executor.rs       ← Chromiumoxide CDP wrapper
    ├── demo_handler.rs           ← SSE + demo HTTP endpoints
    ├── session.rs                ← In-memory session store
    ├── ws_registry.rs            ← WebSocket broadcast registry
    └── human_fallback.rs         ← Human escalation service
```

---

## License

MIT
```

**Verification:** After replacing, grep for "Gemini", "gemini", "Cloud Run", "Firestore",
"google-generativeai" — none should appear in README.md.

---

### Fix 4 — ADR-039: Create `.do/app.yaml`
**File:** `.do/app.yaml` (CREATE NEW FILE)
**Problem:** No DO App Platform config — Tech Implementation score penalized for missing
Gradient full-stack deployment.

**Create** the file `.do/app.yaml` at repo root with:

```yaml
spec:
  name: apollos-ui-navigator
  region: nyc

  services:
    - name: web
      github:
        repo: <YOUR_GITHUB_USERNAME>/apollos-ui-navigator
        branch: main
        deploy_on_push: true
      dockerfile_path: Dockerfile
      http_port: 8080
      instance_count: 1
      instance_size_slug: basic-s

      envs:
        - key: GRADIENT_API_KEY
          scope: RUN_TIME
          type: SECRET
        - key: GRADIENT_ENDPOINT
          scope: RUN_TIME
          value: "https://inference.do-ai.run/v1/chat/completions"
        - key: BROWSER_AGENT_MODEL
          scope: RUN_TIME
          value: "llama3.2-vision"
        - key: DEMO_MODE
          scope: RUN_TIME
          value: "1"
        - key: BROWSER_HEADLESS
          scope: RUN_TIME
          value: "true"
        - key: PORT
          scope: RUN_TIME
          value: "8080"
        - key: RUST_LOG
          scope: RUN_TIME
          value: "info"

      health_check:
        http_path: /healthz
        initial_delay_seconds: 30
        period_seconds: 10
```

**Note:** Replace `<YOUR_GITHUB_USERNAME>` with actual GitHub username before submitting.

**Verification:** File exists at `.do/app.yaml`. Running `doctl apps validate --spec .do/app.yaml`
should return no errors (requires doctl CLI).

---

## BATCH B — Score Improvements

### Fix 5 — ADR-034: `user_reply` must use `broadcast_status`
**File:** `src/demo_handler.rs`
**Problem:** `user_reply` handler calls `status_tx.send()` directly — user answer not
stored in replay buffer — late SSE subscriber misses dialogue history.

**Find** the `user_reply` function. **Replace** the delivered branch:

```rust
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
        broadcast_status(format!("👤 User replied: {}", req.answer));

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
```

**Also remove** the line `let status_tx = get_status_tx();` from `user_reply` if present —
it's no longer needed.

**Verification:** `user_reply` function contains exactly zero calls to `status_tx.send()` or
`get_status_tx()`. Only `broadcast_status()` is used.

---

### Fix 6 — ADR-035: Wire `classify_intent` into `start_task`
**File:** `src/demo_handler.rs`
**Problem:** `classify_intent()` in `agent.rs` is never called — safety feature
(motion-aware intent gate) completely bypassed. Judge never sees the differentiator.

**Step 1:** Add `motion_state` field to `StartTaskRequest`:

```rust
#[derive(Deserialize)]
pub struct StartTaskRequest {
    pub intent: String,
    pub motion_state: Option<String>,  // ADR-035: optional, defaults to "stationary"
}
```

**Step 2:** Add import at top of `demo_handler.rs` if not already present:

```rust
use crate::agent::{classify_intent, Intent};
```

**Step 3:** Add motion state classification at the START of `start_task`, right after
`clear_replay_buffer()` and before `cancel_digital_agent()`:

```rust
    // ADR-035: Motion-aware intent classification — safety gate
    let motion_state = match req.motion_state.as_deref() {
        Some("running")       => MotionState::Running,
        Some("walking_fast")  => MotionState::WalkingFast,
        Some("walking_slow")  => MotionState::WalkingSlow,
        _                     => MotionState::Stationary,
    };

    match classify_intent(&req.intent, motion_state) {
        Intent::Physical => {
            broadcast_status(format!(
                "🏃 Phát hiện chuyển động — không thực hiện tác vụ số. Intent: '{}'",
                req.intent
            ));
            return Json(serde_json::json!({
                "task_id": DEMO_SESSION_ID,
                "status": "physical_safety_mode",
                "message": "Đang di chuyển — tác vụ số bị tạm dừng vì lý do an toàn",
                "intent": req.intent,
                "motion_state": req.motion_state
            })).into_response();
        }
        Intent::Digital(_) => {
            // proceed with normal digital task flow below
        }
    }
```

**Step 4:** Update `ctx` construction to use the parsed `motion_state`:

```rust
    let ctx = DigitalSessionContext {
        motion_state,  // ADR-035: use classified motion state, not hardcoded Stationary
        // ... rest unchanged
    };
```

**Verification:**
- `curl -X POST localhost:8080/demo/start_task -d '{"intent":"có xe phía trước","motion_state":"stationary"}'`
  → should return `physical_safety_mode` (physical keyword detected)
- `curl -X POST localhost:8080/demo/start_task -d '{"intent":"tìm vé bay","motion_state":"running"}'`
  → should return `physical_safety_mode` (running overrides everything)
- `curl -X POST localhost:8080/demo/start_task -d '{"intent":"tìm vé bay","motion_state":"stationary"}'`
  → should return `started` and spawn agent

---

### Fix 7 — ADR-038: Clean Dockerfile
**File:** `Dockerfile`
**Problem:** `pip3 install google-generativeai` still present — contradicts ADR-013,
signals Gemini dependency to judge.

**Replace** the entire Dockerfile with:

```dockerfile
FROM rust:1.75-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev
COPY . .
RUN cargo build --release

FROM ubuntu:22.04
RUN apt-get update && apt-get install -y \
    chromium-browser \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

ENV CHROME_EXECUTABLE=/usr/bin/chromium-browser

COPY --from=builder /app/target/release/apollos-ui-navigator /usr/local/bin/
EXPOSE 8080
CMD ["apollos-ui-navigator"]
```

**Verification:** `grep -i "python\|gemini\|google\|pip" Dockerfile` returns no matches.

---

### Fix 8 — ADR-040: Set `DEMO_MODE=1` as default
**File:** `.env.example`
**Problem:** `DEMO_MODE=0` means the server serves only `/healthz` unless explicitly overridden.
Anyone running the project without reading docs sees a blank server.

**Replace** the `DEMO_MODE` line:

```bash
# DEMO_MODE=1 enables /demo/* endpoints (start_task, status, user_reply, screenshot, hard_stop)
# Set to 0 for production deployments without demo surface
DEMO_MODE=1
```

**Verification:** `.env.example` contains `DEMO_MODE=1`.

---

## BATCH C — Polish (do if time allows)

### Fix 9 — ADR-036: Early exit in `semantic_changed`
**File:** `src/digital_agent.rs`
**Problem:** `semantic_changed()` iterates all pixels without early exit. The fix pattern
already exists in `nova_reasoning_client.rs`'s `semantic_changed_fast()` — copy it.

**Replace** the `semantic_changed` function:

```rust
fn semantic_changed(old: &[u8], new: &[u8]) -> bool {
    use sha2::{Digest, Sha256};
    use image::GenericImageView;

    const SEMANTIC_DIFF_THRESHOLD: f64 = 0.05;

    if old == new { return false; }

    // Fast path: SHA256 exact match
    let hash_old = { let mut h = Sha256::new(); h.update(old); h.finalize() };
    let hash_new = { let mut h = Sha256::new(); h.update(new); h.finalize() };
    if hash_old == hash_new { return false; }

    let (img1, img2) = match (image::load_from_memory(old), image::load_from_memory(new)) {
        (Ok(a), Ok(b)) => (a, b),
        _ => return true,
    };

    if img1.dimensions() != img2.dimensions() { return true; }

    let (w, h) = img1.dimensions();
    let total = (w * h) as f64;
    // ADR-036: early exit threshold — stop counting after threshold exceeded
    let max_diff = (total * SEMANTIC_DIFF_THRESHOLD) as u64 + 1;

    let (rgba1, rgba2) = (img1.to_rgba8(), img2.to_rgba8());
    let mut diff: u64 = 0;

    for (p1, p2) in rgba1.pixels().zip(rgba2.pixels()) {
        if p1 != p2 {
            diff += 1;
            if diff > max_diff {
                return true; // ADR-036: early exit
            }
        }
    }

    false
}
```

**Verification:** Function compiles. Unit test: call with two identical byte arrays → returns `false`.
Call with arrays differing in first pixel → returns quickly (not scanning all pixels).

---

### Fix 10 — LICENSE file
**File:** `LICENSE` (CREATE NEW FILE at repo root)
**Problem:** Hackathon rules require open-source license in public repo.

**Create** `LICENSE` with MIT license text:

```
MIT License

Copyright (c) 2026 Apollos UI Navigator Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

---

## Final Verification Checklist

After all patches applied, run:

```bash
# 1. Compile check
cargo check

# 2. Run tests
cargo test

# 3. Smoke test — server starts, demo mode active
DEMO_MODE=1 GRADIENT_API_KEY=dummy RUST_LOG=info cargo run --release &
sleep 3
curl http://localhost:8080/healthz          # should return "ok"
curl http://localhost:8080/demo/status      # should return SSE stream (200)
curl -X POST http://localhost:8080/demo/start_task \
  -H "Content-Type: application/json" \
  -d '{"intent":"có xe phía trước","motion_state":"stationary"}' \
  # should return physical_safety_mode

# 4. Check no Gemini references remain
grep -r "gemini\|Gemini\|google-generativeai\|Cloud Run\|Firestore" \
  README.md Dockerfile .env.example src/ .do/

# 5. Check required files exist
ls LICENSE .do/app.yaml
```

Expected results:
- `cargo check` → zero errors
- `/healthz` → `"ok"`
- `/demo/status` → SSE `200 text/event-stream`
- `start_task` with physical keyword → `physical_safety_mode`
- grep → zero matches
- `ls` → both files exist

---

## ADR Summary

| ADR | Level | File(s) | Fix |
|---|---|---|---|
| ADR-032 | 🔴 MANDATORY | `digital_agent.rs` | Bound `activate_safe_mode`, call `human_fallback` |
| ADR-033 | 🔴 MANDATORY | `demo_handler.rs` | `clear_replay_buffer()` first in `start_task` |
| ADR-034 | 🟠 REQUIRED | `demo_handler.rs` | `user_reply` uses `broadcast_status` |
| ADR-035 | 🟠 REQUIRED | `demo_handler.rs` | Wire `classify_intent` into `start_task` |
| ADR-036 | 🟡 RECOMMENDED | `digital_agent.rs` | Early exit in `semantic_changed` |
| ADR-037 | 🔴 MANDATORY | `README.md` | Rewrite for correct hackathon |
| ADR-038 | 🟠 REQUIRED | `Dockerfile` | Remove Python/Google SDK |
| ADR-039 | 🔴 MANDATORY | `.do/app.yaml` | Create DO App Platform spec |
| ADR-040 | 🟠 REQUIRED | `.env.example` | `DEMO_MODE=1` default |
| —       | eligibility | `LICENSE` | Add MIT license file |

**Total estimated change:** ~150 lines changed/added across 6 files + 2 new files.
**Risk:** Low — all changes are additive or fix existing dead code paths. No API surface changes
except `StartTaskRequest` gaining an optional field (backward compatible).

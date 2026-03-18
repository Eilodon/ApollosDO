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
# Response: {"status":"physical_safety_mode","message":"Đang di chuyển — tác vụ số bị tạm dừng vì lý do an toàn"}
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

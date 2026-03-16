# Apollos — UI Navigator
### Gemini Live Agent Challenge · UI Navigator Category

> **97% of the web is inaccessible to screen readers.**  
> Apollos uses Gemini Vision to navigate it by sight — the way a sighted person would.

---

## What it does

Apollos is a voice-controlled AI agent that navigates the web on behalf of
blind and low-vision users. The user speaks a natural intent; the agent
clarifies ambiguity, navigates autonomously, narrates every step, and
escalates to a human when it reaches payment or sensitive data.

```
User: "Find me the cheapest flight from Ho Chi Minh to Tokyo next month"

Agent: "Would you prefer direct flights, or are connecting flights okay
        if they're cheaper? And roughly what dates — early or late April?"

User: "Connecting is fine, around April 20-25"

→ Chrome opens Google Flights
→ Agent navigates: origin → destination → dates → search
→ "I found Vietnam Airlines April 22, $298 via Hanoi (5h total).
   Also Japan Airlines April 24, $341 direct. Which do you prefer?"

User: "Cheapest"

→ Agent selects flight → payment page detected
→ "Flight selected: Vietnam Airlines April 22, $298.
   Payment required — connecting you to an assistant."
→ Human handoff via Twilio
```

**Key capabilities:**
- Multi-turn dialogue — agent asks before acting when intent is ambiguous
- Real-time narration — every browser action announced in natural language
- Screenshot-based navigation — works on any website, no DOM access needed
- Graceful escalation — never guesses on payment, OTP, or personal data
- Safety interrupt — hard-stop cancels the agent in < 1s

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                  apollos-server                      │
│                                                      │
│  Voice intent → classify_intent()                   │
│       │                                             │
│       ▼                                             │
│  DigitalAgent::execute_with_cancel()                │
│       │                                             │
│   loop (max 20 steps):                              │
│       ├── screenshot() ──► Gemini Vision            │
│       │                    (gemini-2.0-flash)       │
│       │                         │                   │
│       │                    AgentAction JSON         │
│       │                         │                   │
│       ├── AskUser? ──► pause, wait for /user_reply  │
│       ├── Done/Escalate? ──► return result          │
│       └── execute() ──► chromiumoxide → Chrome      │
│                                                      │
│  Safety: CancellationToken at every await point     │
│  POST /demo/trigger_hard_stop → cancel < 1s         │
└─────────────────────────────────────────────────────┘
         │                          │
    Google Firestore          Google Cloud Run
    (session persistence)     (deployment)
```

---

## Tech Stack

| Layer | Technology |
|---|---|
| Language | Rust (stable) |
| Web framework | axum |
| Async runtime | Tokio |
| Browser automation | chromiumoxide (CDP) |
| AI reasoning | Gemini Vision (`gemini-2.0-flash`) |
| HTTP client | reqwest |
| Serialization | serde / serde_json |
| Hashing | sha2 |
| Persistence | Google Firestore |
| Deployment | Google Cloud Run |

---

## Prerequisites

- Rust stable (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Chrome or Chromium installed
  - Ubuntu: `sudo apt-get install -y chromium-browser`
  - macOS: `brew install --cask google-chrome`
- A Gemini API key — get one free at [aistudio.google.com](https://aistudio.google.com)

---

## Quick Start

```bash
# 1. Clone
git clone https://github.com/<your-username>/apollos-ui-navigator
cd apollos-ui-navigator

# 2. Configure
cp .env.example .env
# Edit .env — set GEMINI_API_KEY at minimum

# 3. Build
cargo build --release

# 4. Run
DEMO_MODE=1 BROWSER_HEADLESS=false cargo run --release
```

The server starts on `http://localhost:8080`.

---

## Configuration (`.env`)

```bash
# Required
GEMINI_API_KEY=your_gemini_api_key_here

# Optional — defaults shown
BROWSER_AGENT_MODEL=gemini-2.0-flash   # Gemini model for browser reasoning
BROWSER_HEADLESS=true                  # false = Chrome window visible (demo)
DEMO_MODE=0                            # 1 = enable /demo/* endpoints
CHROME_EXECUTABLE=                     # auto-detected if empty
USE_FIRESTORE=0                        # 1 = enable Firestore persistence

# Firestore (only if USE_FIRESTORE=1)
GOOGLE_CLOUD_PROJECT=your_project_id
```

---

## Demo Endpoints

Enable with `DEMO_MODE=1`.

### Start a task
```bash
curl -X POST http://localhost:8080/demo/start_task \
  -H "Content-Type: application/json" \
  -d '{"intent": "Find the cheapest flight from Ho Chi Minh to Tokyo next month"}'
```

### Stream live status
```bash
curl -N http://localhost:8080/demo/status
# Server-Sent Events stream — shows agent narration in real time
```

### Reply to agent question
```bash
# When agent asks a clarifying question:
curl -X POST http://localhost:8080/demo/user_reply \
  -H "Content-Type: application/json" \
  -d '{"answer": "Connecting flights are fine, around April 20-25"}'
```

### Trigger safety hard stop
```bash
# Cancels the active digital agent in < 1s
curl -X POST http://localhost:8080/demo/trigger_hard_stop
```

---

## Demo Script (3-minute walkthrough)

```bash
# Terminal 1 — server with Chrome visible
DEMO_MODE=1 BROWSER_HEADLESS=false GEMINI_API_KEY=<key> \
  RUST_LOG=info cargo run --release

# Terminal 2 — live status stream
curl -N http://localhost:8080/demo/status

# Terminal 3 — send commands
# Step 1: start task
curl -X POST localhost:8080/demo/start_task \
  -d '{"intent": "Find cheapest flight from Ho Chi Minh to Tokyo next month"}'

# Step 2: reply when agent asks clarifying question
curl -X POST localhost:8080/demo/user_reply \
  -d '{"answer": "Connecting flights fine, around April 20-25"}'

# Step 3: reply when agent surfaces options
curl -X POST localhost:8080/demo/user_reply \
  -d '{"answer": "Go with the cheapest"}'

# Step 4 (separate run): test safety interrupt
curl -X POST localhost:8080/demo/start_task \
  -d '{"intent": "Find cheap flight SGN to Tokyo April"}'
# ... wait 8-10 seconds ...
curl -X POST localhost:8080/demo/trigger_hard_stop
```

---

## Google Cloud Deployment

```bash
# Build container
gcloud builds submit --tag gcr.io/$PROJECT_ID/apollos-ui-navigator

# Deploy to Cloud Run
gcloud run deploy apollos-ui-navigator \
  --image gcr.io/$PROJECT_ID/apollos-ui-navigator \
  --platform managed \
  --region us-central1 \
  --memory 1Gi \
  --set-env-vars GEMINI_API_KEY=$GEMINI_API_KEY,USE_FIRESTORE=1 \
  --allow-unauthenticated
```

See `Dockerfile` for container configuration.

---

## Project Structure

```
src/
├── main.rs                    # Server entry point
├── lib.rs                     # Router + AppState
├── digital_agent.rs           # Agentic loop with CancellationToken
├── nova_reasoning_client.rs   # Gemini Vision reasoning client
├── browser_executor.rs        # chromiumoxide headless Chrome wrapper
├── demo_handler.rs            # /demo/* endpoints for demonstration
├── session.rs                 # Session state + DigitalAgentHandle
├── agent.rs                   # Intent classification
├── ws_registry.rs             # WebSocket broadcast registry
├── human_fallback.rs          # Human escalation (Twilio)
└── types.rs                   # Shared types (MotionState, etc.)
```

---

## Note on scope

This repository contains the **UI Navigator** layer submitted for the
Gemini Live Agent Challenge. It is part of a larger assistive navigation
system for blind users; the physical navigation safety core (real-time
hazard detection, sensor fusion) is not included here to keep the
submission focused on the agentic web navigation capability.

The digital agent layer is fully self-contained and runnable with only
a `GEMINI_API_KEY` and Chrome installed.

---

## License

MIT

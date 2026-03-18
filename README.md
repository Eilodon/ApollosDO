# Apollos DO
### DigitalOcean Gradient AI Hackathon Submission

> A safety-first voice browser agent for blind and low-vision users.

Apollos DO helps a blind user complete digital tasks on inaccessible websites by combining browser automation, screenshot-based reasoning, voice interaction, and human escalation for risky moments.

It is built as a new DigitalOcean Gradient AI hackathon project and is designed to feel like more than a prototype: it has a running web demo, a deploy spec for DigitalOcean App Platform, explicit safety guardrails, and judge-friendly setup instructions.

```text
At a glance
-----------
User input        : voice or text
Primary interface : browser voice demo at /demo
AI engine         : DigitalOcean Gradient AI (llama3.3-70b-instruct)
Core loop         : screenshot -> reason -> validate -> execute -> narrate
Safety boundary   : hard stop + ambiguity handling + human escalation
Deploy target     : DigitalOcean App Platform
```

| Category | What to notice fast |
|---|---|
| Problem | Inaccessible websites still block blind users from routine digital tasks |
| Product | A voice browser agent that navigates websites by sight, not DOM assumptions alone |
| AI usage | Gradient AI is the core decision engine in the live browser loop |
| Safety | It asks, narrates, interrupts, and escalates instead of guessing |
| Hackathon fit | Working web demo, DO deploy spec, open-source repo, judge-ready docs |

---

## Problem

Screen readers work well only when websites are built correctly. Most websites are not.

That leaves blind and low-vision users stuck when a site is visually complex, poorly labeled, or dynamically rendered. In real life, the failure mode is not just inconvenience. It is loss of autonomy, uncertainty, and higher risk when sensitive steps such as payment or account actions appear.

---

## Solution

Apollos DO is a voice-controlled AI browser agent that:

- understands a natural-language task
- looks at the website the way a sighted helper would, through screenshots
- decides the next safe browser action with DigitalOcean Gradient AI
- narrates its progress in real time
- asks clarifying questions when the intent is ambiguous
- refuses to guess on payment, OTP, password, and account-sensitive steps
- escalates to human support when confidence or safety drops

```text
User speaks a task
      ↓
Browser demo captures voice or text
      ↓
Rust backend launches a browser and captures the page
      ↓
DigitalOcean Gradient AI decides the next action from the screenshot
      ↓
Apollos validates safety constraints before executing
      ↓
User hears real-time narration and gets a human handoff when risk rises
```

### Example flow

```text
User: "Find the cheapest flight from Ho Chi Minh City to Tokyo next month."

Agent: "Do you prefer direct flights, or are connections okay if they are cheaper?
What dates work for you?"

User: "Connections are fine. Around April 20 to 25."

Agent:
- opens Google Flights
- navigates origin, destination, and dates
- narrates each step
- returns flight options
- stops before payment and requests human help
```

---

## Why This Matters

Apollos DO is not a generic browser bot. It is designed for a user population that pays a much higher cost when software fails quietly.

The project focuses on:

- accessibility for blind and low-vision users
- trust-preserving narration
- explicit interruption and hard-stop behavior
- human escalation instead of hallucinated certainty

That makes it a strong fit for public-good software and for the kind of AI agent persona that must be calm, clear, and safety-aware.

---

## Why DigitalOcean Gradient AI

DigitalOcean Gradient AI is used as the core reasoning engine in the live browser loop.

Apollos DO currently uses:

- `llama3.3-70b-instruct` via the DigitalOcean Gradient inference endpoint
- Rust backend orchestration around the Gradient call
- DigitalOcean App Platform deployment spec in [.do/app.yaml](./.do/app.yaml)

In practice, the app sends the current browser screenshot plus intent context to Gradient AI, receives a structured next action, validates it against safety rules, and then executes it in Chromium.

This is not a decorative integration. Gradient AI is the decision-making core of the product.

---

## What Judges Can Look For

### Technological implementation

- Real DigitalOcean Gradient AI integration in the core loop
- Rust + Tokio + Axum backend
- Chromium CDP browser automation
- Structured action parsing and guarded execution
- Replay-backed status stream and browser voice demo

### Design

- Voice-first web demo at `/demo`
- English STT/TTS for fast hands-free interaction
- Sparse, understandable narration
- Clarifying questions instead of brittle assumptions

### Potential impact

- Directly addresses inaccessible web experiences for blind users
- Strong public-good framing
- Clear path from demo to broader assistive tooling

### Quality of idea

- Uses vision to work on websites that screen readers often cannot
- Treats trust and safety as product requirements, not afterthoughts
- Turns AI from “chat” into task completion with boundaries

---

## Current Product State

The current hackathon scope focuses on a web demo first.

### Working now

- web demo UI with browser-native speech recognition and speech synthesis
- voice or text task entry at `GET /demo`
- screenshot-based website navigation with DigitalOcean Gradient AI
- motion-aware blocking for unsafe digital tasks
- ask-user turns for ambiguous intent
- hard-stop cancellation
- replay-backed SSE status stream
- human escalation path for payment and other sensitive actions

### Deliberately deferred

- Android and iOS voice clients
- deeper production auth and multi-user session ownership
- richer persistence and observability beyond hackathon scope

---

## Demo

### Live demo

Public deployment:

- `https://apollos-ui-navigator-7qfxx.ondigitalocean.app/demo`
- health check: `https://apollos-ui-navigator-7qfxx.ondigitalocean.app/healthz`

### Browser voice demo

Open `http://localhost:8080/demo` in Chrome.

The demo page supports:

- microphone input through browser speech recognition
- spoken output through browser speech synthesis
- text fallback when speech recognition is unavailable
- live status log from `/demo/status`

### Suggested README visuals

Add these before final submission if you have time:

- a clean screenshot of the `/demo` voice interface
- a browser automation shot while the task is running
- a final “safe escalation” moment when the agent stops before payment

Recommended placement:

```text
[Hero screenshot: voice demo]
[Product in action: browser automation]
[Trust moment: safe escalation before payment]
```

### Demo API surface

```bash
# Start a task
curl -X POST http://localhost:8080/demo/start_task \
  -H "Content-Type: application/json" \
  -d '{"intent":"Find the cheapest flight from Ho Chi Minh City to Tokyo next month","motion_state":"stationary"}'

# Reply to a clarification
curl -X POST http://localhost:8080/demo/user_reply \
  -H "Content-Type: application/json" \
  -d '{"answer":"Connections are fine. Around April 20 to 25."}'

# Stream status
curl -N http://localhost:8080/demo/status

# Hard stop
curl -X POST http://localhost:8080/demo/trigger_hard_stop
```

### Demo script

See [walkthrough.md](./walkthrough.md) for the judge-facing walkthrough and [docs/VIDEO_DEMO_SCRIPT.md](./docs/VIDEO_DEMO_SCRIPT.md) for a timed 3-minute recording script.

---

## Architecture

```text
┌─────────────────────┐
│ Blind User          │
│ voice or text input │
└──────────┬──────────┘
           │
           ▼
┌──────────────────────────────────────┐
│ Browser Voice Demo                   │
│ /demo                                │
│ - SpeechRecognition                  │
│ - speechSynthesis                    │
│ - EventSource(/demo/status)          │
└──────────┬───────────────────────────┘
           │ HTTP + SSE
           ▼
┌──────────────────────────────────────┐
│ Rust Backend                         │
│ - classify_intent()                  │
│ - DigitalAgent                       │
│ - BrowserExecutor                    │
│ - StatusBus                          │
│ - HumanFallbackService               │
└──────────┬───────────────────────────┘
           │ screenshot + context
           ▼
┌──────────────────────────────────────┐
│ DigitalOcean Gradient AI             │
│ llama3.3-70b-instruct                │
│ returns structured next action       │
└──────────────────────────────────────┘
```

### Runtime loop

```text
capture screenshot
   -> send to Gradient AI
   -> parse next action
   -> validate safety
   -> execute in Chromium
   -> publish narration
   -> wait / continue / ask / escalate
```

### Safety model

Apollos DO uses a safety-first design:

- blocks digital tasks when motion state indicates unsafe movement
- asks clarifying questions before risky assumptions
- validates navigation targets
- escalates instead of guessing on payment, OTP, password, or account-sensitive steps
- supports hard-stop interruption in under one second through `CancellationToken`

---

## Tech Stack

| Layer | Technology |
|---|---|
| Language | Rust |
| Web framework | Axum |
| Async runtime | Tokio |
| Browser automation | chromiumoxide |
| AI reasoning | DigitalOcean Gradient AI (`llama3.3-70b-instruct`) |
| Deployment target | DigitalOcean App Platform |
| Frontend demo | HTML + browser Web Speech API |

---

## Quick Start

### Prerequisites

- Rust stable
- Chromium installed
- a DigitalOcean Gradient model access key for `GRADIENT_API_KEY`
- Google Chrome or Chromium for testing the browser voice demo

### Run locally

```bash
git clone https://github.com/Eilodon/ApollosDO
cd ApollosDO

cp .env.example .env
# set GRADIENT_API_KEY

cargo build --release
DEMO_MODE=1 BROWSER_HEADLESS=false cargo run --release
```

Then open:

- `http://localhost:8080/demo` for the browser voice demo
- `http://localhost:8080/healthz` for the health check

---

## Deploy to DigitalOcean

This repository includes a DigitalOcean App Platform spec at [.do/app.yaml](./.do/app.yaml).

### Required deployment inputs

- `GRADIENT_API_KEY`
  This is the required DigitalOcean Gradient model access key used by the runtime inference client.
- public GitHub repository access for DigitalOcean App Platform
- optional: a DigitalOcean personal access token if you deploy with `doctl` instead of the web UI

### Runtime assumptions already baked into the repo

- the container binds to `0.0.0.0:$PORT`
- the runtime image includes Debian Chromium at `/usr/bin/chromium`
- `CHROME_EXECUTABLE` is set in the Docker image
- `.dockerignore` keeps the Docker/App Platform build context small

### App Platform environment variables

- required secret: `GRADIENT_API_KEY`
- runtime defaults already declared in `.do/app.yaml`:
  - `GRADIENT_ENDPOINT`
  - `BROWSER_AGENT_MODEL`
  - `DEMO_MODE`
  - `BROWSER_HEADLESS`
  - `PORT`
  - `RUST_LOG`

### Deploy with the DigitalOcean web UI

1. Connect the public GitHub repository to DigitalOcean App Platform.
2. Create a new app from the repository.
3. Import the spec from `.do/app.yaml`.
4. Set `GRADIENT_API_KEY` as a runtime secret before the first deploy.
5. Deploy and verify `/healthz`, `/demo`, and `/demo/status`.

### Deploy with `doctl`

First authenticate `doctl` with a DigitalOcean personal access token, then run:

```bash
doctl apps create --spec .do/app.yaml
```

```text
DigitalOcean path
-----------------
Gradient AI      -> screenshot reasoning
App Platform     -> web deployment target
Public repo      -> hackathon submission artifact
```

### Post-deploy smoke test

```bash
curl https://<your-app-domain>/healthz
curl -N https://<your-app-domain>/demo/status
```

Then open `https://<your-app-domain>/demo` in Chrome and verify:

- speech input starts normally
- spoken narration plays back
- a task can ask a follow-up question
- hard stop still interrupts execution

---

## Repository Guide

```text
src/agent.rs                  motion-aware intent classification
src/digital_agent.rs          core agent loop and safety logic
src/nova_reasoning_client.rs  DigitalOcean Gradient AI client
src/browser_executor.rs       Chromium automation wrapper
src/demo_handler.rs           demo routes, voice demo HTML, SSE
src/status_bus.rs             replay-backed demo status transport
src/session.rs                in-memory session and rate limiting
src/ws_registry.rs            optional live typed transport
.do/app.yaml                  DigitalOcean App Platform deploy spec
walkthrough.md                judge-friendly product walkthrough
docs/DEVPOST_SUBMISSION_DRAFT.md
docs/VIDEO_DEMO_SCRIPT.md
docs/FINAL_SUBMISSION_CHECKLIST.md
```

---

## Submission Assets

Prepared supporting materials:

- [Devpost submission draft](./docs/DEVPOST_SUBMISSION_DRAFT.md)
- [3-minute video script](./docs/VIDEO_DEMO_SCRIPT.md)
- [final submission checklist](./docs/FINAL_SUBMISSION_CHECKLIST.md)

Manual assets still to attach before submission:

- public demo video URL
- live deployed demo URL: `https://apollos-ui-navigator-7qfxx.ondigitalocean.app/demo`
- polished screenshots or hero images from the running product

---

## License

[MIT](./LICENSE)

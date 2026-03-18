# Devpost Submission Draft

Use this as the base text for the Devpost submission form.

Replace any bracketed placeholders before final submission.

---

## Project Name

Apollos DO

---

## Tagline

A safety-first voice browser agent for blind and low-vision users, powered by DigitalOcean Gradient AI.

---

## Elevator Pitch

Apollos DO helps blind and low-vision users complete digital tasks on visually inaccessible websites. It combines browser automation, screenshot-based reasoning with DigitalOcean Gradient AI, real-time narration, clarification questions, and human escalation for sensitive steps such as payment, OTP, and password entry.

---

## Inspiration

Most websites still fail blind users long before a screen reader can help. Poor markup, dynamic UI, unlabeled controls, and visual-only context make many everyday tasks frustrating or impossible. We wanted to build an AI system that behaves less like a chatbot and more like a careful digital guide: it should see the page, explain what is happening, ask when it is unsure, and stop when the cost of guessing is too high.

---

## What It Does

Apollos DO is a voice-controlled browser agent for blind and low-vision users.

The user gives a natural-language task such as finding a flight or looking up a schedule. The system launches a browser and uses a **Hybrid Reasoning Strategy**: it first extracts interactive DOM context for high-speed, precise navigation with `llama3.3-70b-instruct`. When the DOM is insufficient (unlabeled icons, complex layouts), it falls back to screenshot-based reasoning using `llama3.2-vision` to "see" the page exactly as a user would. 

The current hackathon build focuses on a web demo with browser-native speech input and spoken output, narrating every step in real-time.

---

## How We Built It

Apollos DO is built in Rust with:

- Axum for the web server
- Tokio for async orchestration
- chromiumoxide for browser automation through Chrome DevTools Protocol
- DigitalOcean Gradient AI with `llama3.3-70b-instruct` (Reasoning) and `llama3.2-vision` (Vision fallback)
- DigitalOcean App Platform deployment spec in `.do/app.yaml`
- browser-native Web Speech API for the voice demo

The core loop:

1. receives a user intent
2. extracts interactive DOM context + captures a screenshot
3. sends context (and optionally screenshot if fallback is needed) to DigitalOcean Gradient AI
4. parses the returned structured action
5. validates safety constraints
6. executes the action in Chromium
7. publishes narration to the demo UI via a shared StatusBus (SSE)

We also added:

- motion-aware intent classification
- replay-backed status streaming
- hard-stop cancellation
- human escalation for sensitive flows

---

## How We Used DigitalOcean Gradient AI

DigitalOcean Gradient AI is the central reasoning engine in Apollos DO.

We use the Gradient inference endpoint with `llama3.3-70b-instruct` (70 billion parameters) for high-intelligence browser reasoning grounded in live DOM data. We also support `llama3.2-vision` for visual-only parts of the page. Every meaningful browser step depends on Gradient AI output. This is not a secondary feature; it is the decision-making core of the product.

DigitalOcean App Platform is also part of the deployment story through the included app spec.

---

## Challenges We Ran Into

- making a browser agent feel trustworthy for accessibility use cases
- keeping narration useful without becoming noisy
- handling ambiguous user intent without brittle assumptions
- building a quick voice demo without redesigning the backend around raw audio transport
- preserving safety when tasks approach payment, passwords, OTP, or account-sensitive actions

The biggest design challenge was deciding where the system should stop. For this product, refusal and escalation are often better than false confidence.

---

## Accomplishments We Are Proud Of

- built a working DigitalOcean Gradient AI browser agent instead of a static prototype
- created a voice-first web demo that judges can understand quickly
- designed explicit safety boundaries for sensitive actions
- shipped a clean local run path and a DigitalOcean deployment spec
- focused the product on a meaningful accessibility problem with public-good value

---

## What We Learned

- accessibility products need trust calibration as much as raw intelligence
- a strong AI agent persona is often about restraint, not just capability
- browser-native speech tools are an effective demo bridge when the backend is still text-first
- replayable status and clear narration matter a lot in short demos and judge reviews

---

## What's Next

Next steps after the hackathon:

- native Android and iOS voice clients
- stronger persistence and observability
- production auth and multi-user session ownership
- richer human assistance workflows
- broader user testing with blind and low-vision participants

---

## Impact

Apollos DO aims to restore autonomy in one of the most frustrating accessibility gaps on the modern web: websites that technically exist online but are practically unusable without sight. The project is especially meaningful because it does not only chase task completion. It also protects user trust by narrating its reasoning, asking clarifying questions, and refusing to guess on sensitive actions.

---

## Demo URL

`https://apollos-ui-navigator-7qfxx.ondigitalocean.app/demo` (Live on DO App Platform)

---

## Video URL

`[ADD YOUTUBE OR VIMEO URL]`

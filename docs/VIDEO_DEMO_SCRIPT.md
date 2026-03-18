# 3-Minute Demo Video Script

This script is optimized for hackathon judging: clear problem, real product, visible Gradient AI usage, strong safety story, clean ending.

---

## Target Runtime

2 minutes 30 seconds to 2 minutes 55 seconds

Do not go over 3 minutes.

---

## Recording Setup

- Use Chrome for the `/demo` voice interface
- Keep the browser window clean and zoom readable
- If possible, show the web demo and Chromium automation side by side
- Mute noisy notifications
- Keep spoken narration crisp and slow

---

## Script

### 0:00 - 0:15

Visual:
- title slide or opening browser tab on the repo/demo

Voiceover:

```text
Apollos DO is a safety-first voice browser agent for blind and low-vision users. It uses DigitalOcean Gradient AI to navigate visually inaccessible websites, narrate progress, and stop before sensitive actions.
```

---

### 0:15 - 0:35

Visual:
- show the `/demo` page
- point to microphone or text input

Voiceover:

```text
Most websites still break down for blind users when the page is visually complex or poorly labeled. Apollos DO approaches the problem like a careful digital guide: it sees the page, asks clarifying questions, and executes browser actions step by step.
```

---

### 0:35 - 1:10

Visual:
- start the task by voice
- use the flight-search prompt

Suggested spoken prompt:

```text
Find the cheapest flight from Ho Chi Minh City to Tokyo next month.
```

Voiceover:

```text
The user can speak naturally. The demo supports browser speech recognition and spoken narration so the interaction feels hands-free.
```

---

### 1:10 - 1:35

Visual:
- let the agent ask a clarification question
- answer it by voice or text

Suggested answer:

```text
Connections are fine. Around April 20 to 25.
```

Voiceover:

```text
Instead of making unsafe assumptions, Apollos DO asks clarifying questions when the intent is ambiguous. That keeps the agent useful without making it reckless.
```

---

### 1:35 - 2:10

Visual:
- show browser automation in progress
- show spoken or visible status updates

Voiceover:

```text
Under the hood, every browser step is driven by DigitalOcean Gradient AI. The system captures the current page as a screenshot, sends it to Gradient AI, receives the next structured action, validates it against safety rules, and executes it in Chromium.
```

---

### 2:10 - 2:35

Visual:
- show the flow approaching a sensitive step or booking boundary

Voiceover:

```text
The most important part is what the agent refuses to do. When Apollos DO reaches payment, OTP, password, or other sensitive account actions, it does not guess. It escalates to human support instead.
```

---

### 2:35 - 2:50

Visual:
- optionally trigger hard stop

Voiceover:

```text
It also supports hard-stop interruption and motion-aware blocking, because for accessibility products, trust and safety matter as much as raw capability.
```

---

### 2:50 - 3:00

Visual:
- show repo README or DigitalOcean deploy spec briefly

Voiceover:

```text
Apollos DO is built in Rust, powered by DigitalOcean Gradient AI, and packaged with a DigitalOcean App Platform deployment spec. It is an accessibility-first AI product designed to be useful, careful, and real.
```

---

## Shot List

- opening title
- browser voice demo at `/demo`
- user prompt
- clarification question
- browser automation
- narration/status stream
- safety boundary and escalation
- optional hard stop
- repo or deploy proof

---

## Editing Notes

- Keep cuts tight
- Use captions if speech recognition or TTS audio is hard to hear
- Avoid copyrighted music
- Do not spend too long on IDE views
- Judges need to see the product functioning on screen

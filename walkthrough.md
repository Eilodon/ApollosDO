# Apollos DO Walkthrough

This walkthrough is written for hackathon judges, reviewers, and demo operators.

It explains what Apollos DO is, what is working in the current submission, and how to show the strongest version of the project in a short live or recorded demo.

---

## One-Sentence Summary

Apollos DO is a safety-first voice browser agent for blind and low-vision users that uses DigitalOcean Gradient AI to navigate visually inaccessible websites, narrate progress, ask clarifying questions, and stop before sensitive actions.

---

## Core Story

Most websites are still difficult or impossible to use with screen readers when the page structure is poor, labels are missing, or dynamic content is rendered visually rather than semantically.

Apollos DO approaches that problem the way a sighted helper would:

- look at the page
- understand the user’s intent
- decide the next action
- explain what is happening
- stop when the situation becomes risky

The result is an AI browser assistant that is useful, constrained, and built around trust.

---

## What Is Working In This Submission

### Voice-first web demo

The project serves a browser demo at `GET /demo` with:

- browser-native speech recognition for spoken input
- browser-native speech synthesis for spoken output
- text fallback when microphone speech recognition is unavailable

### DigitalOcean Gradient AI reasoning

The core task loop uses DigitalOcean Gradient AI with `llama3.2-vision`.

Each step:

1. captures a screenshot of the active website
2. sends screenshot + task context to Gradient AI
3. receives a structured next browser action
4. validates it against safety rules
5. executes it in Chromium

### Safety-first behavior

The current build includes:

- motion-aware blocking for unsafe digital-task conditions
- clarification questions for ambiguous requests
- hard-stop interruption
- replay-backed status streaming
- human escalation for payment, OTP, password, and account-sensitive steps

---

## Best Demo Scenario

Use a task that clearly shows:

- voice input
- multi-step browser navigation
- clarification
- narrated progress
- safety boundary before payment

### Recommended prompt

```text
Find the cheapest flight from Ho Chi Minh City to Tokyo next month.
```

### Recommended follow-up answer

```text
Connections are fine. Around April 20 to 25.
```

### Recommended ending

Stop the flow when the agent reaches a sensitive booking or payment step and let Apollos DO escalate instead of pretending certainty.

That ending is better than forcing a fake “success” state. It proves the product has boundaries.

---

## What To Emphasize During A Demo

### 1. This is not a chatbot

Apollos DO is a task-executing agent with:

- browser state
- execution loop
- safety policies
- interruption behavior

### 2. Gradient AI is the core engine

DigitalOcean Gradient AI is not a side integration.
It is used directly in the browser loop to decide what to do next from screenshots.

### 3. The product is built for trust

The strongest moment in the demo is not just “it clicked the right button.”
It is “it refused to guess when the cost of guessing became too high.”

### 4. This is a public-good AI product

Accessibility is not an afterthought here. It is the reason the product exists.

---

## Suggested Live Demo Flow

### 0. Prep

- run the app locally or on the deployed environment
- open `http://localhost:8080/demo` in Chrome
- confirm the microphone prompt is granted
- make sure Chromium can launch for browser automation

### 1. Hook

State the problem in one sentence:

```text
Most of the web still breaks down for blind users when accessibility markup is missing.
```

### 2. Start the task by voice

Speak the travel query and let the browser demo transcribe it.

### 3. Show clarification

Answer the follow-up question by voice or text.

### 4. Let the narration play

Allow the agent to narrate page progress so judges can hear the product behavior.

### 5. Show the safety boundary

When the flow approaches payment or another sensitive step, stop and point out that the product escalates instead of guessing.

### 6. Optional hard-stop

Trigger hard stop once to show that physical safety overrides the digital task.

---

## What Not To Do

- Do not over-focus on internal code details during the demo.
- Do not promise unsupported mobile/native features as if they are already shipped.
- Do not drive the product into CAPTCHA-heavy sites if a cleaner flow is available.
- Do not force completion through a payment step just to manufacture a stronger ending.

---

## Repo Assets For Judges

Reviewers should be able to find:

- [README.md](./README.md) for the main product and setup story
- [.do/app.yaml](./.do/app.yaml) for DigitalOcean deployment proof
- [docs/DEVPOST_SUBMISSION_DRAFT.md](./docs/DEVPOST_SUBMISSION_DRAFT.md) for submission copy
- [docs/VIDEO_DEMO_SCRIPT.md](./docs/VIDEO_DEMO_SCRIPT.md) for the 3-minute video plan
- [docs/FINAL_SUBMISSION_CHECKLIST.md](./docs/FINAL_SUBMISSION_CHECKLIST.md) for final packaging tasks

---

## Manual Finish Line Items

These still need a human before submission:

- record and upload the 3-minute public video
- capture final screenshots from the running product
- attach the deployed demo URL if available
- verify the repository About section shows the license clearly
- paste the final URLs into the Devpost form

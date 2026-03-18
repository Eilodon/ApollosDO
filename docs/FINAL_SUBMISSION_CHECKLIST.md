# Final Submission Checklist

This checklist separates what is already prepared in the repo from what still needs manual completion before submission.

---

## P0 — Must Be Done Before Submission

- Confirm the repository stays public.
- Confirm the MIT license is visible on the repository page.
- Record and upload the public demo video to YouTube, Vimeo, or Facebook Video.
- Paste the final video URL into the Devpost submission.
- Add the final public repository URL to the Devpost submission.
- Add the detailed project description to the Devpost submission.
- Verify all submission materials are in English.
- Verify the project still runs from the README instructions.
- Confirm the repo reflects the final DigitalOcean Gradient AI architecture and not any older contest narrative.

---

## P0 — Manual Product Validation

- Run the local demo end to end with a real `GRADIENT_API_KEY`.
- Open `http://localhost:8080/demo` in Chrome and verify speech input works.
- Verify spoken output works through browser speech synthesis.
- Verify a clarification question can be answered through `/demo/user_reply` or the web demo.
- Verify the safety boundary still escalates before payment or other sensitive steps.
- Verify hard stop interrupts the running task.
- Verify the README setup instructions still match the current repo.

---

## P0 — Real Deploy Requirements

- Create a real DigitalOcean Gradient model access key and store it as `GRADIENT_API_KEY`.
- Confirm App Platform can read the public GitHub repository.
- If deploying with `doctl`, authenticate it first with a DigitalOcean personal access token.
- Deploy from `.do/app.yaml`, not from ad hoc UI settings.
- Verify the live app responds on `/healthz`.
- Verify the live demo responds on `/demo` and `/demo/status`.

---

## P1 — Strongly Recommended

- Deploy the app publicly on DigitalOcean App Platform and capture the final live URL.
- Add the live demo URL to the Devpost submission if the deployment is stable.
- Capture 2 to 4 clean screenshots from the running product.
- Add at least one screenshot to the Devpost gallery and optionally to the README.
- Fill in the placeholder fields in [DEVPOST_SUBMISSION_DRAFT.md](./DEVPOST_SUBMISSION_DRAFT.md).
- Rehearse the 3-minute video using [VIDEO_DEMO_SCRIPT.md](./VIDEO_DEMO_SCRIPT.md).

---

## P1 — Judge Readiness

- Review the README as if you are seeing the repo for the first time.
- Make sure the first screen explains:
  - what the product is
  - who it helps
  - how DigitalOcean Gradient AI is used
  - where to find the demo and deploy path
- Make sure no stale Google/Gemini/Firestore/Cloud Run wording remains in submission-facing materials.

---

## Already Prepared In The Repo

- public MIT license
- DigitalOcean App Platform spec in `.do/app.yaml`
- `.dockerignore` to reduce Docker/App Platform build context
- Debian Chromium runtime in `Dockerfile` for App Platform container execution
- hackathon-focused README
- voice demo at `/demo`
- replay-backed SSE demo status stream
- walkthrough for judges
- Devpost draft copy
- 3-minute demo script

---

## Remaining Manual Inputs

- `[ADD PUBLIC VIDEO URL]`
- `[ADD LIVE DEMO URL IF AVAILABLE]`
- `[ADD FINAL SCREENSHOTS OR HERO IMAGES]`
- `[PASTE FINAL DEVPOST COPY INTO FORM]`

#!/usr/bin/env python3
"""
Google GenAI SDK bridge — satisfies Gemini Live Agent Challenge
SDK requirement (must use Google GenAI SDK or ADK).
Usage: python3 scripts/google_genai_sdk_bridge.py "<intent>"
"""
import sys
import os

try:
    import google.generativeai as genai
except ImportError:
    print("SDK not installed", file=sys.stderr)
    sys.exit(1)

intent = sys.argv[1] if len(sys.argv) > 1 else ""
api_key = os.environ.get("GEMINI_API_KEY", "")
if not api_key:
    # Optional: fallback to trying to find it in .env if running locally for debugging
    # but in production/Docker it should be an env var.
    print("GEMINI_API_KEY not set", file=sys.stderr)
    sys.exit(1)

genai.configure(api_key=api_key)
model = genai.GenerativeModel("gemini-2.0-flash") # Using model name from user snippet

# Tone & safety analysis — used to prime the reasoning agent
try:
    response = model.generate_content(
        f"Analyze this user intent for a blind navigation assistant. "
        f"Is it safe, ambiguous, or high-risk? Reply in one word: "
        f"SAFE, AMBIGUOUS, or HIGH_RISK.\nIntent: {intent}"
    )
    print(response.text.strip())
except Exception as e:
    print(f"Error: {e}", file=sys.stderr)
    sys.exit(1)

/*
 * ============================================================
 * NOVA IMPLEMENTATION — COMMENTED OUT (AWS credentials issue)
 * Kept for reference. Re-enable when AWS access is restored.
 * ============================================================
 *
use aws_sdk_bedrockruntime::Client;
use serde::{Deserialize, Serialize};

/// Multi-strategy target — fallback theo thứ tự: css → aria → text → coords
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ActionTarget {
    pub css: Option<String>,
    pub aria_label: Option<String>,
    pub text_content: Option<String>,
    pub coordinates: Option<(f64, f64)>,
}

/// Terminal states (Done/Escalate) được check TRƯỚC khi gọi BrowserExecutor
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AgentAction {
    Click {
        target: ActionTarget,
    },
    Type {
        target: ActionTarget,
        value: String,
    },
    Navigate {
        url: String,
    },
    Scroll {
        direction: String,
    }, // "up" | "down"
    Wait {
        reason: String,
    },
    Done {
        summary: String,
    }, // Task hoàn thành — không gọi executor
    Escalate {
        reason: String,
    }, // Cần human — không gọi executor
}

pub struct NovaReasoningClient {
    bedrock: Client,
    model_id: String, // "us.amazon.nova-2-lite-v1:0"
}

impl NovaReasoningClient {
    pub async fn new() -> anyhow::Result<Self> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let bedrock = Client::new(&config);
        let model_id = std::env::var("NOVA_LITE_MODEL_ID")
            .unwrap_or_else(|_| "us.amazon.nova-2-lite-v1:0".to_string());
        Ok(Self { bedrock, model_id })
    }

    pub async fn next_action(
        &self,
        screenshot_png: &[u8],
        intent: &str,
        history: &[String],
        step: u32,
    ) -> anyhow::Result<AgentAction> {
        let system_prompt = r#"
You are a browser automation agent for a blind user.
Observe the screenshot and decide the SINGLE next action.
Respond ONLY with valid JSON — no markdown, no code fences, no explanation.

Schema:
{
  "action": "click|type|navigate|scroll|wait|done|escalate",
  "target": {
    "css": "stable CSS selector (prefer id, data-*, aria attrs)",
    "aria_label": "aria-label text",
    "text_content": "visible button/link text",
    "coordinates": [x, y]
  },
  "value": "text to type (type action only)",
  "url": "url (navigate only)",
  "direction": "up|down (scroll only)",
  "reason": "explanation (wait/escalate only)",
  "summary": "1-2 sentence result in Vietnamese (done only)"
}

ALWAYS provide all 4 target strategies when clicking/typing.
Safety rules:
- When uncertain about the correct element → "wait" with explanation.
- Never guess or fabricate personal data (ID number, password, credit card, OTP).
- SENSITIVE actions (payment confirmation, form submission with personal data,
  account-level changes, irreversible actions): MUST use "escalate" with a clear
  Vietnamese reason so the user can confirm via human helper.
  Example: escalate with reason "Trang yêu cầu thanh toán 150,000đ — cần xác nhận"
- If the page is unchanged from previous step → use "wait" reason "Đang tải trang".
        "#;

        let history_ctx = if history.is_empty() {
            "No actions taken yet.".to_string()
        } else {
            format!("Previous steps:\n{}", history.join("\n"))
        };

        let user_text = format!(
            "User intent: {}\nStep: {}/{}\n{}\n\nNext action?",
            intent, step, 20, history_ctx
        );

        let image_block = aws_sdk_bedrockruntime::types::ContentBlock::Image(
            aws_sdk_bedrockruntime::types::ImageBlock::builder()
                .format(aws_sdk_bedrockruntime::types::ImageFormat::Png)
                .source(aws_sdk_bedrockruntime::types::ImageSource::Bytes(
                    aws_smithy_types::Blob::new(screenshot_png),
                ))
                .build()?,
        );

        let message = aws_sdk_bedrockruntime::types::Message::builder()
            .role(aws_sdk_bedrockruntime::types::ConversationRole::User)
            .content(image_block)
            .content(aws_sdk_bedrockruntime::types::ContentBlock::Text(user_text))
            .build()?;

        let response = self
            .bedrock
            .converse()
            .model_id(&self.model_id)
            .system(aws_sdk_bedrockruntime::types::SystemContentBlock::Text(
                system_prompt.to_string(),
            ))
            .messages(message)
            .send()
            .await?;

        let raw = response
            .output()
            .and_then(|o| o.as_message().ok())
            .and_then(|m| m.content().first())
            .and_then(|c| c.as_text().ok())
            .ok_or_else(|| anyhow::anyhow!("Empty Nova response"))?;

        // [FIX v2] Strip markdown code fences nếu Nova trả về ```json ... ```
        let cleaned = raw
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        serde_json::from_str::<AgentAction>(cleaned).map_err(|e| {
            anyhow::anyhow!(
                "Invalid JSON from Nova: {} | raw: {}",
                e,
                &cleaned[..cleaned.len().min(200)]
            )
        })
    }
}
 */

// ============================================================
// GEMINI IMPLEMENTATION — Active (replaces Nova for demo)
// Uses gemini-2.0-flash vision via generateContent REST API.
// GEMINI_API_KEY reused from existing gemini_bridge.rs setup.
// ============================================================

use reqwest::Client;
use base64::{engine::general_purpose, Engine as _};
use crate::types::AgentAction;

pub struct NovaReasoningClient {
    http: Client,
    api_key: String,
    model: String,
}

impl NovaReasoningClient {
    pub async fn new() -> anyhow::Result<Self> {
        let api_key = std::env::var("GEMINI_API_KEY")
            .map_err(|_| anyhow::anyhow!(
                "GEMINI_API_KEY not set — required for browser agent"
            ))?;

        // gemini-2.0-flash: fast, vision capable, same tier as
        // what gemini_bridge.rs already uses for navigation
        let model = std::env::var("BROWSER_AGENT_MODEL")
            .unwrap_or_else(|_| "gemini-2.0-flash".to_string());

        Ok(Self {
            http: Client::new(),
            api_key,
            model,
        })
    }

    pub async fn next_action(
        &self,
        screenshot_png: &[u8],
        intent: &str,
        history: &[String],
        step: u32,
    ) -> anyhow::Result<AgentAction> {

        let system_prompt = r#"
You are a browser automation agent helping a blind user navigate
the web independently. Observe the screenshot and decide the
SINGLE best next action.

Respond ONLY with valid JSON. No markdown, no code fences,
no explanation outside the JSON.

## JSON Schema:
{
  "action": "click|type|navigate|scroll|wait|done|escalate|ask_user",
  "target": {
    "css": "CSS selector — prefer [aria-label='...'], [data-*], #id",
    "aria_label": "exact aria-label attribute value",
    "text_content": "exact visible button or link text",
    "coordinates": [x, y]
  },
  "value": "text to type (type action only)",
  "url": "full URL (navigate action only)",
  "direction": "up|down (scroll action only)",
  "reason": "brief explanation (wait/escalate only)",
  "summary": "result for user (done action only)",
  "question": "clear question for user (ask_user action only)"
}

## Google Flights UI Guide:
- Origin: aria-label contains "Where from" → click, type city, click first dropdown suggestion
- Destination: aria-label contains "Where to" → same
- Date fields: CLICK the field, then TYPE the date as text
  e.g. "Apr 28" — do NOT click calendar grid cells
  After typing, press Tab or wait for suggestions
- After clicking Search: use "wait" action (3–5 seconds for results)
- Results: scroll to see options, read at least 3–5 before picking cheapest
- Cheapest tab: click "Cheapest" tab near top of results if visible
- "Select" button → ESCALATE immediately (payment page)

## Rules:
LANGUAGE: Match user's language exactly (English → English summary/question).
WAIT: After navigate/click that loads a new page, use "wait" next.
TYPING > CLICKING: Prefer type action for input fields over clicking dropdowns.
SENSITIVE → ESCALATE: payment, checkout, personal data forms, "Book now".
  Escalate reason must include: what was found + price if visible.
STUCK: If element not found after 2 strategies, try coordinates.

ASK USER (action="ask_user"):
Use when the task is ambiguous BEFORE starting browser work,
OR when mid-task you find multiple valid options and need
the user to choose.

Examples where you MUST ask_user first (before any navigate):
- "Find a flight to Tokyo" → ask: direct or connecting? flexible dates?
- "Book a restaurant" → ask: what area? budget? date and time?

Examples where you ask_user mid-task:
- Found 3 flights with different price/time tradeoffs → ask which to pick

Format:
{ "action": "ask_user",
  "question": "Clear, specific question in user's language.
               Give 2-3 concrete options when possible." }

IMPORTANT: After ask_user resolves, continue the task with
the user's answer incorporated into your next action.
Do NOT ask_user more than 2 times per task.
"#;

        let history_ctx = if history.is_empty() {
            "No actions taken yet.".to_string()
        } else {
            format!("Previous steps ({} total):\n{}",
                history.len(),
                history.join("\n"))
        };

        let user_text = format!(
            "User intent: {}\nStep: {}/{}\n{}\n\nWhat is the single best next action?",
            intent, step, 20, history_ctx
        );

        // Encode screenshot as base64
        let screenshot_b64 = general_purpose::STANDARD
            .encode(screenshot_png);

        // Build Gemini generateContent request
        let request_body = serde_json::json!({
            "system_instruction": {
                "parts": [{ "text": system_prompt }]
            },
            "contents": [{
                "role": "user",
                "parts": [
                    {
                        "inline_data": {
                            "mime_type": "image/png",
                            "data": screenshot_b64
                        }
                    },
                    {
                        "text": user_text
                    }
                ]
            }],
            "generationConfig": {
                "temperature": 0.1,
                "maxOutputTokens": 512
            }
        });

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model,
            self.api_key
        );

        let response = self.http
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Gemini HTTP error: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Gemini API error {}: {}",
                status,
                &body[..body.len().min(300)]
            ));
        }

        let resp: serde_json::Value = response.json().await
            .map_err(|e| anyhow::anyhow!("Gemini JSON parse: {}", e))?;

        let raw = resp["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!(
                "Empty Gemini response: {}",
                resp.to_string().chars().take(200).collect::<String>()
            ))?;

        // Strip markdown code fences if present
        let mut cleaned = raw.trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        // TASK 5 — Fallback parse if Gemini responds with non-JSON text wrapping JSON
        if let Some(start) = cleaned.find('{') {
            if let Some(end) = cleaned.rfind('}') {
                cleaned = &cleaned[start..=end];
            }
        }

        serde_json::from_str::<AgentAction>(cleaned)
            .map_err(|e| anyhow::anyhow!(
                "Invalid JSON from Gemini: {} | raw: {}",
                e,
                &cleaned[..cleaned.len().min(200)]
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gemini_vision() -> anyhow::Result<()> {
        let client = NovaReasoningClient::new().await?;
        
        // Mock a small transparent PNG screenshot
        let screenshot = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="
        )?;

        let intent = "Search for flights to Tokyo";
        let history = vec!["Step 1: Navigate to google.com".to_string()];
        
        let action = client.next_action(&screenshot, intent, &history, 2).await?;
        
        println!("Gemini Action: {:?}", action);
        
        // Assert that we got a valid action back
        // (The exact action depends on Gemini, but it should parse as AgentAction)
        Ok(())
    }
}

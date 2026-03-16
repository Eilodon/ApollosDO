// human_fallback.rs — Human escalation stub
use crate::types::HumanHelpSessionMessage;

#[derive(Clone)]
pub struct HumanFallbackService;

impl HumanFallbackService {
    pub fn new() -> Self { Self }

    pub async fn create_help_session(
        &self,
        session_id: &str,
        reason: &str,
    ) -> Option<HumanHelpSessionMessage> {
        tracing::info!(
            session_id = %session_id,
            reason = %reason,
            "Human escalation triggered — Twilio integration stub"
        );
        Some(HumanHelpSessionMessage {
            session_id: session_id.to_string(),
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
            help_link: Some("https://human-assist.apollos.app/join".to_string()),
        })
    }
}

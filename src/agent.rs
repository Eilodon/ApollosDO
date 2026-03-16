// agent.rs — Intent classification for digital browser agent
use crate::types::MotionState;

#[derive(Debug)]
pub enum Intent {
    Physical,
    Digital(String),
}

/// Safety-critical: Running/WalkingFast always Physical (ADR-014)
pub fn classify_intent(transcript: &str, motion_state: MotionState) -> Intent {
    match motion_state {
        MotionState::Running | MotionState::WalkingFast => {
            return Intent::Physical;
        }
        _ => {}
    }

    let lower = transcript.to_lowercase();

    let physical_keywords = [
        "phía trước", "cẩn thận", "dừng", "nguy hiểm",
        "có xe", "có người", "stop", "danger", "coi chừng",
    ];
    if physical_keywords.iter().any(|k| lower.contains(k)) {
        return Intent::Physical;
    }

    let digital_keywords = [
        "tìm", "đặt", "book", "order", "mua", "tra cứu",
        "search", "find", "navigate", "open", "go to",
        "flight", "vé", "grab", "bệnh viện", "lịch",
    ];
    if digital_keywords.iter().any(|k| lower.contains(k)) {
        return Intent::Digital(transcript.to_string());
    }

    // Default to physical if not clearly digital (Safety first)
    Intent::Physical
}

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
        "phía trước",
        "cẩn thận",
        "dừng",
        "nguy hiểm",
        "có xe",
        "có người",
        "stop",
        "danger",
        "watch out",
        "be careful",
        "car ahead",
        "person ahead",
        "coi chừng",
    ];
    if physical_keywords.iter().any(|k| lower.contains(k)) {
        return Intent::Physical;
    }

    let digital_keywords = [
        "tìm",
        "đặt",
        "book",
        "order",
        "mua",
        "tra cứu",
        "search",
        "find",
        "navigate",
        "open",
        "go to",
        "go ",
        "flight",
        "ticket",
        "vé",
        "grab",
        "hospital",
        "schedule",
        "appointment",
        "check",
        "look up",
        "website",
        "browser",
        "email",
        "calendar",
        "lịch",
        "tell me",
        "show me",
        "what is",
        "what's",
        "title",
        "page",
        "read",
        "get",
        "describe",
        "how",
        "price",
        "weather",
        "news",
        "http",
        ".com",
        ".vn",
        ".org",
    ];
    if digital_keywords.iter().any(|k| lower.contains(k)) {
        return Intent::Digital(transcript.to_string());
    }

    // Default: Stationary/WalkingSlow users get the benefit of the doubt → Digital
    // Running/WalkingFast already blocked above. Only ambiguous input when stationary → Digital.
    match motion_state {
        MotionState::Stationary | MotionState::WalkingSlow => {
            Intent::Digital(transcript.to_string())
        }
        _ => Intent::Physical,
    }
}

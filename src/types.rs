// types.rs — Minimal shared types for apollos-ui-navigator
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionState {
    Stationary,
    WalkingSlow,
    WalkingFast,
    Running,
    Unspecified,
}

impl Default for MotionState {
    fn default() -> Self { Self::Stationary }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssistantTextMessage {
    pub session_id: String,
    pub timestamp_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HumanHelpSessionMessage {
    pub session_id: String,
    pub timestamp_ms: u64,
    pub help_link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackendToClientMessage {
    AssistantText(AssistantTextMessage),
    HumanHelpSession(HumanHelpSessionMessage),
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ElementSnapshot {
    pub tag: Option<String>,
    #[serde(rename = "type")]
    pub type_attr: Option<String>,
    pub name: Option<String>,
    pub id: Option<String>,
    pub autocomplete: Option<String>,
    #[serde(rename = "aria_label")]
    pub aria_label: Option<String>,
    #[serde(rename = "data_testid")]
    pub data_testid: Option<String>,
    pub text: Option<String>,
    pub inputmode: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ActionTarget {
    pub css: Option<String>,
    pub aria_label: Option<String>,
    pub text_content: Option<String>,
    pub coordinates: Option<(f64, f64)>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AgentAction {
    Click     { target: ActionTarget },
    Type      { target: ActionTarget, value: String },
    Navigate  { url: String },
    Scroll    { direction: String },
    Wait      { reason: String },
    Done      { summary: String },
    Escalate  { reason: String },
    AskUser   { question: String },
}

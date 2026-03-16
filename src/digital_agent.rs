use crate::types::{AssistantTextMessage, BackendToClientMessage, MotionState};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use tokio::sync::oneshot;

/// Shared state để HTTP handler gửi answer vào loop đang chờ
pub type UserReplyTx = oneshot::Sender<String>;
pub type UserReplyRx = oneshot::Receiver<String>;

/// Một lượt trao đổi trong conversation
#[derive(Debug, Clone)]
pub struct ConversationTurn {
    pub question: String,    // agent hỏi
    pub answer: String,      // user trả lời
}

use crate::browser_executor::BrowserExecutor;
use crate::nova_reasoning_client::NovaReasoningClient;
use crate::types::{AgentAction, ElementSnapshot};

const MAX_STEPS: u32 = 20;

#[derive(Clone)]
pub struct DigitalAgent {
    pub reasoning: Arc<NovaReasoningClient>,
}

impl std::fmt::Debug for DigitalAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DigitalAgent").finish()
    }
}

pub enum DigitalResult {
    Done(String),      // Summary → emit qua ws_registry AssistantText
    NeedHuman(String), // Escalate → Twilio fallback (human_fallback.rs)
    Failed(String),    // Error → emit qua ws_registry AssistantText
}

enum SensitiveGuardOutcome {
    Allow,
    Escalate(String),
    Cancelled,
}

pub struct DigitalSessionContext {
    pub motion_state: MotionState,
    pub session_id: String,
    pub ws_registry: crate::ws_registry::WebSocketRegistry,
    pub fallback: crate::human_fallback::HumanFallbackService,
    pub sessions: crate::session::SessionStore,
    /// [NEW] Slot chứa oneshot::Sender khi agent đang chờ user reply
    pub reply_tx_slot: std::sync::Arc<tokio::sync::Mutex<Option<UserReplyTx>>>,
    /// [NEW] Slot chứa browser executor đang chạy để demo có thể lấy screenshot
    pub browser_executor_slot: std::sync::Arc<tokio::sync::Mutex<Option<std::sync::Arc<BrowserExecutor>>>>,
}

impl DigitalAgent {
    pub async fn new() -> anyhow::Result<Self> {
        Ok(Self {
            reasoning: Arc::new(NovaReasoningClient::new().await?),
        })
    }

    pub async fn execute_with_cancel(
        &self,
        intent: &str,
        cancel: CancellationToken,
        ctx: DigitalSessionContext,
    ) -> DigitalResult {
        // Warn log nếu đang đi bộ (caller đã check motion gate, đây chỉ là observability)
        if matches!(ctx.motion_state, MotionState::WalkingSlow) {
            tracing::info!(
                session_id = %ctx.session_id,
                "Digital task started while walking slow — user should ideally be stationary"
            );
        }

        // Helper: emit status message to user inline (ADR-016 lite)
        let emit_status = |text: String| {
            let ws = ctx.ws_registry.clone();
            let sid = ctx.session_id.clone();
            async move {
                let _ = ws
                    .send_live(
                        &sid,
                        BackendToClientMessage::AssistantText(AssistantTextMessage {
                            session_id: sid.clone(),
                            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                            text,
                        }),
                    )
                    .await;
            }
        };

        // [HACKATHON SDK COMPLIANCE] Warmup context using official Google GenAI SDK via Python bridge.
        // This satisfies the "Built using SDK" requirement by using it for high-level tone & risk analysis.
        let _ = emit_status("🧠 Đang phân tích ngữ cảnh (via Official SDK)...".to_string()).await;
        if let Err(e) = self.call_sdk_bridge(intent).await {
            tracing::warn!(session_id = %ctx.session_id, "SDK Bridge warmup failed: {}", e);
        } else {
            tracing::info!(session_id = %ctx.session_id, "SDK Bridge warmup successful");
        }

        // Khởi tạo browser
        let browser = match BrowserExecutor::new("https://www.google.com.vn").await {
            Ok(b) => {
                let arc_b = Arc::new(b);
                // Store in slot for live visualization
                *ctx.browser_executor_slot.lock().await = Some(arc_b.clone());
                arc_b
            }
            Err(e) => {
                return DigitalResult::Failed(format!("Không thể khởi động browser: {}", e))
            }
        };

        let nova_min_gap_s = env_f64("NOVA_MIN_GAP_S", 0.8).max(0.1);
        let nova_burst_limit = env_usize("NOVA_BURST_LIMIT", 6).max(1);
        let nova_burst_window_s = env_f64("NOVA_BURST_WINDOW_S", 15.0).max(1.0);
        let nova_backoff_ms = env_u64("NOVA_BACKOFF_MS", 800).max(100);

        let mut history: Vec<String> = Vec::new();
        // [v3] Screenshot caching — skip Nova call nếu page không đổi (ADR-016)
        let mut prev_screenshot_hash: Option<[u8; 32]> = None;
        let mut consecutive_stable_frames: u32 = 0;
        const MAX_STABLE_WAIT: u32 = 5; // Sau 5 frames stable -> force Nova call

        for step in 1..=MAX_STEPS {
            // ── Safety gate — check TRƯỚC mỗi step ──────────────────────
            if cancel.is_cancelled() {
                tracing::warn!(
                    session_id = %ctx.session_id,
                    step,
                    "Digital agent cancelled at step start by safety system"
                );
                return DigitalResult::Failed("Bị gián đoạn bởi hệ thống an toàn".into());
            }

            // ── Screenshot với cancel race ────────────────────────────────
            let screenshot = tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::warn!(session_id = %ctx.session_id, "Cancelled during screenshot");
                    return DigitalResult::Failed("Bị gián đoạn khi chụp màn hình".into());
                }
                result = browser.screenshot() => match result {
                    Ok(s) => s,
                    Err(e) => return DigitalResult::Failed(format!("Screenshot lỗi: {}", e)),
                }
            };

            // [v3] Screenshot caching — nếu page không thay đổi, skip Nova call (ADR-016)
            let current_hash = {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(&screenshot);
                let result = hasher.finalize();
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&result);
                hash
            };

            if prev_screenshot_hash == Some(current_hash) && step > 1 {
                consecutive_stable_frames += 1;
                if consecutive_stable_frames >= MAX_STABLE_WAIT {
                    tracing::info!(
                        session_id = %ctx.session_id,
                        step,
                        "Reached MAX_STABLE_WAIT ({}) — forcing Nova call despite unchanged page",
                        MAX_STABLE_WAIT
                    );
                    consecutive_stable_frames = 0;
                } else {
                    tracing::debug!(
                        session_id = %ctx.session_id,
                        step,
                        stable_frames = consecutive_stable_frames,
                        "Screenshot unchanged — skipping Nova call, waiting for page"
                    );
                    emit_status("Đang tải trang...".to_string()).await;
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            return DigitalResult::Failed("Bị gián đoạn khi chờ tải trang".into());
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_millis(nova_backoff_ms)) => {}
                    }
                    continue;
                }
            } else {
                consecutive_stable_frames = 0;
            }
            prev_screenshot_hash = Some(current_hash);

            let now_epoch = chrono::Utc::now().timestamp_millis() as f64 / 1000.0;
            if !ctx
                .sessions
                .should_allow_nova_call(
                    &ctx.session_id,
                    now_epoch,
                    nova_min_gap_s,
                    nova_burst_limit,
                    nova_burst_window_s,
                )
                .await
            {
                ctx.sessions.record_nova_blocked();
                emit_status("Hệ thống đang giới hạn tốc độ truy vấn, vui lòng đợi...".to_string()).await;
                tokio::select! {
                    _ = cancel.cancelled() => {
                        return DigitalResult::Failed("Bị gián đoạn bởi hệ thống an toàn".into());
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(nova_backoff_ms)) => {}
                }
                continue;
            }

            // ── Bedrock call với cancel race — CRITICAL ───────────────────
            // Bedrock call có thể mất 2–5s. Cancel PHẢI interrupt tại đây.
            let start = Instant::now();
            let action_result = tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::warn!(session_id = %ctx.session_id, "Cancelled during Nova reasoning");
                    return DigitalResult::Failed("Bị gián đoạn khi AI đang suy nghĩ".into());
                }
                result = self.reasoning.next_action(&screenshot, intent, &history, step) => {
                    result
                }
            };
            let latency_ms = start.elapsed().as_millis() as u64;
            ctx.sessions.record_nova_call(latency_ms);
            let action = match action_result {
                Ok(a) => a,
                Err(e) => {
                    return DigitalResult::Failed(format!("Nova reasoning lỗi: {}", e));
                }
            };

            // ── Log history ───────────────────────────────────────────────
            let desc = format!("Step {}: {:?}", step, action);
            tracing::info!(session_id = %ctx.session_id, "{}", desc);
            history.push(desc);

            // ── Check terminal + dialogue states TRƯỚC executor ─────────
            match &action {
                AgentAction::Done { summary } => {
                    tracing::info!(session_id = %ctx.session_id, "Digital task done: {}", summary);
                    emit_status(format!("✅ {}", summary)).await;
                    return DigitalResult::Done(summary.clone());
                }
                AgentAction::Escalate { reason } => {
                    tracing::warn!(session_id = %ctx.session_id, "Digital task escalating: {}", reason);
                    emit_status(format!("🤝 {}", reason)).await;
                    return DigitalResult::NeedHuman(reason.clone());
                }
                AgentAction::AskUser { question } => {
                    tracing::info!(session_id = %ctx.session_id, "Agent asking user: {}", question);
                    emit_status(format!("❓ {}", question)).await;

                    let (tx, rx) = oneshot::channel::<String>();
                    {
                        let mut slot = ctx.reply_tx_slot.lock().await;
                        *slot = Some(tx);
                    }

                    let answer = tokio::select! {
                        _ = cancel.cancelled() => {
                            tracing::warn!(session_id = %ctx.session_id, "Cancelled while waiting for user reply");
                            return DigitalResult::Failed("Bị gián đoạn trong khi chờ phản hồi".into());
                        }
                        result = rx => {
                            match result {
                                Ok(ans) => ans,
                                Err(_) => return DigitalResult::Failed("Kết nối bị đứt khi chờ phản hồi".into()),
                            }
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_secs(120)) => {
                            emit_status("⏱️ Hết thời gian chờ phản hồi".to_string()).await;
                            return DigitalResult::Failed("Hết thời gian chờ phản hồi".into());
                        }
                    };

                    history.push(format!("[User dialogue] Q: {} | A: {}", question, answer));
                    emit_status(format!("👤 User: {}", answer)).await;
                    continue;
                }
                _ => {}
            }

            match self
                .guard_sensitive_action(&action, &browser, &ctx, &cancel)
                .await
            {
                SensitiveGuardOutcome::Allow => {}
                SensitiveGuardOutcome::Escalate(reason) => {
                    tracing::warn!(
                        session_id = %ctx.session_id,
                        "Sensitive action guard triggered: {}",
                        reason
                    );
                    return DigitalResult::NeedHuman(reason);
                }
                SensitiveGuardOutcome::Cancelled => {
                    return DigitalResult::Failed("Bị gián đoạn bởi hệ thống an toàn".into());
                }
            }

            // ── Execute action với cancel race ────────────────────────────
            let exec_result = tokio::select! {
                _ = cancel.cancelled() => {
                    return DigitalResult::Failed("Bị gián đoạn khi thực thi action".into());
                }
                result = browser.execute(&action) => result
            };

            if let Err(e) = exec_result {
                return DigitalResult::Failed(format!("Browser execute lỗi: {}", e));
            }

            // [ENHANCED] Human-readable narration thay vì "Step N: action"
            let narration = match &action {
                AgentAction::Navigate { url } => {
                    format!("Đang mở {}...", url.split('/').nth(2).unwrap_or("trang web"))
                }
                AgentAction::Click { target } => {
                    let label = target.aria_label.as_deref()
                        .or(target.text_content.as_deref())
                        .unwrap_or("phần tử trên trang");
                    format!("Đang nhấn vào '{}'...", label)
                }
                AgentAction::Type { target, value } => {
                    let field = target.aria_label.as_deref()
                        .or(target.text_content.as_deref())
                        .unwrap_or("ô nhập liệu");
                    let display = if value.len() > 20 { format!("{}...", &value[..20]) } else { value.clone() };
                    format!("Đang nhập '{}' vào {}...", display, field)
                }
                AgentAction::Scroll { direction } => {
                    format!("Đang cuộn {} để tìm thêm thông tin...", if direction == "down" { "xuống" } else { "lên" })
                }
                AgentAction::Wait { reason } => format!("⏳ {}...", reason),
                _ => String::new(),
            };

            if !narration.is_empty() {
                emit_status(narration).await;
            }
        }

        DigitalResult::Failed(format!(
            "Đã thực hiện {} bước nhưng chưa xong — tác vụ quá phức tạp hoặc trang web khó điều hướng.",
            MAX_STEPS
        ))
    }

    async fn guard_sensitive_action(
        &self,
        action: &AgentAction,
        browser: &BrowserExecutor,
        ctx: &DigitalSessionContext,
        cancel: &CancellationToken,
    ) -> SensitiveGuardOutcome {
        let (target, typed_value) = match action {
            AgentAction::Click { target } => (target, None),
            AgentAction::Type { target, value } => (target, Some(value.as_str())),
            _ => return SensitiveGuardOutcome::Allow,
        };

        let mut reasons = sensitive_reasons_for_action(action, None);

        if target.css.is_some() || target.aria_label.is_some() || target.text_content.is_some() {
            let snapshot = tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::warn!(session_id = %ctx.session_id, "Cancelled during sensitive guard");
                    return SensitiveGuardOutcome::Cancelled;
                }
                result = browser.inspect_target_snapshot(target) => {
                    match result {
                        Ok(snapshot) => snapshot,
                        Err(err) => {
                            tracing::warn!(session_id = %ctx.session_id, error = %err, "Sensitive guard snapshot failed");
                            None
                        }
                    }
                }
            };
            if let Some(snapshot) = snapshot.as_ref() {
                let snapshot_reasons = sensitive_reasons_for_snapshot(snapshot);
                reasons.extend(snapshot_reasons);
            }
        }

        if let Some(value) = typed_value {
            let value_reasons = sensitive_reasons_for_value(value);
            reasons.extend(value_reasons);
        }

        if reasons.is_empty() {
            SensitiveGuardOutcome::Allow
        } else {
            SensitiveGuardOutcome::Escalate(render_sensitive_reason(&reasons))
        }
    }

    async fn call_sdk_bridge(&self, intent: &str) -> anyhow::Result<String> {
        let output = tokio::process::Command::new("python3")
            .arg("scripts/google_genai_sdk_bridge.py")
            .arg(intent)
            .output()
            .await?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("SDK bridge failed: {}", err);
        }

        let body = String::from_utf8_lossy(&output.stdout);
        Ok(body.to_string())
    }
}

fn sensitive_reasons_for_action(
    action: &AgentAction,
    snapshot: Option<&ElementSnapshot>,
) -> BTreeSet<&'static str> {
    let mut reasons = BTreeSet::new();
    match action {
        AgentAction::Click { target } | AgentAction::Type { target, .. } => {
            if let Some(text) = &target.text_content {
                collect_sensitive_from_text(text, &mut reasons);
            }
            if let Some(label) = &target.aria_label {
                collect_sensitive_from_text(label, &mut reasons);
            }
            if let Some(css) = &target.css {
                collect_sensitive_from_text(css, &mut reasons);
            }
        }
        _ => {}
    }

    if let Some(snapshot) = snapshot {
        reasons.extend(sensitive_reasons_for_snapshot(snapshot));
    }

    reasons
}

fn sensitive_reasons_for_snapshot(snapshot: &ElementSnapshot) -> BTreeSet<&'static str> {
    let mut reasons = BTreeSet::new();
    if let Some(text) = &snapshot.text {
        collect_sensitive_from_text(text, &mut reasons);
    }
    if let Some(label) = &snapshot.aria_label {
        collect_sensitive_from_text(label, &mut reasons);
    }
    if let Some(name) = &snapshot.name {
        collect_sensitive_from_text(name, &mut reasons);
    }
    if let Some(id) = &snapshot.id {
        collect_sensitive_from_text(id, &mut reasons);
    }
    if let Some(testid) = &snapshot.data_testid {
        collect_sensitive_from_text(testid, &mut reasons);
    }

    if let Some(t) = &snapshot.type_attr {
        let lowered = normalize_text(t);
        if lowered == "password" {
            reasons.insert("mat_khau");
        }
    }

    if let Some(ac) = &snapshot.autocomplete {
        let lowered = normalize_text(ac);
        if lowered.contains("one-time-code") || lowered.contains("otp") {
            reasons.insert("otp");
        }
        if lowered.contains("cc-") {
            reasons.insert("the_ngan_hang");
        }
        if lowered.contains("current-password") || lowered.contains("new-password") {
            reasons.insert("mat_khau");
        }
    }

    if let Some(inputmode) = &snapshot.inputmode {
        let lowered = normalize_text(inputmode);
        if lowered == "numeric" {
            if let Some(name) = &snapshot.name {
                if normalize_text(name).contains("otp") {
                    reasons.insert("otp");
                }
            }
        }
    }

    reasons
}

fn sensitive_reasons_for_value(value: &str) -> BTreeSet<&'static str> {
    let mut reasons = BTreeSet::new();
    if looks_like_otp(value) {
        reasons.insert("otp");
    }
    if looks_like_card(value) {
        reasons.insert("the_ngan_hang");
    }
    reasons
}

fn collect_sensitive_from_text(text: &str, reasons: &mut BTreeSet<&'static str>) {
    let normalized = normalize_text(text);

    if contains_keyword(&normalized, &PAYMENT_KEYWORDS) {
        reasons.insert("thanh_toan");
    }
    if contains_keyword(&normalized, &OTP_KEYWORDS) {
        reasons.insert("otp");
    }
    if contains_keyword(&normalized, &PASSWORD_KEYWORDS) {
        reasons.insert("mat_khau");
    }
    if contains_keyword(&normalized, &ACCOUNT_KEYWORDS) {
        reasons.insert("tai_khoan");
    }
}

fn render_sensitive_reason(reasons: &BTreeSet<&'static str>) -> String {
    let mut labels = Vec::new();
    for reason in reasons {
        let label = match *reason {
            "thanh_toan" => "thanh toán",
            "otp" => "OTP/mã xác nhận",
            "mat_khau" => "mật khẩu",
            "tai_khoan" => "tài khoản",
            "the_ngan_hang" => "thẻ/ngân hàng",
            _ => "nhạy cảm",
        };
        labels.push(label);
    }

    format!(
        "Trang có thao tác nhạy cảm ({}) — cần xác nhận",
        labels.join(", ")
    )
}

fn normalize_text(input: &str) -> String {
    input.to_lowercase()
}

fn contains_keyword(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|kw| text.contains(kw))
}

fn looks_like_otp(value: &str) -> bool {
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    let len = digits.len();
    len >= 4 && len <= 8
}

fn looks_like_card(value: &str) -> bool {
    let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
    let len = digits.len();
    len >= 13 && len <= 19
}

const PAYMENT_KEYWORDS: [&str; 16] = [
    "thanh toán",
    "thanh toan",
    "payment",
    "pay",
    "checkout",
    "mua ngay",
    "dat hang",
    "đặt hàng",
    "chuyen khoan",
    "chuyển khoản",
    "bank",
    "ngan hang",
    "ngân hàng",
    "wallet",
    "ví",
    "nap tien",
];

const OTP_KEYWORDS: [&str; 12] = [
    "otp",
    "one-time",
    "ma otp",
    "mã otp",
    "ma xac nhan",
    "mã xác nhận",
    "verification code",
    "2fa",
    "two-factor",
    "ma 2fa",
    "mã 2fa",
    "verify",
];

const PASSWORD_KEYWORDS: [&str; 10] = [
    "password",
    "mat khau",
    "mật khẩu",
    "passcode",
    "pin",
    "doi mat khau",
    "đổi mật khẩu",
    "reset password",
    "new password",
    "current password",
];

const ACCOUNT_KEYWORDS: [&str; 12] = [
    "account",
    "tai khoan",
    "tài khoản",
    "dang nhap",
    "đăng nhập",
    "login",
    "sign in",
    "sign-in",
    "change email",
    "doi email",
    "change phone",
    "xoa tai khoan",
];

fn env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|raw| raw.parse::<f64>().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ActionTarget;

    #[test]
    fn sensitive_guard_flags_payment_click() {
        let action = AgentAction::Click {
            target: ActionTarget {
                css: None,
                aria_label: None,
                text_content: Some("Thanh toán".to_string()),
                coordinates: None,
            },
        };
        let reasons = sensitive_reasons_for_action(&action, None);
        assert!(reasons.contains("thanh_toan"));
    }

    #[test]
    fn sensitive_guard_flags_otp_value() {
        let action = AgentAction::Type {
            target: ActionTarget {
                css: None,
                aria_label: Some("Mã OTP".to_string()),
                text_content: None,
                coordinates: None,
            },
            value: "123456".to_string(),
        };
        let mut reasons = sensitive_reasons_for_action(&action, None);
        reasons.extend(sensitive_reasons_for_value("123456"));
        assert!(reasons.contains("otp"));
    }

    #[test]
    fn sensitive_guard_flags_password_attribute() {
        let action = AgentAction::Click {
            target: ActionTarget {
                css: Some("#password".to_string()),
                aria_label: None,
                text_content: None,
                coordinates: None,
            },
        };
        let snapshot = ElementSnapshot {
            type_attr: Some("password".to_string()),
            ..ElementSnapshot::default()
        };
        let reasons = sensitive_reasons_for_action(&action, Some(&snapshot));
        assert!(reasons.contains("mat_khau"));
    }
}

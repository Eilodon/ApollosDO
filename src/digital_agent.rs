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
const STUCK_THRESHOLD: u32 = 3; // ADR-026: escalate after 3 identical actions
const ASK_USER_MAX_TURNS: u32 = 3; // ADR-029: max dialogue turns per task

/// ADR-017/018: Ensure browser executor slot is cleared at ALL return points.
macro_rules! return_result {
    ($slot:expr, $result:expr) => {{
        *$slot.lock().await = None;
        return $result;
    }};
}

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

        // ADR-013: Direct warmup emit — no subprocess dependency.
        // Gradient AI is the sole reasoning engine.
        let _ = emit_status("🧠 Đang phân tích yêu cầu...".to_string()).await;

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

        // ADR-012: DO Gradient rate limits — tuned for optimal inference throughput
        let nova_min_gap_s = env_f64("NOVA_MIN_GAP_S", 1.0).max(0.1);
        let nova_burst_limit = env_usize("NOVA_BURST_LIMIT", 4).max(1);
        let nova_burst_window_s = env_f64("NOVA_BURST_WINDOW_S", 15.0).max(1.0);
        let nova_backoff_ms = env_u64("NOVA_BACKOFF_MS", 1000).max(100);

        let mut dialogue_history: Vec<String> = Vec::new();
        let mut step_history: Vec<String> = Vec::new();
        // [v3] Screenshot caching — skip Nova call nếu page không đổi (ADR-006 & ADR-024)
        let mut prev_screenshot_bytes: Option<Vec<u8>> = None;
        let mut prev_screenshot_hash: Option<[u8; 32]> = None;
        let mut consecutive_stable_frames: u32 = 0;
        const MAX_STABLE_WAIT: u32 = 5;
        // ADR-026: Stuck detection
        let mut action_key_history: Vec<String> = Vec::new();
        // ADR-029: Ask user turn counter
        let mut ask_user_count: u32 = 0;

        for step in 1..=MAX_STEPS {
            // ── Safety gate — check TRƯỚC mỗi step ──────────────────────
            if cancel.is_cancelled() {
                tracing::warn!(
                    session_id = %ctx.session_id,
                    step,
                    "Digital agent cancelled at step start by safety system"
                );
                return_result!(ctx.browser_executor_slot, DigitalResult::Failed("Bị gián đoạn bởi hệ thống an toàn".into()));
            }

            // ── Screenshot với cancel race ────────────────────────────────
            let screenshot = tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::warn!(session_id = %ctx.session_id, "Cancelled during screenshot");
                    return_result!(ctx.browser_executor_slot, DigitalResult::Failed("Bị gián đoạn khi chụp màn hình".into()));
                }
                result = browser.screenshot() => match result {
                    Ok(s) => s,
                    Err(e) => return_result!(ctx.browser_executor_slot, DigitalResult::Failed(format!("Screenshot lỗi: {}", e))),
                }
            };

            // [v3] Screenshot caching — nếu page không thay đổi, skip Nova call (ADR-024)
            let current_hash = {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(&screenshot);
                let result = hasher.finalize();
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&result);
                hash
            };

            let is_stable = if prev_screenshot_hash == Some(current_hash) {
                true
            } else if let Some(old_bytes) = &prev_screenshot_bytes {
                !semantic_changed(old_bytes, &screenshot)
            } else {
                false
            };

            if is_stable && step > 1 {
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
                            return_result!(ctx.browser_executor_slot, DigitalResult::Failed("Bị gián đoạn khi chờ tải trang".into()));
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_millis(nova_backoff_ms)) => {}
                    }
                    continue;
                }
            } else {
                consecutive_stable_frames = 0;
            }
            prev_screenshot_hash = Some(current_hash);
            prev_screenshot_bytes = Some(screenshot.clone());

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
                        return_result!(ctx.browser_executor_slot, DigitalResult::Failed("Bị gián đoạn bởi hệ thống an toàn".into()));
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(nova_backoff_ms)) => {}
                }
                continue;
            }

            // ── ADR-031: Extract DOM context for hybrid navigation ────────
            let dom_context = match browser.extract_dom_context().await {
                Ok(ctx_str) => Some(ctx_str),
                Err(e) => {
                    tracing::debug!(session_id = %ctx.session_id, "DOM context extraction failed: {} — using vision only", e);
                    None
                }
            };

            // ── Gradient call with cancel race — CRITICAL ───────────────────
            // API call có thể mất 2–5s. Cancel PHẢI interrupt tại đây.
            let start = Instant::now();
            let action_result = tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::warn!(session_id = %ctx.session_id, "Cancelled during Nova reasoning");
                    return_result!(ctx.browser_executor_slot, DigitalResult::Failed("Bị gián đoạn khi AI đang suy nghĩ".into()));
                }
                result = self.reasoning.next_action_with_cancel(
                    &screenshot, intent, &dialogue_history, &step_history, step,
                    Some(&cancel),
                    dom_context.as_deref(),
                ) => {
                    result
                }
            };
            let latency_ms = start.elapsed().as_millis() as u64;
            ctx.sessions.record_nova_call(latency_ms);
            let action = match action_result {
                Ok(a) => a,
                Err(e) => {
                    return_result!(ctx.browser_executor_slot, DigitalResult::Failed(format!("Nova reasoning lỗi: {}", e)));
                }
            };

            // ── ADR-026: Stuck action detection ──────────────────────────
            let action_key = compute_action_key(&action);
            action_key_history.push(action_key.clone());
            if action_key_history.len() >= STUCK_THRESHOLD as usize {
                let tail = &action_key_history[action_key_history.len() - STUCK_THRESHOLD as usize..];
                if tail.iter().all(|k| k == &action_key) {
                    tracing::warn!(
                        session_id = %ctx.session_id,
                        action_key,
                        "ADR-026: Stuck after {} identical actions — escalating to human",
                        STUCK_THRESHOLD
                    );
                    return_result!(ctx.browser_executor_slot, DigitalResult::NeedHuman(
                        format!("AI bị kẹt lặp lại cùng một thao tác ({}) — cần người hỗ trợ", action_key)
                    ));
                }
            }

            // ── Log history ───────────────────────────────────────────────
            let desc = format!("Step {}: {:?}", step, action);
            tracing::info!(session_id = %ctx.session_id, "{}", desc);
            step_history.push(desc);

            // ── Check terminal + dialogue states TRƯỚC executor ─────────
            match &action {
                AgentAction::Done { summary } => {
                    tracing::info!(session_id = %ctx.session_id, "Digital task done: {}", summary);
                    emit_status(format!("✅ {}", summary)).await;
                    return_result!(ctx.browser_executor_slot, DigitalResult::Done(summary.clone()));
                }
                AgentAction::Escalate { reason } => {
                    tracing::warn!(session_id = %ctx.session_id, "Digital task escalating: {}", reason);
                    // ADR-025: Simulate Safe Mode loop
                    return_result!(ctx.browser_executor_slot, activate_safe_mode(&ctx, reason, &cancel).await);
                }
                AgentAction::AskUser { question } => {
                    // ADR-029: Enforce max ask_user turns
                    ask_user_count += 1;
                    if ask_user_count > ASK_USER_MAX_TURNS {
                        tracing::warn!(session_id = %ctx.session_id, "ADR-029: ask_user limit reached ({})", ASK_USER_MAX_TURNS);
                        return_result!(ctx.browser_executor_slot, DigitalResult::NeedHuman(
                            "AI đã hỏi quá nhiều lần — cần người hỗ trợ trực tiếp".to_string()
                        ));
                    }
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
                            return_result!(ctx.browser_executor_slot, DigitalResult::Failed("Bị gián đoạn trong khi chờ phản hồi".into()));
                        }
                        result = rx => {
                            match result {
                                Ok(ans) => ans,
                                Err(_) => return_result!(ctx.browser_executor_slot, DigitalResult::Failed("Kết nối bị đứt khi chờ phản hồi".into())),
                            }
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_secs(120)) => {
                            emit_status("⏱️ Hết thời gian chờ phản hồi".to_string()).await;
                            return_result!(ctx.browser_executor_slot, DigitalResult::Failed("Hết thời gian chờ phản hồi".into()));
                        }
                    };

                    dialogue_history.push(format!("[User dialogue] Q: {} | A: {}", question, answer));
                    if dialogue_history.len() > 10 {
                        dialogue_history.remove(0);
                    }
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
                    return_result!(ctx.browser_executor_slot, activate_safe_mode(&ctx, &reason, &cancel).await);
                }
                SensitiveGuardOutcome::Cancelled => {
                    return_result!(ctx.browser_executor_slot, DigitalResult::Failed("Bị gián đoạn bởi hệ thống an toàn".into()));
                }
            }

            // ── ADR-027: Navigate URL validation ────────────────────────────
            if let AgentAction::Navigate { url } = &action {
                match validate_navigate_url(url) {
                    NavigateDecision::Block(reason) => {
                        tracing::warn!(session_id = %ctx.session_id, url, "ADR-027: Navigate blocked — {}", reason);
                        step_history.push(format!("Step {}: BLOCKED navigate to {} — {}", step, url, reason));
                        continue; // skip this action, let AI try again
                    }
                    NavigateDecision::Escalate(reason) => {
                        return_result!(ctx.browser_executor_slot, DigitalResult::NeedHuman(reason));
                    }
                    NavigateDecision::Allow => {} // proceed
                }
            }

            // ── Execute action với cancel race ────────────────────────────
            let exec_result = tokio::select! {
                _ = cancel.cancelled() => {
                    return_result!(ctx.browser_executor_slot, DigitalResult::Failed("Bị gián đoạn khi thực thi action".into()));
                }
                result = browser.execute(&action) => result
            };

            if let Err(e) = exec_result {
                return_result!(ctx.browser_executor_slot, DigitalResult::Failed(format!("Browser execute lỗi: {}", e)));
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

        // ADR-017: Clear browser executor slot before final return
        *ctx.browser_executor_slot.lock().await = None;
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

    // ADR-013: call_sdk_bridge() removed — DO Gradient is sole reasoning engine
}

// ── ADR-026: Stuck action detection helper ──────────────────────────────
fn compute_action_key(action: &AgentAction) -> String {
    match action {
        AgentAction::Click { target } => {
            format!("click:{}", target.css.as_deref()
                .or(target.aria_label.as_deref())
                .or(target.text_content.as_deref())
                .unwrap_or("coords"))
        }
        AgentAction::Type { target, value } => {
            format!("type:{}:{}", target.css.as_deref()
                .or(target.aria_label.as_deref())
                .unwrap_or("?"), &value[..value.len().min(10)])
        }
        AgentAction::Navigate { url } => format!("navigate:{}", url),
        AgentAction::Scroll { direction } => format!("scroll:{}", direction),
        AgentAction::Wait { .. } => "wait".to_string(),
        AgentAction::Done { .. } => "done".to_string(),
        AgentAction::Escalate { .. } => "escalate".to_string(),
        AgentAction::AskUser { .. } => "ask_user".to_string(),
    }
}

// ── ADR-027: Navigate URL validation ────────────────────────────────────
enum NavigateDecision {
    Allow,
    Block(String),
    Escalate(String),
}

fn validate_navigate_url(url: &str) -> NavigateDecision {
    let lower = url.to_lowercase();

    // Block dangerous protocols
    if lower.starts_with("javascript:")
        || lower.starts_with("data:")
        || lower.starts_with("file:")
        || lower.starts_with("vbscript:")
    {
        return NavigateDecision::Block(format!("Blocked protocol: {}", url.split(':').next().unwrap_or("?")));
    }

    // Block local/private IPs
    let host_part = lower.split("//").nth(1).unwrap_or("").split('/').next().unwrap_or("");
    if host_part.starts_with("127.")
        || host_part.starts_with("192.168.")
        || host_part.starts_with("10.")
        || host_part == "localhost"
        || host_part.starts_with("172.16.")
    {
        return NavigateDecision::Block(format!("Blocked local/private address: {}", host_part));
    }

    // Escalate payment/banking URLs
    let sensitive_domains = ["checkout", "payment", "pay/", "/pay?", "billing", "purchase"];
    for domain in &sensitive_domains {
        if host_part.contains(domain) {
            return NavigateDecision::Escalate(
                format!("URL chứa trang nhạy cảm ({}) — cần xác nhận", domain)
            );
        }
    }

    // Allow must be https:// or http://
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return NavigateDecision::Block(format!("Only HTTP/HTTPS allowed, got: {}", url.split(':').next().unwrap_or("?")));
    }

    NavigateDecision::Allow
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

use image::GenericImageView;

fn semantic_changed(old: &[u8], new: &[u8]) -> bool {
    use sha2::{Digest, Sha256};

    const SEMANTIC_DIFF_THRESHOLD: f64 = 0.05;

    if old == new { return false; }

    // ADR-036: Fast path — SHA256 exact match
    let hash_old = { let mut h = Sha256::new(); h.update(old); h.finalize() };
    let hash_new = { let mut h = Sha256::new(); h.update(new); h.finalize() };
    if hash_old == hash_new { return false; }

    let (img1, img2) = match (image::load_from_memory(old), image::load_from_memory(new)) {
        (Ok(a), Ok(b)) => (a, b),
        _ => return true,
    };

    if img1.dimensions() != img2.dimensions() { return true; }

    let (w, h) = img1.dimensions();
    let total = (w * h) as f64;
    // ADR-036: early exit threshold — stop counting after threshold exceeded
    let max_diff = (total * SEMANTIC_DIFF_THRESHOLD) as u64 + 1;

    let (rgba1, rgba2) = (img1.to_rgba8(), img2.to_rgba8());
    let mut diff: u64 = 0;

    for (p1, p2) in rgba1.pixels().zip(rgba2.pixels()) {
        if p1 != p2 {
            diff += 1;
            if diff > max_diff {
                return true; // ADR-036: early exit
            }
        }
    }

    false
}

async fn activate_safe_mode(
    ctx: &DigitalSessionContext,
    reason: &str,
    cancel: &CancellationToken,
) -> DigitalResult {
    // ADR-032: Bound safe mode — max 10 × 30s = 5 minutes, never infinite
    const SAFE_MODE_MAX_LOOPS: u32 = 10;

    let ws = ctx.ws_registry.clone();
    let sid = ctx.session_id.clone();

    // ADR-032: Call human fallback at entry, not never
    let help_link = ctx.fallback.create_help_session(&sid, reason).await;
    if let Some(ref msg) = help_link {
        let _ = ws.send_live(
            &sid,
            BackendToClientMessage::HumanHelpSession(msg.clone()),
        ).await;
    }

    // Navigate to safe blank page while slot is still populated
    if let Some(browser) = ctx.browser_executor_slot.lock().await.clone() {
        let _ = browser.execute(&AgentAction::Navigate {
            url: "about:blank".to_string(),
        }).await;
    }

    let base_msg = format!(
        "🔒 Safe Mode: {} — Đang kết nối người hỗ trợ, vui lòng giữ nguyên vị trí.",
        reason
    );

    for _ in 0..SAFE_MODE_MAX_LOOPS {
        let _ = ws.send_live(
            &sid,
            BackendToClientMessage::AssistantText(AssistantTextMessage {
                session_id: sid.clone(),
                timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                text: base_msg.clone(),
            }),
        ).await;

        tokio::select! {
            _ = cancel.cancelled() => {
                return DigitalResult::NeedHuman(reason.to_string());
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {}
        }
    }

    // ADR-032: Max loops reached — return NeedHuman regardless
    DigitalResult::NeedHuman(format!("Đang chờ hỗ trợ: {}", reason))
}

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

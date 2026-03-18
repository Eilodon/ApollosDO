use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::types::MotionState;
use crate::browser_executor::BrowserExecutor;
use crate::digital_agent::{DigitalResult, UserReplyTx};

// ADR-012: In-memory session store — no external database dependency

pub struct DigitalAgentHandle {
    pub cancel: CancellationToken,
    pub task: JoinHandle<DigitalResult>,
}

impl std::fmt::Debug for DigitalAgentHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DigitalAgentHandle")
            .field("cancel", &"<CancellationToken>")
            .field("task", &"<JoinHandle>")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigitalAgentCancelReason {
    HardStop,
    EdgeHazard,
    ReRegister,
    EmotionPanic,
    SessionPrune,
}

impl DigitalAgentCancelReason {
    fn as_label(self) -> &'static str {
        match self {
            Self::HardStop => "hard_stop",
            Self::EdgeHazard => "edge_hazard",
            Self::ReRegister => "re_register",
            Self::EmotionPanic => "emotion_panic",
            Self::SessionPrune => "session_prune",
        }
    }
}

#[derive(Debug, Default)]
struct DigitalAgentCancelMetrics {
    hard_stop: AtomicU64,
    edge_hazard: AtomicU64,
    re_register: AtomicU64,
    emotion_panic: AtomicU64,
    session_prune: AtomicU64,
}

impl DigitalAgentCancelMetrics {
    fn increment(&self, reason: DigitalAgentCancelReason) {
        let counter = match reason {
            DigitalAgentCancelReason::HardStop => &self.hard_stop,
            DigitalAgentCancelReason::EdgeHazard => &self.edge_hazard,
            DigitalAgentCancelReason::ReRegister => &self.re_register,
            DigitalAgentCancelReason::EmotionPanic => &self.emotion_panic,
            DigitalAgentCancelReason::SessionPrune => &self.session_prune,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> DigitalAgentCancelSnapshot {
        DigitalAgentCancelSnapshot {
            hard_stop: self.hard_stop.load(Ordering::Relaxed),
            edge_hazard: self.edge_hazard.load(Ordering::Relaxed),
            re_register: self.re_register.load(Ordering::Relaxed),
            emotion_panic: self.emotion_panic.load(Ordering::Relaxed),
            session_prune: self.session_prune.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DigitalAgentCancelSnapshot {
    pub hard_stop: u64,
    pub edge_hazard: u64,
    pub re_register: u64,
    pub emotion_panic: u64,
    pub session_prune: u64,
}

#[derive(Debug, Default)]
struct NovaCallMetrics {
    calls_total: AtomicU64,
    calls_blocked: AtomicU64,
    latency_ms_sum: AtomicU64,
    latency_ms_count: AtomicU64,
}

impl NovaCallMetrics {
    fn record_call(&self, latency_ms: u64) {
        self.calls_total.fetch_add(1, Ordering::Relaxed);
        self.latency_ms_sum
            .fetch_add(latency_ms, Ordering::Relaxed);
        self.latency_ms_count.fetch_add(1, Ordering::Relaxed);
    }

    fn record_blocked(&self) {
        self.calls_blocked.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Debug)]
pub struct SessionState {
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub motion_state: MotionState,
    pub last_seen: DateTime<Utc>,
    pub digital_agent_handle: Option<DigitalAgentHandle>,
    pub pending_user_reply: Arc<tokio::sync::Mutex<Option<UserReplyTx>>>,
    pub browser_executor: Arc<tokio::sync::Mutex<Option<Arc<BrowserExecutor>>>>,
    pub nova_call_timestamps: Vec<f64>,
    pub nova_call_total: u64,
    pub dialogue_history: Vec<String>,
    pub step_history: Vec<String>,
    pub action_key_history: Vec<String>,
    pub ask_user_count: u32,
}

impl SessionState {
    fn new(session_id: String) -> Self {
        Self {
            session_id,
            created_at: Utc::now(),
            motion_state: MotionState::Stationary,
            last_seen: Utc::now(),
            digital_agent_handle: None,
            pending_user_reply: Arc::new(tokio::sync::Mutex::new(None)),
            browser_executor: Arc::new(tokio::sync::Mutex::new(None)),
            nova_call_timestamps: Vec::new(),
            nova_call_total: 0,
            dialogue_history: Vec::new(),
            step_history: Vec::new(),
            action_key_history: Vec::new(),
            ask_user_count: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SessionStore {
    inner: Arc<RwLock<HashMap<String, Arc<RwLock<SessionState>>>>>,
    digital_agent_cancel_metrics: Arc<DigitalAgentCancelMetrics>,
    nova_call_metrics: Arc<NovaCallMetrics>,
}

impl SessionStore {
    pub async fn get_session(&self, session_id: &str) -> Option<Arc<RwLock<SessionState>>> {
        let guard = self.inner.read().await;
        guard.get(session_id).cloned()
    }

    pub async fn get_browser_executor_slot(
        &self,
        session_id: &str,
    ) -> Arc<tokio::sync::Mutex<Option<Arc<BrowserExecutor>>>> {
        if let Some(arc) = self.inner.read().await.get(session_id) {
            return arc.read().await.browser_executor.clone();
        }
        Arc::new(tokio::sync::Mutex::new(None))
    }

    pub async fn get_reply_slot(
        &self,
        session_id: &str,
    ) -> Arc<tokio::sync::Mutex<Option<UserReplyTx>>> {
        if let Some(arc) = self.inner.read().await.get(session_id) {
            let slot = arc.read().await.pending_user_reply.clone();
            return slot;
        }
        let mut guard = self.inner.write().await;
        let arc = guard.entry(session_id.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(SessionState::new(session_id.to_string()))))
            .clone();
        let slot = arc.read().await.pending_user_reply.clone();
        slot
    }



    pub async fn send_user_reply(&self, session_id: &str, answer: String) -> bool {
        let slot = self.get_reply_slot(session_id).await;
        let mut guard = slot.lock().await;
        if let Some(tx) = guard.take() {
            tx.send(answer).is_ok()
        } else {
            false
        }
    }

    pub async fn touch_session(
        &self,
        session_id: &str,
        motion_state: Option<MotionState>,
        _lat: Option<f64>,
        _lng: Option<f64>,
        _heading: Option<f32>,
        _aria: Option<serde_json::Value>,
        _active: bool,
    ) {
        let mut guard = self.inner.write().await;
        let arc = guard.entry(session_id.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(SessionState::new(session_id.to_string()))));
        let mut state = arc.write().await;
        state.last_seen = Utc::now();
        if let Some(ms) = motion_state {
            state.motion_state = ms;
        }
    }

    pub async fn set_digital_agent_handle(
        &self,
        session_id: &str,
        handle: DigitalAgentHandle,
    ) {
        let mut guard = self.inner.write().await;
        let arc = guard.entry(session_id.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(SessionState::new(session_id.to_string()))));
        let mut state = arc.write().await;
        if let Some(prev) = state.digital_agent_handle.replace(handle) {
            prev.cancel.cancel();
            self.digital_agent_cancel_metrics.increment(DigitalAgentCancelReason::ReRegister);
        }
    }

    pub async fn cancel_digital_agent(
        &self,
        session_id: &str,
        reason: DigitalAgentCancelReason,
    ) {
        if let Some(arc) = self.inner.read().await.get(session_id).cloned() {
            let mut state = arc.write().await;

            // ADR-006: Clear browser executor slot BEFORE cancelling the token.
            // This ensures the Arc<BrowserExecutor> refcount drops to 0 promptly,
            // allowing the Chrome process to be cleaned up by Drop.
            {
                let mut slot = state.browser_executor.lock().await;
                *slot = None;
                tracing::debug!(
                    session_id = %session_id,
                    "browser_executor_slot cleared on cancel ({:?})",
                    reason
                );
            }

            if let Some(handle) = state.digital_agent_handle.take() {
                handle.cancel.cancel();
                self.digital_agent_cancel_metrics.increment(reason);
                tracing::info!(
                    session_id = %session_id,
                    reason = reason.as_label(),
                    "digital agent cancelled"
                );
            }
        }
    }

    pub async fn clear_digital_agent_handle(&self, session_id: &str) {
        if let Some(arc) = self.inner.read().await.get(session_id).cloned() {
            arc.write().await.digital_agent_handle.take();
        }
    }

    pub async fn should_allow_nova_call(
        &self,
        session_id: &str,
        now: f64,
        min_gap_s: f64,
        burst_limit: usize,
        burst_window_s: f64,
    ) -> bool {
        if let Some(arc) = self.inner.read().await.get(session_id).cloned() {
            let mut state = arc.write().await;
            
            // Clean up old timestamps
            state.nova_call_timestamps.retain(|&t| now - t < burst_window_s);
            
            if let Some(&last) = state.nova_call_timestamps.last() {
                if now - last < min_gap_s {
                    return false;
                }
            }
            
            if state.nova_call_timestamps.len() >= burst_limit {
                return false;
            }
            
            state.nova_call_timestamps.push(now);
            state.nova_call_total += 1;
            true
        } else {
            false
        }
    }

    pub fn record_nova_call(&self, latency_ms: u64) {
        self.nova_call_metrics.record_call(latency_ms);
    }

    pub fn record_nova_blocked(&self) {
        self.nova_call_metrics.record_blocked();
    }
}

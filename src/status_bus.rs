use std::collections::VecDeque;
use std::sync::Mutex as StdMutex;

use tokio::sync::broadcast;

const STATUS_BUFFER_CAPACITY: usize = 50;

static STATUS_TX: std::sync::OnceLock<broadcast::Sender<String>> = std::sync::OnceLock::new();
static STATUS_REPLAY_BUFFER: std::sync::OnceLock<StdMutex<VecDeque<String>>> =
    std::sync::OnceLock::new();

fn status_tx() -> &'static broadcast::Sender<String> {
    STATUS_TX.get_or_init(|| {
        let (tx, _) = broadcast::channel(64);
        tx
    })
}

fn replay_buffer() -> &'static StdMutex<VecDeque<String>> {
    STATUS_REPLAY_BUFFER
        .get_or_init(|| StdMutex::new(VecDeque::with_capacity(STATUS_BUFFER_CAPACITY)))
}

pub fn publish(message: impl Into<String>) {
    let message = message.into();
    let _ = status_tx().send(message.clone());

    if let Ok(mut buffer) = replay_buffer().lock() {
        if buffer.len() >= STATUS_BUFFER_CAPACITY {
            buffer.pop_front();
        }
        buffer.push_back(message);
    }
}

pub fn clear_replay() {
    if let Ok(mut buffer) = replay_buffer().lock() {
        buffer.clear();
    }
}

pub fn replay_snapshot() -> Vec<String> {
    replay_buffer()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .cloned()
        .collect()
}

pub fn subscribe() -> broadcast::Receiver<String> {
    status_tx().subscribe()
}

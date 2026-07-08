//! Latest-value channel for rolling inference windows.
//!
//! Rolling windows supersede older rolling work. Final windows are retained
//! because they close an utterance and may contain unique speech.

use std::{
    collections::VecDeque,
    sync::{Condvar, Mutex},
    time::Duration,
};

use super::audio_window::AudioWindow;

pub struct QueuedWindow {
    pub window: AudioWindow,
    pub queued_at: std::time::Instant,
}

pub enum Recv {
    Window(QueuedWindow),
    /// Timed out with nothing queued; the caller can re-check its stop flags.
    Empty,
    /// The producer is gone and the queue is drained.
    Closed,
}

struct Inner {
    buffer: VecDeque<QueuedWindow>,
    closed: bool,
}

pub struct ChunkQueue {
    inner: Mutex<Inner>,
    available: Condvar,
}

impl ChunkQueue {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                buffer: VecDeque::with_capacity(2),
                closed: false,
            }),
            available: Condvar::new(),
        }
    }

    /// Replace any pending rolling snapshot with the newest available audio.
    pub fn push(&self, window: AudioWindow) -> usize {
        let mut inner = self.lock();
        let before = inner.buffer.len();
        inner.buffer.retain(|queued| queued.window.final_window);
        let replaced = before - inner.buffer.len();
        inner.buffer.push_back(QueuedWindow {
            window,
            queued_at: std::time::Instant::now(),
        });
        drop(inner);
        self.available.notify_one();
        replaced
    }

    /// Wait up to `timeout` for the next chunk so callers stay responsive to
    /// cancellation while blocked.
    pub fn recv_timeout(&self, timeout: Duration) -> Recv {
        let mut inner = self.lock();
        loop {
            if let Some(window) = inner.buffer.pop_front() {
                return Recv::Window(window);
            }
            if inner.closed {
                return Recv::Closed;
            }
            let (guard, result) = self
                .available
                .wait_timeout(inner, timeout)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            inner = guard;
            if result.timed_out() && inner.buffer.is_empty() && !inner.closed {
                return Recv::Empty;
            }
        }
    }

    /// Signal that no more chunks will arrive and wake any waiting consumer.
    pub fn close(&self) {
        self.lock().closed = true;
        self.available.notify_all();
    }

    pub fn depth(&self) -> usize {
        self.lock().buffer.len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(final_window: bool) -> AudioWindow {
        AudioWindow {
            start_sample: 0,
            reason: "test",
            final_window,
            pcm_gap_ms: 0,
            speech_ms: 0,
            samples: vec![0.0],
        }
    }

    #[test]
    fn rolling_windows_replace_stale_rolling_work() {
        let queue = ChunkQueue::new();
        assert_eq!(queue.push(window(false)), 0);
        assert_eq!(queue.push(window(false)), 1);
        assert_eq!(queue.depth(), 1);
    }

    #[test]
    fn final_windows_are_never_replaced() {
        let queue = ChunkQueue::new();
        queue.push(window(true));
        assert_eq!(queue.push(window(false)), 0);
        assert_eq!(queue.depth(), 2);
    }
}

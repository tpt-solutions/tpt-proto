//! Cancellation token for first-class cancellation across async tasks.
//!
//! A lightweight, `Clone`-able handle that any number of tasks can observe.
//! When cancelled, all async waiters are released and synchronous waiters
//! unblock.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Notify;

#[derive(Debug)]
struct CancellationState {
    cancelled: AtomicBool,
    notify: Notify,
}

/// A handle that can be used to signal and observe cancellation.
///
/// Cloning produces another handle to the same underlying state. Cancellation
/// is cooperative: tasks must await [`CancellationToken::cancelled`] or call
/// [`CancellationToken::wait`], or poll [`CancellationToken::is_cancelled`].
#[derive(Clone, Debug)]
pub struct CancellationToken {
    inner: Arc<CancellationState>,
}

impl CancellationToken {
    /// Create a new, uncancelled token.
    pub fn new() -> Self {
        CancellationToken {
            inner: Arc::new(CancellationState {
                cancelled: AtomicBool::new(false),
                notify: Notify::new(),
            }),
        }
    }

    /// Whether cancellation has been signalled.
    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    /// Signal cancellation, waking any async and synchronous waiters.
    pub fn cancel(&self) {
        self.inner.cancelled.store(true, Ordering::SeqCst);
        self.inner.notify.notify_waiters();
    }

    /// Async: resolve once cancellation has been signalled.
    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.inner.notify.notified().await;
    }

    /// Block the current thread until cancellation is signalled.
    pub fn wait(&self) {
        if self.is_cancelled() {
            return;
        }
        futures::executor::block_on(self.cancelled());
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_cancelled_initially() {
        let t = CancellationToken::new();
        assert!(!t.is_cancelled());
    }

    #[test]
    fn cancel_propagates_to_clones() {
        let t = CancellationToken::new();
        let c = t.clone();
        t.cancel();
        assert!(c.is_cancelled());
        assert!(t.is_cancelled());
    }

    #[test]
    fn async_cancel_wakes() {
        let t = CancellationToken::new();
        let c = t.clone();
        let handle = std::thread::spawn(move || {
            futures::executor::block_on(c.cancelled());
            true
        });
        t.cancel();
        assert!(handle.join().unwrap());
    }

    #[test]
    fn wait_returns_after_cancel() {
        let t = CancellationToken::new();
        let c = t.clone();
        let handle = std::thread::spawn(move || {
            c.wait();
            true
        });
        t.cancel();
        assert!(handle.join().unwrap());
    }
}

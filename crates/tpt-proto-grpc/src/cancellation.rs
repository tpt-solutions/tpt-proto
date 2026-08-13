//! Cancellation token for first-class cancellation across async tasks.
//!
//! A lightweight, `Clone`-able, synchronization-free-of-async handle that any
//! number of tasks can observe. When cancelled, all waiters are released.

use std::sync::{Arc, Condvar, Mutex};

#[derive(Debug)]
struct CancellationState {
    cancelled: bool,
}

/// A handle that can be used to signal and observe cancellation.
///
/// Cloning produces another handle to the same underlying state. Cancellation
/// is cooperative: tasks must poll [`CancellationToken::is_cancelled`] or call
/// [`CancellationToken::wait`].
#[derive(Clone, Debug)]
pub struct CancellationToken {
    inner: Arc<(Mutex<CancellationState>, Condvar)>,
}

impl CancellationToken {
    /// Create a new, uncancelled token.
    pub fn new() -> Self {
        CancellationToken {
            inner: Arc::new((Mutex::new(CancellationState { cancelled: false }), Condvar::new())),
        }
    }

    /// Whether cancellation has been signalled.
    pub fn is_cancelled(&self) -> bool {
        self.inner.0.lock().unwrap().cancelled
    }

    /// Signal cancellation, waking any waiters.
    pub fn cancel(&self) {
        let mut guard = self.inner.0.lock().unwrap();
        guard.cancelled = true;
        self.inner.1.notify_all();
    }

    /// Block until cancellation is signalled.
    pub fn wait(&self) {
        let mut guard = self.inner.0.lock().unwrap();
        while !guard.cancelled {
            guard = self.inner.1.wait(guard).unwrap();
        }
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

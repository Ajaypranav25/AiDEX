//! Cooperative cancellation shared across threads.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// Why a [`CancelToken`] stopped the work it guards.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelReason {
    /// [`CancelToken::cancel`] was called on this token or a clone of it.
    Cancelled,
    /// The token's deadline passed.
    DeadlineExceeded,
}

/// A cloneable cancellation flag with an optional deadline.
///
/// Clones share one flag: cancelling any clone cancels all of them. A deadline is
/// per token value; [`CancelToken::with_deadline`] returns a token that shares the
/// flag and adds (or tightens) a deadline. Work checks the token cooperatively; the
/// token never interrupts a thread by itself.
#[derive(Clone, Debug, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
    deadline: Option<Instant>,
}

impl CancelToken {
    /// A token that is not cancelled and has no deadline.
    pub fn new() -> Self {
        Self::default()
    }

    /// A token sharing this token's flag whose deadline is the earlier of the
    /// existing deadline and `deadline`.
    #[must_use]
    pub fn with_deadline(&self, deadline: Instant) -> Self {
        Self {
            flag: Arc::clone(&self.flag),
            deadline: Some(self.deadline.map_or(deadline, |own| own.min(deadline))),
        }
    }

    /// [`Self::with_deadline`] at `now + timeout`; a timeout too large to represent
    /// adds no deadline.
    #[must_use]
    pub fn with_timeout(&self, timeout: Duration) -> Self {
        match Instant::now().checked_add(timeout) {
            Some(deadline) => self.with_deadline(deadline),
            None => self.clone(),
        }
    }

    /// Request cancellation of every clone of this token.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// Whether [`Self::cancel`] was called (deadlines are not considered).
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Time left before the deadline; `None` without a deadline.
    pub fn remaining(&self) -> Option<Duration> {
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }

    /// `Some(reason)` once the work should stop. Explicit cancellation wins over
    /// an expired deadline.
    pub fn state(&self) -> Option<CancelReason> {
        if self.is_cancelled() {
            Some(CancelReason::Cancelled)
        } else if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            Some(CancelReason::DeadlineExceeded)
        } else {
            None
        }
    }

    /// Whether the work should stop for either reason.
    pub fn is_stopped(&self) -> bool {
        self.state().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_cancellation() {
        let token = CancelToken::new();
        let clone = token.clone();
        assert_eq!(token.state(), None);
        clone.cancel();
        assert!(token.is_cancelled());
        assert_eq!(token.state(), Some(CancelReason::Cancelled));
    }

    #[test]
    fn deadlines_tighten_and_expire() {
        let token = CancelToken::new();
        let later = token.with_timeout(Duration::from_secs(3600));
        let sooner = later.with_deadline(Instant::now());
        assert!(later.remaining().unwrap() > Duration::from_secs(3000));
        assert_eq!(sooner.state(), Some(CancelReason::DeadlineExceeded));
        let relaxed = sooner.with_timeout(Duration::from_secs(3600));
        assert_eq!(relaxed.deadline(), sooner.deadline());
        assert_eq!(token.state(), None);
        token.cancel();
        assert_eq!(sooner.state(), Some(CancelReason::Cancelled));
    }
}

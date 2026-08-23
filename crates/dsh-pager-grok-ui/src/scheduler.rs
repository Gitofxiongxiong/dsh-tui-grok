//! Bounded UI scheduling and reconnect backoff primitives.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SchedulerStats {
    pub enqueued: u64,
    pub processed: u64,
    pub dropped: u64,
    pub max_pending: usize,
}

#[derive(Debug, Clone)]
pub struct BoundedScheduler<T> {
    queue: VecDeque<T>,
    capacity: usize,
    stats: SchedulerStats,
}

impl<T> BoundedScheduler<T> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "scheduler capacity must be non-zero");
        Self {
            queue: VecDeque::with_capacity(capacity),
            capacity,
            stats: SchedulerStats::default(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Enqueue the newest event and discard the oldest event on overflow.
    /// Notifications are replayable from host authority, so retaining the
    /// newest window gives the UI bounded latency while a repair path handles
    /// any resulting sequence gap.
    pub fn push(&mut self, item: T) {
        self.stats.enqueued = self.stats.enqueued.saturating_add(1);
        if self.queue.len() == self.capacity {
            self.queue.pop_front();
            self.stats.dropped = self.stats.dropped.saturating_add(1);
        }
        self.queue.push_back(item);
        self.stats.max_pending = self.stats.max_pending.max(self.queue.len());
    }

    pub fn pop(&mut self) -> Option<T> {
        let item = self.queue.pop_front();
        if item.is_some() {
            self.stats.processed = self.stats.processed.saturating_add(1);
        }
        item
    }

    pub fn drain_budget(&mut self, budget: usize) -> Vec<T> {
        (0..budget).filter_map(|_| self.pop()).collect()
    }

    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    pub fn stats(&self) -> SchedulerStats {
        self.stats
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconnectPolicy {
    pub initial: Duration,
    pub maximum: Duration,
    pub max_attempts: u32,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(100),
            maximum: Duration::from_secs(5),
            max_attempts: 8,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReconnectState {
    policy: ReconnectPolicy,
    attempts: u32,
    next_at: Option<Instant>,
}

impl ReconnectState {
    pub fn new(policy: ReconnectPolicy) -> Self {
        Self {
            policy,
            attempts: 0,
            next_at: None,
        }
    }

    pub fn schedule(&mut self, now: Instant) -> Option<Duration> {
        if self.attempts >= self.policy.max_attempts {
            return None;
        }
        let multiplier = 1u32 << self.attempts.min(16);
        let delay = self
            .policy
            .initial
            .checked_mul(multiplier)
            .unwrap_or(self.policy.maximum)
            .min(self.policy.maximum);
        self.attempts = self.attempts.saturating_add(1);
        self.next_at = Some(now + delay);
        Some(delay)
    }

    pub fn ready(&self, now: Instant) -> bool {
        self.next_at.is_some_and(|deadline| now >= deadline)
    }

    pub fn reset(&mut self) {
        self.attempts = 0;
        self.next_at = None;
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    pub fn exhausted(&self) -> bool {
        self.attempts >= self.policy.max_attempts
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationGuard {
    session_id: u64,
    request_id: u64,
    generation: u64,
}

impl GenerationGuard {
    pub fn new(session_id: u64, request_id: u64, generation: u64) -> Self {
        Self {
            session_id,
            request_id,
            generation,
        }
    }

    pub fn accepts(&self, session_id: u64, request_id: u64, generation: u64) -> bool {
        self.session_id == session_id
            && self.request_id == request_id
            && self.generation == generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_is_bounded_and_reports_drops() {
        let mut scheduler = BoundedScheduler::new(2);
        scheduler.push(1);
        scheduler.push(2);
        scheduler.push(3);
        assert_eq!(scheduler.pending(), 2);
        assert_eq!(scheduler.drain_budget(2), vec![2, 3]);
        assert_eq!(scheduler.stats().dropped, 1);
        assert_eq!(scheduler.stats().max_pending, 2);
    }

    #[test]
    fn reconnect_backoff_is_exponential_and_bounded() {
        let policy = ReconnectPolicy {
            initial: Duration::from_millis(10),
            maximum: Duration::from_millis(25),
            max_attempts: 4,
        };
        let mut state = ReconnectState::new(policy);
        let now = Instant::now();
        assert_eq!(state.schedule(now), Some(Duration::from_millis(10)));
        assert_eq!(state.schedule(now), Some(Duration::from_millis(20)));
        assert_eq!(state.schedule(now), Some(Duration::from_millis(25)));
        assert!(state.ready(now + Duration::from_millis(25)));
    }

    #[test]
    fn generation_guard_rejects_late_results() {
        let guard = GenerationGuard::new(1, 2, 3);
        assert!(guard.accepts(1, 2, 3));
        assert!(!guard.accepts(1, 2, 4));
    }
}

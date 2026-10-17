//! Snowflake identifier generation.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// A generator of time-ordered 64-bit identifiers.
pub trait IdGenerator: Send + Sync {
    /// Return the next identifier. Must be unique across the cluster.
    fn next_id(&self) -> i64;
}

/// A classic Snowflake generator: 41 bits of millisecond time, 10 bits of node
/// id, and 12 bits of per-millisecond sequence.
pub struct Snowflake {
    node_id: u64,
    epoch_ms: u64,
    state: Mutex<State>,
}

struct State {
    last_ms: u64,
    sequence: u64,
}

/// The ChatBox epoch (2026-01-01T00:00:00Z) in Unix milliseconds.
pub const DEFAULT_EPOCH_MS: u64 = 1_767_225_600_000;

impl Snowflake {
    /// Create a generator for `node_id`. The node id must fit in 10 bits.
    pub fn new(node_id: u64) -> Self {
        Self {
            node_id: node_id & 0x3ff,
            epoch_ms: DEFAULT_EPOCH_MS,
            state: Mutex::new(State {
                last_ms: 0,
                sequence: 0,
            }),
        }
    }

    /// Create a generator with an explicit epoch (used in tests).
    pub fn with_epoch(node_id: u64, epoch_ms: u64) -> Self {
        let mut s = Self::new(node_id);
        s.epoch_ms = epoch_ms;
        s
    }

    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(self.epoch_ms)
            .saturating_sub(self.epoch_ms)
    }
}

impl IdGenerator for Snowflake {
    fn next_id(&self) -> i64 {
        let mut state = self.state.lock().expect("snowflake mutex poisoned");
        let mut now = self.now_ms();
        if now < state.last_ms {
            // Clock moved backwards: keep monotonicity by reusing the last ms.
            now = state.last_ms;
        }
        if now == state.last_ms {
            state.sequence = (state.sequence + 1) & 0xfff;
            if state.sequence == 0 {
                // Sequence exhausted for this millisecond; wait for the next.
                while self.now_ms() <= state.last_ms {
                    std::hint::spin_loop();
                }
                now = self.now_ms();
            }
        } else {
            state.sequence = 0;
        }
        state.last_ms = now;
        let id = (now << 22) | ((self.node_id & 0x3ff) << 12) | state.sequence;
        // IDs are stored as signed 64-bit; keep them positive.
        (id & 0x7fff_ffff_ffff_ffff) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_increasing() {
        let sf = Snowflake::new(1);
        let a = sf.next_id();
        let b = sf.next_id();
        assert!(b > a, "{b} should be greater than {a}");
        let ids: std::collections::HashSet<i64> = (0..10_000).map(|_| sf.next_id()).collect();
        assert_eq!(ids.len(), 10_000);
    }

    #[test]
    fn node_id_is_encoded() {
        let a = Snowflake::new(1).next_id();
        let b = Snowflake::new(2).next_id();
        let node_a = (a >> 12) & 0x3ff;
        let node_b = (b >> 12) & 0x3ff;
        assert_eq!(node_a, 1);
        assert_eq!(node_b, 2);
    }
}

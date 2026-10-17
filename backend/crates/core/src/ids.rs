//! Snowflake identifier generation.
//!
//! IDs are **53-bit** values so they round-trip losslessly through JavaScript
//! `Number` (and any other IEEE-754 double). A full 64-bit id would exceed
//! `Number.MAX_SAFE_INTEGER` (2^53 - 1) and be silently rounded by the web and
//! Android clients. The layout is:
//!
//! ```text
//!  41 bits  milliseconds since the ChatBox epoch
//!   6 bits  node id          (0..63)
//!   6 bits  sequence         (0..63 per millisecond, per node)
//! ```
//!
//! That leaves the top 11 bits zero, keeping every generated value positive
//! and within `2^53 - 1`. Higher throughput per node can be obtained by
//! lowering the timestamp precision; correctness for the clients takes
//! precedence over the larger id space.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// A generator of time-ordered, JavaScript-safe 64-bit identifiers.
pub trait IdGenerator: Send + Sync {
    /// Return the next identifier. Must be unique across the cluster.
    fn next_id(&self) -> i64;
}

/// A 53-bit Snowflake generator.
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

const NODE_BITS: u64 = 6;
const SEQUENCE_BITS: u64 = 6;
const NODE_MASK: u64 = (1 << NODE_BITS) - 1;
const SEQUENCE_MASK: u64 = (1 << SEQUENCE_BITS) - 1;
const TIMESTAMP_BITS: u64 = 41;
const TIMESTAMP_MASK: u64 = (1 << TIMESTAMP_BITS) - 1;
const NODE_SHIFT: u64 = SEQUENCE_BITS;
const TIMESTAMP_SHIFT: u64 = SEQUENCE_BITS + NODE_BITS;

impl Snowflake {
    /// Create a generator for `node_id`. The node id must fit in 6 bits.
    pub fn new(node_id: u64) -> Self {
        Self {
            node_id: node_id & NODE_MASK,
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
            & TIMESTAMP_MASK
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
            state.sequence = (state.sequence + 1) & SEQUENCE_MASK;
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
        let id =
            (now << TIMESTAMP_SHIFT) | ((self.node_id & NODE_MASK) << NODE_SHIFT) | state.sequence;
        id as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The largest value a JavaScript `Number` represents exactly.
    const JS_MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

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
    fn ids_fit_in_a_javascript_number() {
        let sf = Snowflake::new(63);
        for _ in 0..5_000 {
            let id = sf.next_id();
            assert!(id > 0);
            assert!(id <= JS_MAX_SAFE_INTEGER, "{id} exceeds MAX_SAFE_INTEGER");
        }
    }

    #[test]
    fn node_id_is_encoded() {
        let a = Snowflake::new(1).next_id();
        let b = Snowflake::new(2).next_id();
        let node_a = (a as u64 >> NODE_SHIFT) & NODE_MASK;
        let node_b = (b as u64 >> NODE_SHIFT) & NODE_MASK;
        assert_eq!(node_a, 1);
        assert_eq!(node_b, 2);
    }
}

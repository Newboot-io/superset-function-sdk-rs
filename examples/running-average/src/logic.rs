//! Bounded sliding-window running average per variable.
//!
//! State lives in linear memory and resets on crash, broker restart, or deploy.
//! Key length, tracked series count, and window length are all capped to stay well inside 16 MiB.

use std::collections::{BTreeMap, VecDeque};

/// Hard cap on each tracked key's byte length.
pub const MAX_KEY_BYTES: usize = 128;

/// Hard cap on distinct variable IDs tracked in memory.
pub const MAX_SERIES: usize = 256;

/// Hard cap on sliding-window length.
pub const MAX_WINDOW: usize = 64;

/// Bounded per-variable sliding-window state with FIFO eviction.
#[derive(Default)]
pub struct WindowTracker {
  order: VecDeque<String>,
  series: BTreeMap<String, VecDeque<f64>>,
}

impl WindowTracker {
  /// Record `value` for `key` with window size `window` and return the new mean.
  pub fn push(&mut self, key: &str, value: f64, window: usize) -> f64 {
    let bounded_key = truncate_key(key);
    let cap = window.clamp(1, MAX_WINDOW);
    if !self.series.contains_key(bounded_key) {
      while self.series.len() >= MAX_SERIES {
        if let Some(oldest) = self.order.pop_front() {
          self.series.remove(&oldest);
        } else {
          break;
        }
      }
      self.order.push_back(bounded_key.to_string());
    }
    let buf = self
      .series
      .entry(bounded_key.to_string())
      .or_insert_with(|| VecDeque::with_capacity(cap));
    if buf.len() >= cap {
      buf.pop_front();
    }
    if value.is_finite() {
      buf.push_back(value);
    }
    if buf.is_empty() {
      return 0.0;
    }
    let sum: f64 = buf.iter().sum();
    let mean = sum / buf.len() as f64;
    if !mean.is_finite() {
      return 0.0;
    }
    (mean * 1000.0).round() / 1000.0
  }
}

fn truncate_key(raw: &str) -> &str {
  if raw.len() <= MAX_KEY_BYTES {
    return raw;
  }
  let mut end = MAX_KEY_BYTES;
  while end > 0 && !raw.is_char_boundary(end) {
    end -= 1;
  }
  &raw[..end]
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn computes_bounded_sliding_mean() {
    let mut tracker = WindowTracker::default();
    assert_eq!(tracker.push("v1", 10.0, 2), 10.0);
    assert_eq!(tracker.push("v1", 20.0, 2), 15.0);
    assert_eq!(tracker.push("v1", 30.0, 2), 25.0);
  }

  #[test]
  fn caps_key_length_and_series_count() {
    let mut tracker = WindowTracker::default();
    let huge = "a".repeat(10_000);
    assert_eq!(tracker.push(&huge, 5.0, 4), 5.0);
    assert!(tracker.series.keys().all(|k| k.len() <= MAX_KEY_BYTES));
  }
}

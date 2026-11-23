//! Guest counters over `wasm-graph:processor/metrics@1.1.0`.
//!
//! Newboot MQ exports guest counters under `nbmq_wasm_guest_counter_total` (capped at
//! 32 series per operator, 4 labels per call, 64 bytes per key/value). Keep label
//! cardinality low and never put message values into labels.

#[cfg(not(target_arch = "wasm32"))]
use std::cell::RefCell;

/// A captured counter increment returned by [`crate::test::run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterEntry {
  pub labels: Vec<(String, String)>,
  pub name: String,
  pub value: u64,
}

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
  static CAPTURED: RefCell<Option<Vec<CounterEntry>>> = const { RefCell::new(None) };
}

/// Panic-safe, nesting-safe RAII guard for capturing guest counters in [`crate::test::run`].
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct CaptureGuard {
  previous: Option<Option<Vec<CounterEntry>>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl CaptureGuard {
  pub(crate) fn enter() -> Self {
    let previous = CAPTURED.with(|cell| cell.borrow_mut().replace(Vec::new()));
    Self {
      previous: Some(previous),
    }
  }

  pub(crate) fn finish(mut self) -> Vec<CounterEntry> {
    let prev = self.previous.take().flatten();
    CAPTURED.with(|cell| std::mem::replace(&mut *cell.borrow_mut(), prev).unwrap_or_default())
  }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for CaptureGuard {
  fn drop(&mut self) {
    if let Some(prev) = self.previous.take() {
      CAPTURED.with(|cell| {
        *cell.borrow_mut() = prev;
      });
    }
  }
}

/// Increment a guest counter by 1 with no labels.
pub fn increment(name: &str) {
  add(name, 1, &[]);
}

/// Add `value` to a guest counter with optional `(key, value)` labels.
pub fn add(name: &str, value: u64, labels: &[(&str, &str)]) {
  #[cfg(target_arch = "wasm32")]
  {
    crate::__private::wit_add_counter(name, value, labels);
  }
  #[cfg(not(target_arch = "wasm32"))]
  {
    CAPTURED.with(|cell| {
      if let Some(entries) = cell.borrow_mut().as_mut() {
        entries.push(CounterEntry {
          labels: labels
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
          name: name.to_string(),
          value,
        });
      }
    });
  }
}

//! Host-stamped guest logging over `wasm-graph:processor/logger@1.1.0`.
//!
//! Never log raw message content: function logs leave the plant with the cluster's
//! other logs and are rate-limited to 10 lines/s (burst 50, 1 KiB per line).

#[cfg(not(target_arch = "wasm32"))]
use std::cell::RefCell;

/// Guest log severity matching `wasm-graph:processor/logger@1.1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
  Debug,
  Error,
  Info,
  Trace,
  Warn,
}

impl Level {
  /// Lowercase severity name.
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Debug => "debug",
      Self::Error => "error",
      Self::Info => "info",
      Self::Trace => "trace",
      Self::Warn => "warn",
    }
  }
}

/// A captured log line returned by [`crate::test::run`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
  pub context: String,
  pub level: Level,
  pub message: String,
}

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
  static CAPTURED: RefCell<Option<Vec<LogEntry>>> = const { RefCell::new(None) };
}

/// Panic-safe, nesting-safe RAII guard for capturing guest logs in [`crate::test::run`].
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct CaptureGuard {
  previous: Option<Option<Vec<LogEntry>>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl CaptureGuard {
  pub(crate) fn enter() -> Self {
    let previous = CAPTURED.with(|cell| cell.borrow_mut().replace(Vec::new()));
    Self {
      previous: Some(previous),
    }
  }

  pub(crate) fn finish(mut self) -> Vec<LogEntry> {
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

/// Emit a structured guest log line.
pub fn emit(level: Level, context: &str, message: &str) {
  #[cfg(target_arch = "wasm32")]
  {
    crate::__private::wit_log(level, context, message);
  }
  #[cfg(not(target_arch = "wasm32"))]
  {
    CAPTURED.with(|cell| {
      if let Some(entries) = cell.borrow_mut().as_mut() {
        entries.push(LogEntry {
          context: context.to_string(),
          level,
          message: message.to_string(),
        });
      }
    });
  }
}

/// Emit a `trace`-level guest log line.
#[macro_export]
macro_rules! log_trace {
  ($($arg:tt)*) => {
    $crate::log::emit(
      $crate::log::Level::Trace,
      ::core::module_path!(),
      &::std::format!($($arg)*),
    )
  };
}

/// Emit a `debug`-level guest log line.
#[macro_export]
macro_rules! log_debug {
  ($($arg:tt)*) => {
    $crate::log::emit(
      $crate::log::Level::Debug,
      ::core::module_path!(),
      &::std::format!($($arg)*),
    )
  };
}

/// Emit an `info`-level guest log line.
#[macro_export]
macro_rules! log_info {
  ($($arg:tt)*) => {
    $crate::log::emit(
      $crate::log::Level::Info,
      ::core::module_path!(),
      &::std::format!($($arg)*),
    )
  };
}

/// Emit a `warn`-level guest log line.
#[macro_export]
macro_rules! log_warn {
  ($($arg:tt)*) => {
    $crate::log::emit(
      $crate::log::Level::Warn,
      ::core::module_path!(),
      &::std::format!($($arg)*),
    )
  };
}

/// Emit an `error`-level guest log line.
#[macro_export]
macro_rules! log_error {
  ($($arg:tt)*) => {
    $crate::log::emit(
      $crate::log::Level::Error,
      ::core::module_path!(),
      &::std::format!($($arg)*),
    )
  };
}

pub use crate::{
  log_debug as debug, log_error as error, log_info as info, log_trace as trace, log_warn as warn,
};

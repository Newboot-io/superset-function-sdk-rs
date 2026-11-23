//! Native test runner: run `#[map]` and `#[filter]` functions on sample messages
//! without compiling to WebAssembly.

use crate::error::Result;
use crate::log::{self, LogEntry};
use crate::message::Message;
use crate::metrics::{self, CounterEntry};
use crate::settings::Settings;
use crate::Call;

/// Parameters for [`run`] (fields in alphabetical order).
pub struct Run<'a, F> {
  pub function: F,
  pub message: Message,
  pub settings: &'a [(&'a str, &'a str)],
}

/// Output of a single function invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestOutcome {
  /// A `#[filter]` function returned `Ok(false)` and dropped the message.
  Dropped,
  /// A `#[map]` returned a new message or a `#[filter]` returned `Ok(true)`.
  Output(Message),
}

impl TestOutcome {
  /// Return the output [`Message`] if the message was not dropped.
  pub fn into_message(self) -> Option<Message> {
    match self {
      Self::Dropped => None,
      Self::Output(m) => Some(m),
    }
  }
}

/// Full result of [`run`], including captured guest logs and counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestRun {
  pub counters: Vec<CounterEntry>,
  pub logs: Vec<LogEntry>,
  pub outcome: TestOutcome,
}

/// Return type supported by [`run`]: [`Message`] for `#[map]` and `bool` for `#[filter]`.
pub trait TestOutput: Sized {
  fn into_outcome(self, original: Message) -> TestOutcome;
}

impl TestOutput for Message {
  fn into_outcome(self, _original: Message) -> TestOutcome {
    TestOutcome::Output(self)
  }
}

impl TestOutput for bool {
  fn into_outcome(self, original: Message) -> TestOutcome {
    if self {
      TestOutcome::Output(original)
    } else {
      TestOutcome::Dropped
    }
  }
}

/// Run a `#[map]` or `#[filter]` function natively against a sample [`Message`].
///
/// Parses `settings` via [`Settings::from_properties`], captures any `newboot::log::*!`
/// lines and `newboot::metrics::*` increments emitted during the call (panic-safe and
/// nesting-safe), and returns a [`TestRun`].
pub fn run<S, O, F>(params: Run<'_, F>) -> Result<TestRun>
where
  S: Settings,
  O: TestOutput,
  F: FnOnce(Call<S>) -> Result<O>,
{
  let props: Vec<(String, String)> = params
    .settings
    .iter()
    .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
    .collect();
  let settings = S::from_properties(&props)?;
  let original = params.message.clone();
  let log_guard = log::CaptureGuard::enter();
  let metrics_guard = metrics::CaptureGuard::enter();
  let result = (params.function)(Call {
    message: params.message,
    settings,
  });
  let logs = log_guard.finish();
  let counters = metrics_guard.finish();
  let outcome = result?.into_outcome(original);
  Ok(TestRun {
    counters,
    logs,
    outcome,
  })
}

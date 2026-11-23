//! Error type for Newboot MQ Functions.

use std::fmt;

/// Convenience result alias used by `#[map]` and `#[filter]` functions.
pub type Result<T> = std::result::Result<T, Error>;

/// Error returned by a Newboot MQ Function or the SDK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
  message: String,
}

impl Error {
  /// Create a new error with a short diagnostic message.
  ///
  /// Never include raw message payloads or secret values in error messages:
  /// error reasons may be surfaced in cluster metrics and logs.
  pub fn new(message: impl Into<String>) -> Self {
    Self {
      message: message.into(),
    }
  }

  /// Create an error for a missing required setting.
  pub fn missing_setting(key: &str) -> Self {
    Self::new(format!("missing required setting `{key}`"))
  }

  /// Create an error for a setting whose value failed to parse.
  pub fn invalid_setting(key: &str, detail: &str) -> Self {
    Self::new(format!("invalid setting `{key}`: {detail}"))
  }

  /// Return the error message.
  pub fn message(&self) -> &str {
    &self.message
  }
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.message)
  }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
  fn from(err: serde_json::Error) -> Self {
    // Format only category and position — never quote offending payload content.
    Self::new(format!(
      "invalid JSON ({:?} at line {}, column {})",
      err.classify(),
      err.line(),
      err.column()
    ))
  }
}

impl From<std::io::Error> for Error {
  fn from(err: std::io::Error) -> Self {
    Self::new(format!("I/O error: {}", err.kind()))
  }
}

impl From<String> for Error {
  fn from(message: String) -> Self {
    Self { message }
  }
}

impl From<&str> for Error {
  fn from(message: &str) -> Self {
    Self::new(message)
  }
}

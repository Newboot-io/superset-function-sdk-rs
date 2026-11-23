//! Route settings parsing and metadata for `function.toml` generation.

use crate::error::{Error, Result};

/// Value type declared for a setting in `function.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKind {
  Boolean,
  Integer,
  Number,
  String,
}

impl SettingKind {
  /// Return the `type` string written to `function.toml`.
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Boolean => "boolean",
      Self::Integer => "integer",
      Self::Number => "number",
      Self::String => "string",
    }
  }
}

/// Metadata describing one setting declared by `#[derive(Settings)]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingSpec {
  pub default: Option<&'static str>,
  pub description: &'static str,
  pub key: &'static str,
  pub kind: SettingKind,
  pub required: bool,
}

/// Settings struct parsed from the route's string key-value properties during `init`.
///
/// Derive this trait with `#[derive(newboot::Settings)]`. Never store secrets in settings:
/// settings are stored in plain text on the route and shown in the console.
pub trait Settings: Sized {
  /// Static specification of every setting declared by this struct (used to generate `function.toml`).
  fn specs() -> &'static [SettingSpec];

  /// Parse settings from the `(key, value)` pairs supplied by Newboot MQ at `init`.
  fn from_properties(properties: &[(String, String)]) -> Result<Self>;

  /// Clone the parsed settings for a single `Call`.
  fn clone_for_call(&self) -> Self;
}

/// Empty settings type for functions that take no route settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoSettings;

impl Settings for NoSettings {
  fn specs() -> &'static [SettingSpec] {
    &[]
  }

  fn from_properties(_properties: &[(String, String)]) -> Result<Self> {
    Ok(Self)
  }

  fn clone_for_call(&self) -> Self {
    Self
  }
}

/// Look up the last value for `key` in a property list.
pub fn find_property<'a>(properties: &'a [(String, String)], key: &str) -> Option<&'a str> {
  properties
    .iter()
    .rev()
    .find(|(k, _)| k == key)
    .map(|(_, v)| v.as_str())
}

/// Parse a finite `f64` setting.
pub fn parse_f64(key: &str, raw: &str) -> Result<f64> {
  let v: f64 = raw
    .trim()
    .parse()
    .map_err(|_| Error::invalid_setting(key, "expected a number"))?;
  if !v.is_finite() {
    return Err(Error::invalid_setting(key, "expected a finite number"));
  }
  Ok(v)
}

/// Parse a finite `f32` setting.
pub fn parse_f32(key: &str, raw: &str) -> Result<f32> {
  let v: f32 = raw
    .trim()
    .parse()
    .map_err(|_| Error::invalid_setting(key, "expected a number"))?;
  if !v.is_finite() {
    return Err(Error::invalid_setting(key, "expected a finite number"));
  }
  Ok(v)
}

/// Parse an `i64` setting.
pub fn parse_i64(key: &str, raw: &str) -> Result<i64> {
  raw
    .trim()
    .parse()
    .map_err(|_| Error::invalid_setting(key, "expected an integer"))
}

/// Parse a `u64` setting.
pub fn parse_u64(key: &str, raw: &str) -> Result<u64> {
  raw
    .trim()
    .parse()
    .map_err(|_| Error::invalid_setting(key, "expected a non-negative integer"))
}

/// Parse an `i32` setting.
pub fn parse_i32(key: &str, raw: &str) -> Result<i32> {
  raw
    .trim()
    .parse()
    .map_err(|_| Error::invalid_setting(key, "expected a 32-bit integer"))
}

/// Parse a `u32` setting.
pub fn parse_u32(key: &str, raw: &str) -> Result<u32> {
  raw
    .trim()
    .parse()
    .map_err(|_| Error::invalid_setting(key, "expected a non-negative 32-bit integer"))
}

/// Parse a `usize` setting.
pub fn parse_usize(key: &str, raw: &str) -> Result<usize> {
  raw
    .trim()
    .parse()
    .map_err(|_| Error::invalid_setting(key, "expected a non-negative integer"))
}

/// Parse a boolean setting (`true` / `false`).
pub fn parse_bool(key: &str, raw: &str) -> Result<bool> {
  match raw.trim() {
    "true" => Ok(true),
    "false" => Ok(false),
    _ => Err(Error::invalid_setting(key, "expected `true` or `false`")),
  }
}

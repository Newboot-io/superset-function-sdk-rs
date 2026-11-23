//! `function.toml` manifest rendering and verification.

use crate::error::{Error, Result};
use crate::settings::SettingSpec;
use std::ffi::OsStr;
use std::fmt::Write;
use std::path::Path;

/// Whether a function runs as a `map` or a `filter` step on a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
  Filter,
  Map,
}

impl FunctionKind {
  /// Return the `runs_as` string written to `function.toml`.
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Filter => "filter",
      Self::Map => "map",
    }
  }
}

/// Manifest metadata rendered into `function.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionManifest {
  pub description: String,
  pub kind: FunctionKind,
  pub name: String,
  pub settings: &'static [SettingSpec],
  pub version: String,
}

impl FunctionManifest {
  /// Render this manifest as a deterministic `function.toml` document.
  pub fn to_toml(&self) -> String {
    render_function_toml(self)
  }

  /// Verify that `path` matches [`Self::to_toml`], or write it when `UPDATE_FUNCTION_TOML=1`.
  pub fn check_file(&self, path: impl AsRef<Path>) -> Result<()> {
    let update = std::env::var_os("UPDATE_FUNCTION_TOML").as_deref() == Some(OsStr::new("1"));
    self.check_file_with_update(path, update)
  }

  /// Verify `path` against [`Self::to_toml`], or overwrite it when `update` is `true`.
  pub fn check_file_with_update(&self, path: impl AsRef<Path>, update: bool) -> Result<()> {
    let expected = self.to_toml();
    let path = path.as_ref();
    if update {
      std::fs::write(path, &expected)?;
      return Ok(());
    }
    let actual = std::fs::read_to_string(path)?;
    if actual != expected {
      return Err(Error::new(format!(
        "{} is out of date with #[derive(Settings)]; re-run with UPDATE_FUNCTION_TOML=1 cargo test",
        path.display()
      )));
    }
    Ok(())
  }
}

/// Render a [`FunctionManifest`] to `function.toml`.
pub fn render_function_toml(manifest: &FunctionManifest) -> String {
  let mut out = String::new();
  let _ = writeln!(
    out,
    "# Generated from #[derive(Settings)]. Do not edit by hand."
  );
  let _ = writeln!(out, "name = {}", toml_quote(&manifest.name));
  let _ = writeln!(out, "version = {}", toml_quote(&manifest.version));
  let _ = writeln!(out, "runs_as = {}", toml_quote(manifest.kind.as_str()));
  if !manifest.description.is_empty() {
    let _ = writeln!(out, "description = {}", toml_quote(&manifest.description));
  }
  for spec in manifest.settings {
    let _ = writeln!(out);
    let _ = writeln!(out, "[[settings]]");
    let _ = writeln!(out, "key = {}", toml_quote(spec.key));
    let _ = writeln!(out, "type = {}", toml_quote(spec.kind.as_str()));
    let _ = writeln!(out, "required = {}", spec.required);
    if let Some(default) = spec.default {
      let _ = writeln!(out, "default = {}", toml_quote(default));
    }
    if !spec.description.is_empty() {
      let _ = writeln!(out, "description = {}", toml_quote(spec.description));
    }
  }
  out
}

fn toml_quote(s: &str) -> String {
  let mut out = String::with_capacity(s.len() + 2);
  out.push('"');
  for ch in s.chars() {
    match ch {
      '\\' => out.push_str("\\\\"),
      '"' => out.push_str("\\\""),
      '\x08' => out.push_str("\\b"),
      '\x0c' => out.push_str("\\f"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      c if (c as u32) < 0x20 || c == '\x7f' => {
        let _ = write!(out, "\\u{:04X}", c as u32);
      }
      c => out.push(c),
    }
  }
  out.push('"');
  out
}

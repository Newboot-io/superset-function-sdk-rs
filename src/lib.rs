//! Rust SDK for Newboot MQ Functions over `wasm-graph:processor@1.1.0`.
//!
//! Annotate a function with [`map`] or [`filter`], declare its route settings with
//! `#[derive(Settings)]`, and test it natively on sample files with [`test::run`].
//!
//! ```rust
//! use newboot::{map, Call, Message, Settings};
//!
//! #[derive(Settings)]
//! struct Scale {
//!   /// Multiplier applied to `value`.
//!   factor: f64,
//! }
//!
//! #[map]
//! fn scale_value(Call { message, settings }: Call<Scale>) -> newboot::Result<Message> {
//!   let mut doc = message.json()?;
//!   if let Some(v) = doc["value"].as_f64() {
//!     doc["value"] = serde_json::json!(v * settings.factor);
//!   }
//!   Ok(message.with_json(doc))
//! }
//!
//! let run = newboot::test::run(newboot::test::Run {
//!   function: scale_value,
//!   message: Message::from_json("sensors/t1", serde_json::json!({"value": 2.0})),
//!   settings: &[("factor", "3.0")],
//! })
//! .unwrap();
//! let out = run.outcome.into_message().unwrap();
//! assert_eq!(out.json().unwrap()["value"], 6.0);
//! ```

pub mod error;
pub mod log;
pub mod manifest;
pub mod message;
pub mod metrics;
pub mod settings;
#[cfg(not(target_arch = "wasm32"))]
pub mod test;

#[cfg(target_arch = "wasm32")]
#[doc(hidden)]
pub mod __private {
  pub use crate::wasm::*;
}

#[cfg(target_arch = "wasm32")]
mod wasm;

pub use crate::error::{Error, Result};
pub use crate::manifest::{render_function_toml, FunctionKind, FunctionManifest};
pub use crate::message::{Message, Telemetry, DEFAULT_SAMPLE_TOPIC, JSON_CONTENT_TYPE};
pub use crate::settings::{NoSettings, SettingKind, SettingSpec, Settings};
pub use serde_json;
pub use superset_function_sdk_macros::{filter, map, Settings};

/// Input to every `#[map]` and `#[filter]` function (fields in alphabetical order).
#[derive(Debug, Clone, PartialEq)]
pub struct Call<S = NoSettings> {
  pub message: Message,
  pub settings: S,
}

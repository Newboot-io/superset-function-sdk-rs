//! Richer example: declarative JSON rule mapper adapted from Newboot's mapper operator.
//!
//! Takes a `rules` JSON string setting (`topic` UNS template + `payload` convert/rename/set/keep/drop)
//! and reshapes each message deterministically. Compiled rules are cached across calls.

pub mod rules;

use newboot::{map, Call, Error, Message, Settings};
use std::sync::Mutex;

static CACHE: Mutex<Option<(String, rules::CompiledMapper)>> = Mutex::new(None);

#[derive(Settings)]
pub struct RuleMapperSettings {
  /// JSON rule document configuring topic rewriting and payload reshaping
  #[setting(default = "{\"version\":1}")]
  pub rules: String,
}

#[map]
pub fn rule_mapper(
  Call { message, settings }: Call<RuleMapperSettings>,
) -> newboot::Result<Message> {
  let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
  let needs_compile = guard
    .as_ref()
    .is_none_or(|(cached_raw, _)| cached_raw != &settings.rules);
  if needs_compile {
    let compiled = rules::CompiledMapper::from_properties(&[(
      rules::RULES_KEY.to_string(),
      settings.rules.clone(),
    )])
    .map_err(Error::new)?;
    *guard = Some((settings.rules, compiled));
  }
  let (_, compiled) = guard.as_ref().unwrap();
  let mapped = compiled
    .apply(message.topic(), message.payload())
    .map_err(Error::new)?;
  let mut out = message
    .with_topic(mapped.topic)
    .with_payload(mapped.payload);
  if mapped.is_json {
    out = out.with_content_type(newboot::JSON_CONTENT_TYPE);
  }
  newboot::metrics::increment("mapped");
  Ok(out)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn applies_declarative_rules_via_sdk() {
    let cfg = serde_json::json!({
      "version": 1,
      "topic": {
        "template": "acme/{site}/{payload.variableId}",
        "vars": { "site": "lyon" }
      },
      "payload": {
        "passthrough": true,
        "rename": [{ "from": "value", "to": "reading" }],
        "set": [{ "field": "unit", "value": "ratio" }]
      }
    });

    let run = newboot::test::run(newboot::test::Run {
      function: rule_mapper,
      message: Message::from_json(
        "output/telemetry/t/n/m/v",
        serde_json::json!({"variableId": "corrosion_camera_1", "value": 0.91}),
      ),
      settings: &[("rules", &cfg.to_string())],
    })
    .unwrap();
    let out = run.outcome.into_message().unwrap();
    assert_eq!(out.topic(), "acme/lyon/corrosion_camera_1");
    let doc = out.json().unwrap();
    assert_eq!(doc["reading"], 0.91);
    assert_eq!(doc["unit"], "ratio");
  }

  #[test]
  fn function_toml_matches_settings() {
    rule_mapper_manifest(
      "rule-mapper",
      env!("CARGO_PKG_VERSION"),
      "Declarative JSON rule mapper for topics and payloads",
    )
    .check_file("function.toml")
    .unwrap();
  }
}

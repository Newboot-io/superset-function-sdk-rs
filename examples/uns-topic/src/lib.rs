//! Example: rewrite the message topic into an ISA-95 Unified Namespace (UNS) path.

pub mod logic;

use newboot::{map, Call, Message, Settings};

#[derive(Settings)]
pub struct UnsSettings {
  /// Plant area segment
  #[setting(default = "area-1")]
  pub area: String,
  /// Enterprise segment
  #[setting(default = "acme")]
  pub enterprise: String,
  /// Production line segment
  #[setting(default = "line-1")]
  pub line: String,
  /// Plant site segment
  #[setting(default = "plant-1")]
  pub site: String,
}

#[map]
pub fn uns_topic(Call { message, settings }: Call<UnsSettings>) -> newboot::Result<Message> {
  let doc = message.json()?;
  let variable = doc["variableId"]
    .as_str()
    .or_else(|| doc["name"].as_str())
    .unwrap_or("unknown");
  let topic = logic::build_topic(logic::UnsParts {
    area: &settings.area,
    enterprise: &settings.enterprise,
    line: &settings.line,
    site: &settings.site,
    variable,
  });
  newboot::metrics::increment("rewritten");
  Ok(message.with_topic(topic))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rewrites_topic_from_variable_id() {
    let run = newboot::test::run(newboot::test::Run {
      function: uns_topic,
      message: Message::from_json(
        "output/telemetry/t/n/m/v",
        serde_json::json!({"variableId": "corrosion_camera_1", "value": 0.91}),
      ),
      settings: &[
        ("enterprise", "acme"),
        ("site", "lyon"),
        ("area", "paint"),
        ("line", "line-2"),
      ],
    })
    .unwrap();
    let out = run.outcome.into_message().unwrap();
    assert_eq!(out.topic(), "acme/lyon/paint/line-2/corrosion_camera_1");
  }

  #[test]
  fn function_toml_matches_settings() {
    uns_topic_manifest(
      "uns-topic",
      env!("CARGO_PKG_VERSION"),
      "Build an ISA-95 Unified Namespace topic from route settings and variableId",
    )
    .check_file("function.toml")
    .unwrap();
  }
}

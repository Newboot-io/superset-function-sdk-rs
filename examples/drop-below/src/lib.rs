//! Example: `#[filter]` that drops telemetry readings below `minValue`.

pub mod logic;

use newboot::{filter, Call, Error, Settings};

#[derive(Settings)]
pub struct MinSettings {
  /// JSON field holding the numeric value
  #[setting(default = "value")]
  pub field: String,
  /// Minimum numeric value to keep (inclusive)
  #[setting(default = "0.5")]
  pub min_value: f64,
}

#[filter]
pub fn drop_below(Call { message, settings }: Call<MinSettings>) -> newboot::Result<bool> {
  let doc = message.json()?;
  let value = doc[&settings.field]
    .as_f64()
    .ok_or_else(|| Error::new("missing numeric field"))?;
  let keep = logic::meets_minimum(logic::MinimumCheck {
    min_value: settings.min_value,
    value,
  });
  if keep {
    newboot::metrics::increment("kept");
  } else {
    newboot::metrics::increment("dropped");
  }
  Ok(keep)
}

#[cfg(test)]
mod tests {
  use super::*;
  use newboot::Message;

  #[test]
  fn keeps_above_and_drops_below() {
    let kept = newboot::test::run(newboot::test::Run {
      function: drop_below,
      message: Message::from_json("t", serde_json::json!({"value": 0.91})),
      settings: &[("minValue", "0.5")],
    })
    .unwrap();
    assert!(kept.outcome.into_message().is_some());

    let dropped = newboot::test::run(newboot::test::Run {
      function: drop_below,
      message: Message::from_json("t", serde_json::json!({"value": 0.12})),
      settings: &[("minValue", "0.5")],
    })
    .unwrap();
    assert_eq!(dropped.outcome, newboot::test::TestOutcome::Dropped);
  }

  #[test]
  fn function_toml_matches_settings() {
    drop_below_manifest(
      "drop-below",
      env!("CARGO_PKG_VERSION"),
      "Drop messages whose numeric value is below minValue",
    )
    .check_file("function.toml")
    .unwrap();
  }
}

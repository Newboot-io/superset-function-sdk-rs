//! Starter Newboot MQ Function: weigh corrosion readings by zone and add `riskScore`.

pub mod logic;

use newboot::{map, Call, Error, Message, Settings};

#[derive(Settings)]
pub struct Weights {
  /// zone:weight pairs, for example "z1:0.5,z3:1.2"
  #[setting(default = "z1:0.5,z3:1.2")]
  pub zone_weights: String,
}

#[map]
pub fn corrosion_risk(Call { message, settings }: Call<Weights>) -> newboot::Result<Message> {
  let mut doc = message.json()?;
  let zone = doc["zone"]
    .as_str()
    .ok_or_else(|| Error::new("missing field zone"))?;
  let corrosion_pct = doc["value"]
    .as_f64()
    .or_else(|| doc["corrosionPct"].as_f64())
    .ok_or_else(|| Error::new("missing numeric field value"))?;
  let w = logic::weight(logic::ZoneWeight {
    weights: &settings.zone_weights,
    zone,
  });
  let score = logic::risk(logic::Risk {
    corrosion_pct,
    weight: w,
  });
  doc["riskScore"] = serde_json::json!(score);
  newboot::log::debug!("zone weight {w}");
  newboot::metrics::increment("scored");
  Ok(message.with_json(doc))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn scores_sample_message() {
    let run = newboot::test::run(newboot::test::Run {
      function: corrosion_risk,
      message: Message::from_sample_file("samples/in.json").unwrap(),
      settings: &[("zoneWeights", "z1:0.5,z3:1.2")],
    })
    .unwrap();
    let out = run.outcome.into_message().unwrap();
    assert_eq!(out.json().unwrap()["riskScore"], 1.09);
    assert_eq!(run.logs.len(), 1);
    assert_eq!(run.counters.len(), 1);
  }

  #[test]
  fn fails_on_missing_zone_sample() {
    let err = newboot::test::run(newboot::test::Run {
      function: corrosion_risk,
      message: Message::from_sample_file("samples/invalid/no-zone.json").unwrap(),
      settings: &[("zoneWeights", "z1:0.5,z3:1.2")],
    })
    .unwrap_err();
    assert!(err.message().contains("missing field zone"));
  }

  #[test]
  fn function_toml_matches_settings() {
    corrosion_risk_manifest(
      "corrosion-risk",
      env!("CARGO_PKG_VERSION"),
      "Weigh corrosion readings by zone and add riskScore",
    )
    .check_file("function.toml")
    .unwrap();
  }
}

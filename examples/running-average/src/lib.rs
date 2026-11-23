//! Example: stateful bounded sliding-window running average per `variableId`.

pub mod logic;

use newboot::{map, Call, Error, Message, Settings};
use std::sync::Mutex;

static TRACKER: Mutex<Option<logic::WindowTracker>> = Mutex::new(None);

#[derive(Settings)]
pub struct AverageSettings {
  /// Number of recent readings averaged per variable (1..=64)
  #[setting(default = "5")]
  pub window_size: usize,
}

#[map]
pub fn running_average(
  Call { message, settings }: Call<AverageSettings>,
) -> newboot::Result<Message> {
  let mut doc = message.json()?;
  let key = doc["variableId"].as_str().unwrap_or("default").to_string();
  let value = doc["value"]
    .as_f64()
    .ok_or_else(|| Error::new("missing numeric field value"))?;
  let mean = {
    let mut guard = TRACKER.lock().unwrap_or_else(|e| e.into_inner());
    let tracker = guard.get_or_insert_with(logic::WindowTracker::default);
    tracker.push(&key, value, settings.window_size)
  };
  doc["runningAverage"] = serde_json::json!(mean);
  newboot::metrics::increment("averaged");
  Ok(message.with_json(doc))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn updates_running_average_across_calls() {
    *TRACKER.lock().unwrap() = None;
    let r1 = newboot::test::run(newboot::test::Run {
      function: running_average,
      message: Message::from_json("t", serde_json::json!({"variableId": "v1", "value": 10.0})),
      settings: &[("windowSize", "2")],
    })
    .unwrap();
    assert_eq!(
      r1.outcome.into_message().unwrap().json().unwrap()["runningAverage"],
      10.0
    );

    let r2 = newboot::test::run(newboot::test::Run {
      function: running_average,
      message: Message::from_json("t", serde_json::json!({"variableId": "v1", "value": 20.0})),
      settings: &[("windowSize", "2")],
    })
    .unwrap();
    assert_eq!(
      r2.outcome.into_message().unwrap().json().unwrap()["runningAverage"],
      15.0
    );
  }

  #[test]
  fn function_toml_matches_settings() {
    running_average_manifest(
      "running-average",
      env!("CARGO_PKG_VERSION"),
      "Compute a bounded sliding-window running average per variableId",
    )
    .check_file("function.toml")
    .unwrap();
  }
}

//! Example: rename a JSON field and apply a linear unit conversion (e.g. °C → °F).

pub mod logic;

use newboot::{map, Call, Message, Settings};

#[derive(Settings)]
pub struct RenameConvertSettings {
  /// Decimal places to round to
  #[setting(default = "2")]
  pub decimals: u32,
  /// Source JSON field name to rename
  #[setting(default = "value")]
  pub from_field: String,
  /// Additive offset applied after scaling
  #[setting(default = "0.0")]
  pub offset: f64,
  /// Multiplicative scale factor
  #[setting(default = "1.0")]
  pub scale: f64,
  /// Destination JSON field name
  #[setting(default = "convertedValue")]
  pub to_field: String,
}

#[map]
pub fn rename_convert(
  Call { message, settings }: Call<RenameConvertSettings>,
) -> newboot::Result<Message> {
  let mut doc = message.json()?;
  if logic::reshape(
    &mut doc,
    logic::Reshape {
      decimals: settings.decimals,
      from: &settings.from_field,
      offset: settings.offset,
      scale: settings.scale,
      to: &settings.to_field,
    },
  ) {
    newboot::metrics::increment("converted");
  }
  Ok(message.with_json(doc))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn renames_and_scales_sample() {
    let run = newboot::test::run(newboot::test::Run {
      function: rename_convert,
      message: Message::from_json("sensors/temp", serde_json::json!({"temp_c": 20.0})),
      settings: &[
        ("fromField", "temp_c"),
        ("toField", "tempF"),
        ("scale", "1.8"),
        ("offset", "32.0"),
        ("decimals", "1"),
      ],
    })
    .unwrap();
    let out = run.outcome.into_message().unwrap();
    assert_eq!(out.json().unwrap()["tempF"], 68.0);
  }

  #[test]
  fn function_toml_matches_settings() {
    rename_convert_manifest(
      "rename-convert",
      env!("CARGO_PKG_VERSION"),
      "Rename a JSON field and apply a linear unit conversion",
    )
    .check_file("function.toml")
    .unwrap();
  }
}

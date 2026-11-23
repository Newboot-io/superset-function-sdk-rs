//! Plain logic for `rename-convert`.

use serde_json::{json, Value};

/// Named parameters for [`convert_value`] (alphabetical fields).
pub struct Convert {
  pub decimals: u32,
  pub offset: f64,
  pub scale: f64,
  pub value: f64,
}

/// Named parameters for [`reshape`] (alphabetical fields).
pub struct Reshape<'a> {
  pub decimals: u32,
  pub from: &'a str,
  pub offset: f64,
  pub scale: f64,
  pub to: &'a str,
}

/// Apply `value * scale + offset` and round to `decimals` places.
pub fn convert_value(params: Convert) -> f64 {
  let factor = 10_f64.powi(params.decimals.min(9) as i32);
  let scaled = params.value * params.scale + params.offset;
  if !scaled.is_finite() {
    return 0.0;
  }
  (scaled * factor).round() / factor
}

/// Rename `params.from` to `params.to` in a JSON object and convert numeric values.
pub fn reshape(doc: &mut Value, params: Reshape<'_>) -> bool {
  let Some(obj) = doc.as_object_mut() else {
    return false;
  };
  let Some(raw) = obj.remove(params.from) else {
    return false;
  };
  let next = match raw.as_f64() {
    Some(v) => json!(convert_value(Convert {
      decimals: params.decimals,
      offset: params.offset,
      scale: params.scale,
      value: v,
    })),
    None => raw,
  };
  obj.insert(params.to.to_string(), next);
  true
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn converts_celsius_to_fahrenheit() {
    let mut doc = json!({"temp_c": 25.0, "unit": "C"});
    assert!(reshape(
      &mut doc,
      Reshape {
        decimals: 1,
        from: "temp_c",
        offset: 32.0,
        scale: 1.8,
        to: "tempF",
      }
    ));
    assert_eq!(doc["tempF"], 77.0);
    assert!(doc.get("temp_c").is_none());
  }
}

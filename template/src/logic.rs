//! Plain, unit-tested logic for the starter `corrosion-risk` function.

/// Named parameters for [`weight`] (alphabetical fields).
pub struct ZoneWeight<'a> {
  pub weights: &'a str,
  pub zone: &'a str,
}

/// Named parameters for [`risk`] (alphabetical fields).
pub struct Risk {
  pub corrosion_pct: f64,
  pub weight: f64,
}

/// Look up `zone` in a comma-separated `"z1:0.5,z3:1.2"` table; defaults to `1.0`.
pub fn weight(params: ZoneWeight<'_>) -> f64 {
  for pair in params.weights.split(',') {
    let Some((k, v)) = pair.split_once(':') else {
      continue;
    };
    if k.trim() == params.zone {
      if let Ok(parsed) = v.trim().parse::<f64>() {
        if parsed.is_finite() {
          return parsed;
        }
      }
    }
  }
  1.0
}

/// Compute a rounded risk score (`corrosion_pct * weight`, 2 decimal places).
pub fn risk(params: Risk) -> f64 {
  let raw = params.corrosion_pct * params.weight;
  if !raw.is_finite() {
    return 0.0;
  }
  (raw * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn looks_up_zone_weight_with_fallback() {
    assert_eq!(
      weight(ZoneWeight {
        weights: "z1:0.5,z3:1.2",
        zone: "z3",
      }),
      1.2
    );
    assert_eq!(
      weight(ZoneWeight {
        weights: "z1:0.5,z3:1.2",
        zone: "z9",
      }),
      1.0
    );
  }

  #[test]
  fn computes_rounded_risk_score() {
    assert_eq!(
      risk(Risk {
        corrosion_pct: 0.91,
        weight: 1.2,
      }),
      1.09
    );
  }
}

//! Plain logic for `drop-below`.

/// Named parameters for [`meets_minimum`] (alphabetical fields).
pub struct MinimumCheck {
  pub min_value: f64,
  pub value: f64,
}

/// Return `true` when `value` is finite and at least `min_value`.
pub fn meets_minimum(check: MinimumCheck) -> bool {
  check.value.is_finite() && check.value >= check.min_value
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn keeps_at_or_above_minimum() {
    assert!(meets_minimum(MinimumCheck {
      min_value: 0.5,
      value: 0.5,
    }));
    assert!(!meets_minimum(MinimumCheck {
      min_value: 0.5,
      value: 0.49,
    }));
  }
}

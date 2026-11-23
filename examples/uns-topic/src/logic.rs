//! Plain logic for building an ISA-95 Unified Namespace (UNS) topic.

/// Named parameters for [`build_topic`] (alphabetical fields).
pub struct UnsParts<'a> {
  pub area: &'a str,
  pub enterprise: &'a str,
  pub line: &'a str,
  pub site: &'a str,
  pub variable: &'a str,
}

/// Sanitize one MQTT topic segment (strip wildcards and slashes).
pub fn sanitize_segment(raw: &str) -> String {
  let clean: String = raw
    .trim()
    .chars()
    .map(|c| match c {
      '/' | '+' | '#' | '\0' => '_',
      c if c.is_control() => '_',
      c => c,
    })
    .collect();
  if clean.is_empty() {
    "unknown".to_string()
  } else {
    clean
  }
}

/// Maximum output topic length in bytes.
pub const MAX_TOPIC_BYTES: usize = 512;

/// Build an ISA-95 UNS topic: `{enterprise}/{site}/{area}/{line}/{variable}`.
pub fn build_topic(parts: UnsParts<'_>) -> String {
  let mut topic = format!(
    "{}/{}/{}/{}/{}",
    sanitize_segment(parts.enterprise),
    sanitize_segment(parts.site),
    sanitize_segment(parts.area),
    sanitize_segment(parts.line),
    sanitize_segment(parts.variable),
  );
  if topic.len() > MAX_TOPIC_BYTES {
    let mut end = MAX_TOPIC_BYTES;
    while end > 0 && !topic.is_char_boundary(end) {
      end -= 1;
    }
    topic.truncate(end);
  }
  topic
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn builds_clean_uns_topic() {
    let topic = build_topic(UnsParts {
      area: "press",
      enterprise: "acme",
      line: "line-1",
      site: "lyon",
      variable: "temp/1",
    });
    assert_eq!(topic, "acme/lyon/press/line-1/temp_1");
  }
}

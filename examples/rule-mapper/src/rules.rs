//! Declarative rule-driven mapper & filter engine (pure Rust, runs natively in `cargo test` and
//! compiled to `wasm32-wasip2` inside `newboot-mq`).
//!
//! Implements declarative `map` + `filter` rule semantics:
//! - `filter` predicates evaluate against the input JSON payload before transformation (`true` keeps,
//!   `false` drops).
//! - `map` rewrites the topic (ISA-95 UNS templates with `{scope}`, `{site}`, `{cluster}`,
//!   `{module}`, `{topic}`, `{topic.last}`, `{topic.N}`, `{payload.<field>}`) and reshapes the JSON
//!   payload in deterministic order: `convert` -> `rename` -> `set` -> `keep`/`drop`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Configuration property key holding the JSON rule document.
pub const RULES_KEY: &str = "rules";

/// Bounds that keep every `init` and `process` call well within `newboot-mq`'s fuel & memory caps.
pub const MAX_RULES_BYTES: usize = 32 * 1024;
pub const MAX_FILTER_RULES: usize = 32;
pub const MAX_FIELD_RULES: usize = 64;
pub const MAX_TOPIC_VARS: usize = 16;
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
pub const MAX_TOPIC_BYTES: usize = 512;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MapperConfig {
  #[serde(default)]
  pub filter: Vec<FilterRule>,
  #[serde(default)]
  pub payload: Option<PayloadRule>,
  #[serde(default)]
  pub topic: Option<TopicRule>,
  #[serde(default = "default_version")]
  pub version: u8,
}

fn default_version() -> u8 {
  1
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TopicRule {
  pub template: String,
  #[serde(default)]
  pub vars: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterRule {
  pub field: String,
  pub op: FilterOp,
  #[serde(default)]
  pub value: Option<Value>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilterOp {
  Eq,
  Ne,
  Gt,
  Gte,
  Lt,
  Lte,
  Exists,
  Contains,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PayloadRule {
  #[serde(default)]
  pub convert: Vec<ConvertRule>,
  #[serde(default)]
  pub drop: Vec<String>,
  #[serde(default)]
  pub keep: Vec<String>,
  #[serde(default = "default_true")]
  pub passthrough: bool,
  #[serde(default)]
  pub rename: Vec<RenameRule>,
  #[serde(default)]
  pub set: Vec<SetRule>,
}

fn default_true() -> bool {
  true
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConvertRule {
  EpochSToIso8601 {
    field: String,
  },
  EpochMsToIso8601 {
    field: String,
  },
  Scale {
    field: String,
    factor: f64,
    #[serde(default)]
    offset: f64,
    #[serde(default)]
    round: Option<u32>,
  },
  Replace {
    field: String,
    from: String,
    to: String,
  },
  ToString {
    field: String,
  },
  ToNumber {
    field: String,
  },
  ToBool {
    field: String,
  },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RenameRule {
  pub from: String,
  pub to: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SetRule {
  pub field: String,
  pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedMessage {
  pub topic: String,
  pub payload: Vec<u8>,
  pub is_json: bool,
}

#[derive(Debug, Clone)]
pub struct CompiledMapper {
  config: MapperConfig,
}

impl CompiledMapper {
  pub fn from_properties(properties: &[(String, String)]) -> Result<Self, String> {
    let raw = properties
      .iter()
      .find(|(k, _)| k == RULES_KEY)
      .map(|(_, v)| v.as_str())
      .ok_or_else(|| format!("missing '{RULES_KEY}' property"))?;
    Self::from_json(raw)
  }

  pub fn from_json(raw: &str) -> Result<Self, String> {
    if raw.len() > MAX_RULES_BYTES {
      return Err(format!("rules exceed {MAX_RULES_BYTES} bytes"));
    }
    let config: MapperConfig =
      serde_json::from_str(raw).map_err(|e| format!("invalid rules JSON: {e}"))?;
    validate_config(&config)?;
    Ok(Self { config })
  }

  /// Evaluate `filter` predicates against the incoming payload. Empty filter list keeps everything.
  pub fn matches(&self, payload: &[u8]) -> Result<bool, String> {
    if self.config.filter.is_empty() {
      return Ok(true);
    }
    if payload.len() > MAX_INPUT_BYTES {
      return Err(format!("payload exceeds {MAX_INPUT_BYTES} bytes"));
    }
    let value: Value =
      serde_json::from_slice(payload).map_err(|e| format!("filter input is not JSON: {e}"))?;
    for rule in &self.config.filter {
      if !eval_filter(rule, &value) {
        return Ok(false);
      }
    }
    Ok(true)
  }

  /// Apply topic rewrite and payload transformations.
  pub fn apply(&self, topic: &str, payload: &[u8]) -> Result<MappedMessage, String> {
    if payload.len() > MAX_INPUT_BYTES {
      return Err(format!("payload exceeds {MAX_INPUT_BYTES} bytes"));
    }
    let needs_json = self.config.payload.is_some()
      || self
        .config
        .topic
        .as_ref()
        .is_some_and(|t| t.template.contains("{payload."));

    let parsed: Option<Value> = if needs_json {
      let v: Value =
        serde_json::from_slice(payload).map_err(|e| format!("map input is not JSON: {e}"))?;
      Some(v)
    } else {
      None
    };

    let out_topic = match &self.config.topic {
      Some(rule) => render_topic(rule, topic, parsed.as_ref())?,
      None => topic.to_owned(),
    };

    let (out_payload, is_json) = match (&self.config.payload, parsed) {
      (Some(rule), Some(Value::Object(obj))) => {
        let mapped = transform_object(obj, rule)?;
        let bytes =
          serde_json::to_vec(&Value::Object(mapped)).map_err(|e| format!("encode JSON: {e}"))?;
        (bytes, true)
      }
      (Some(_), _) => return Err("payload rules require a JSON object".into()),
      (None, _) => (payload.to_vec(), false),
    };

    Ok(MappedMessage {
      topic: out_topic,
      payload: out_payload,
      is_json,
    })
  }
}

fn validate_config(cfg: &MapperConfig) -> Result<(), String> {
  if cfg.version != 1 {
    return Err(format!("unsupported mapper version {}", cfg.version));
  }
  if cfg.filter.len() > MAX_FILTER_RULES {
    return Err(format!("too many filter rules (max {MAX_FILTER_RULES})"));
  }
  for f in &cfg.filter {
    if f.field.is_empty() || f.field.len() > 256 {
      return Err("filter field must be 1..=256 bytes".into());
    }
    if f.op != FilterOp::Exists && f.value.is_none() {
      return Err(format!("filter op {:?} requires a value", f.op));
    }
  }
  if let Some(topic) = &cfg.topic {
    if topic.template.is_empty() || topic.template.len() > MAX_TOPIC_BYTES {
      return Err("topic template must be 1..=512 bytes".into());
    }
    if topic.vars.len() > MAX_TOPIC_VARS {
      return Err(format!("too many topic vars (max {MAX_TOPIC_VARS})"));
    }
    for (k, v) in &topic.vars {
      if k.is_empty() || !valid_topic_segment(v) {
        return Err(format!("invalid topic var '{k}'"));
      }
    }
  }
  if let Some(p) = &cfg.payload {
    if p.convert.len() > MAX_FIELD_RULES
      || p.rename.len() > MAX_FIELD_RULES
      || p.set.len() > MAX_FIELD_RULES
      || p.keep.len() > MAX_FIELD_RULES
      || p.drop.len() > MAX_FIELD_RULES
    {
      return Err(format!("too many payload rules (max {MAX_FIELD_RULES})"));
    }
    for c in &p.convert {
      if let ConvertRule::Scale {
        factor,
        offset,
        round,
        ..
      } = c
      {
        if !factor.is_finite() || !offset.is_finite() {
          return Err("scale factor and offset must be finite".into());
        }
        if let Some(r) = round {
          if *r > 9 {
            return Err("scale round must be <= 9".into());
          }
        }
      }
    }
  }
  Ok(())
}

fn valid_topic_segment(s: &str) -> bool {
  !s.is_empty()
    && s.len() <= MAX_TOPIC_BYTES
    && !s
      .bytes()
      .any(|b| b.is_ascii_control() || b == b'+' || b == b'#' || b == 0)
}

fn valid_publish_topic(s: &str) -> bool {
  !s.is_empty()
    && s.len() <= MAX_TOPIC_BYTES
    && !s
      .bytes()
      .any(|b| b.is_ascii_control() || b == b'+' || b == b'#' || b == 0)
}

fn lookup_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
  let mut cur = root;
  for part in path.split('.') {
    cur = cur.as_object()?.get(part)?;
  }
  Some(cur)
}

fn eval_filter(rule: &FilterRule, root: &Value) -> bool {
  let found = lookup_path(root, &rule.field);
  match rule.op {
    FilterOp::Exists => found.is_some_and(|v| !v.is_null()),
    FilterOp::Eq => found.is_some_and(|v| rule.value.as_ref() == Some(v)),
    FilterOp::Ne => found.is_some_and(|v| rule.value.as_ref() != Some(v)),
    FilterOp::Gt => compare_num(found, rule.value.as_ref(), |a, b| a > b),
    FilterOp::Gte => compare_num(found, rule.value.as_ref(), |a, b| a >= b),
    FilterOp::Lt => compare_num(found, rule.value.as_ref(), |a, b| a < b),
    FilterOp::Lte => compare_num(found, rule.value.as_ref(), |a, b| a <= b),
    FilterOp::Contains => match (
      found.and_then(Value::as_str),
      rule.value.as_ref().and_then(Value::as_str),
    ) {
      (Some(haystack), Some(needle)) => haystack.contains(needle),
      _ => false,
    },
  }
}

fn compare_num(a: Option<&Value>, b: Option<&Value>, cmp: fn(f64, f64) -> bool) -> bool {
  match (a.and_then(Value::as_f64), b.and_then(Value::as_f64)) {
    (Some(x), Some(y)) if x.is_finite() && y.is_finite() => cmp(x, y),
    _ => false,
  }
}

fn render_topic(
  rule: &TopicRule,
  input_topic: &str,
  payload: Option<&Value>,
) -> Result<String, String> {
  let segments: Vec<&str> = input_topic.split('/').collect();
  let mut out = String::with_capacity(rule.template.len() + 32);
  let bytes = rule.template.as_bytes();
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] == b'{' {
      let end = rule.template[i + 1..]
        .find('}')
        .ok_or_else(|| "unclosed '{' in topic template".to_owned())?
        + i
        + 1;
      let token = &rule.template[i + 1..end];
      let resolved = resolve_token(token, input_topic, &segments, &rule.vars, payload)?;
      out.push_str(&resolved);
      i = end + 1;
    } else {
      let next = rule.template[i..]
        .find('{')
        .map(|pos| i + pos)
        .unwrap_or(bytes.len());
      out.push_str(&rule.template[i..next]);
      i = next;
    }
    if out.len() > MAX_TOPIC_BYTES {
      return Err(format!("rendered topic exceeds {MAX_TOPIC_BYTES} bytes"));
    }
  }
  if !valid_publish_topic(&out) {
    return Err("rendered topic is not a valid MQTT publish topic".into());
  }
  Ok(out)
}

fn resolve_token(
  token: &str,
  input_topic: &str,
  segments: &[&str],
  vars: &BTreeMap<String, String>,
  payload: Option<&Value>,
) -> Result<String, String> {
  if token == "topic" {
    return Ok(input_topic.to_owned());
  }
  if token == "topic.last" {
    return Ok((*segments.last().unwrap_or(&"")).to_owned());
  }
  if let Some(idx_str) = token.strip_prefix("topic.") {
    let idx: usize = idx_str
      .parse()
      .map_err(|_| format!("invalid topic segment index '{token}'"))?;
    return segments
      .get(idx)
      .map(|s| (*s).to_owned())
      .ok_or_else(|| format!("topic segment {idx} out of bounds"));
  }
  if let Some(field) = token.strip_prefix("payload.") {
    let root =
      payload.ok_or_else(|| "topic template references payload without JSON".to_owned())?;
    let val = lookup_path(root, field)
      .ok_or_else(|| format!("missing payload field '{field}' for topic template"))?;
    let text = match val {
      Value::String(s) => s.clone(),
      Value::Number(n) => n.to_string(),
      Value::Bool(b) => b.to_string(),
      _ => return Err(format!("payload field '{field}' is not a scalar")),
    };
    if text.contains('/') || !valid_topic_segment(&text) {
      return Err(format!(
        "payload field '{field}' is not a valid topic level"
      ));
    }
    return Ok(text);
  }
  if let Some(v) = vars.get(token) {
    return Ok(v.clone());
  }
  Err(format!("unknown topic template variable '{token}'"))
}

fn transform_object(
  mut obj: Map<String, Value>,
  rule: &PayloadRule,
) -> Result<Map<String, Value>, String> {
  // 1. Convert existing fields in place.
  for conv in &rule.convert {
    apply_convert(&mut obj, conv)?;
  }
  // 2. Rename fields.
  for ren in &rule.rename {
    if let Some(v) = obj.remove(&ren.from) {
      obj.insert(ren.to.clone(), v);
    }
  }
  // 3. Apply passthrough / keep / drop.
  let mut out = if !rule.passthrough && rule.keep.is_empty() {
    Map::new()
  } else if !rule.keep.is_empty() {
    let mut kept = Map::new();
    for k in &rule.keep {
      if let Some(v) = obj.remove(k) {
        kept.insert(k.clone(), v);
      }
    }
    kept
  } else {
    obj
  };
  for d in &rule.drop {
    out.remove(d);
  }
  // 4. Set static fields (always present in output even when passthrough=false).
  for s in &rule.set {
    out.insert(s.field.clone(), s.value.clone());
  }
  Ok(out)
}

fn apply_convert(obj: &mut Map<String, Value>, conv: &ConvertRule) -> Result<(), String> {
  match conv {
    ConvertRule::EpochSToIso8601 { field } => {
      if let Some(v) = obj.get(field).and_then(Value::as_f64) {
        if v.is_finite() {
          let secs = v.trunc() as i64;
          obj.insert(field.clone(), Value::String(format_iso8601_utc(secs)?));
        }
      }
    }
    ConvertRule::EpochMsToIso8601 { field } => {
      if let Some(v) = obj.get(field).and_then(Value::as_f64) {
        if v.is_finite() {
          let secs = (v / 1000.0).floor() as i64;
          obj.insert(field.clone(), Value::String(format_iso8601_utc(secs)?));
        }
      }
    }
    ConvertRule::Scale {
      field,
      factor,
      offset,
      round,
    } => {
      if let Some(v) = obj.get(field).and_then(Value::as_f64) {
        if v.is_finite() {
          let mut scaled = v * factor + offset;
          if let Some(decimals) = round {
            let pow = 10_f64.powi(*decimals as i32);
            scaled = (scaled * pow).round() / pow;
          }
          if let Some(num) = serde_json::Number::from_f64(scaled) {
            obj.insert(field.clone(), Value::Number(num));
          }
        }
      }
    }
    ConvertRule::Replace { field, from, to } => {
      if let Some(Value::String(s)) = obj.get(field) {
        if s == from {
          obj.insert(field.clone(), Value::String(to.clone()));
        }
      }
    }
    ConvertRule::ToString { field } => {
      if let Some(v) = obj.get(field) {
        let s = match v {
          Value::String(s) => s.clone(),
          Value::Number(n) => n.to_string(),
          Value::Bool(b) => b.to_string(),
          other => other.to_string(),
        };
        obj.insert(field.clone(), Value::String(s));
      }
    }
    ConvertRule::ToNumber { field } => {
      if let Some(Value::String(s)) = obj.get(field) {
        if let Ok(n) = s.trim().parse::<f64>() {
          if let Some(num) = serde_json::Number::from_f64(n) {
            obj.insert(field.clone(), Value::Number(num));
          }
        }
      }
    }
    ConvertRule::ToBool { field } => {
      if let Some(v) = obj.get(field) {
        let b = match v {
          Value::Bool(b) => Some(*b),
          Value::Number(n) => n.as_f64().map(|f| f != 0.0),
          Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
          },
          _ => None,
        };
        if let Some(b) = b {
          obj.insert(field.clone(), Value::Bool(b));
        }
      }
    }
  }
  Ok(())
}

/// Pure-Rust Unix epoch seconds -> `YYYY-MM-DDTHH:MM:SSZ` (Howard Hinnant civil-from-days).
pub fn format_iso8601_utc(epoch_secs: i64) -> Result<String, String> {
  if !(-62_135_596_800..=253_402_300_799).contains(&epoch_secs) {
    return Err("epoch timestamp out of range".into());
  }
  let days = epoch_secs.div_euclid(86_400);
  let secs_of_day = epoch_secs.rem_euclid(86_400);
  let hour = secs_of_day / 3600;
  let minute = (secs_of_day % 3600) / 60;
  let second = secs_of_day % 60;

  let z = days + 719_468;
  let era = z.div_euclid(146_097);
  let doe = z.rem_euclid(146_097) as u64;
  let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
  let mut year = (yoe as i64) + era * 400;
  let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
  let mp = (5 * doy + 2) / 153;
  let day = doy - (153 * mp + 2) / 5 + 1;
  let month = if mp < 10 { mp + 3 } else { mp - 9 };
  if month <= 2 {
    year += 1;
  }

  Ok(format!(
    "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
  ))
}

#[cfg(test)]
mod tests;

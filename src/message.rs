//! Message and telemetry representations for Newboot MQ Functions.

use crate::error::{Error, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// Standard JSON content type set by [`Message::with_json`] and [`Message::with_telemetry`].
pub const JSON_CONTENT_TYPE: &str = "application/json";

/// Default topic used when loading a raw sample file with [`Message::from_sample_file`].
pub const DEFAULT_SAMPLE_TOPIC: &str = "sample/input";

/// An MQTT message passed into a `#[map]` or `#[filter]` function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
  content_type: Option<String>,
  payload: Vec<u8>,
  timestamp_ms: Option<u64>,
  topic: String,
  user_properties: Vec<(String, String)>,
}

impl Message {
  /// Create a message with a topic and raw byte payload.
  pub fn new(topic: impl Into<String>, payload: impl Into<Vec<u8>>) -> Self {
    Self {
      content_type: None,
      payload: payload.into(),
      timestamp_ms: None,
      topic: topic.into(),
      user_properties: Vec::new(),
    }
  }

  /// Create a JSON message on `topic`.
  pub fn from_json(topic: impl Into<String>, value: Value) -> Self {
    Self::new(topic, Vec::new()).with_json(value)
  }

  /// Create a telemetry message on `topic`.
  pub fn from_telemetry(topic: impl Into<String>, telemetry: &Telemetry) -> Self {
    Self::new(topic, Vec::new()).with_telemetry(telemetry)
  }

  /// Load a sample JSON payload from `path` on [`DEFAULT_SAMPLE_TOPIC`].
  pub fn from_sample_file(path: impl AsRef<Path>) -> Result<Self> {
    Self::from_sample_file_with_topic(path, DEFAULT_SAMPLE_TOPIC)
  }

  /// Load a sample JSON payload from `path` with an explicit `topic`.
  pub fn from_sample_file_with_topic(
    path: impl AsRef<Path>,
    topic: impl Into<String>,
  ) -> Result<Self> {
    let bytes = std::fs::read(path)?;
    Ok(Self {
      content_type: Some(JSON_CONTENT_TYPE.to_string()),
      payload: bytes,
      timestamp_ms: None,
      topic: topic.into(),
      user_properties: Vec::new(),
    })
  }

  /// Return the message's MQTT topic.
  pub fn topic(&self) -> &str {
    &self.topic
  }

  /// Replace the message's MQTT topic.
  pub fn with_topic(mut self, topic: impl Into<String>) -> Self {
    self.topic = topic.into();
    self
  }

  /// Return the raw payload bytes.
  pub fn payload(&self) -> &[u8] {
    &self.payload
  }

  /// Replace the raw payload bytes.
  pub fn with_payload(mut self, payload: impl Into<Vec<u8>>) -> Self {
    self.payload = payload.into();
    self
  }

  /// Return the MQTT 5 content type, if set.
  pub fn content_type(&self) -> Option<&str> {
    self.content_type.as_deref()
  }

  /// Set the MQTT 5 content type.
  pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
    self.content_type = Some(content_type.into());
    self
  }

  /// Return the message timestamp in milliseconds since the Unix epoch, if set.
  pub fn timestamp_ms(&self) -> Option<u64> {
    self.timestamp_ms
  }

  /// Set the message timestamp in milliseconds since the Unix epoch.
  pub fn with_timestamp_ms(mut self, timestamp_ms: u64) -> Self {
    self.timestamp_ms = Some(timestamp_ms);
    self
  }

  /// Return all MQTT 5 user properties.
  pub fn user_properties(&self) -> &[(String, String)] {
    &self.user_properties
  }

  /// Look up the first MQTT 5 user property matching `key`.
  pub fn property(&self, key: &str) -> Option<&str> {
    self
      .user_properties
      .iter()
      .find(|(k, _)| k == key)
      .map(|(_, v)| v.as_str())
  }

  /// Append an MQTT 5 user property.
  pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
    self.user_properties.push((key.into(), value.into()));
    self
  }

  /// Parse the payload as an arbitrary [`serde_json::Value`].
  pub fn json(&self) -> Result<Value> {
    serde_json::from_slice(&self.payload).map_err(Error::from)
  }

  /// Deserialize the payload as a typed JSON value.
  pub fn json_as<T: DeserializeOwned>(&self) -> Result<T> {
    serde_json::from_slice(&self.payload).map_err(Error::from)
  }

  /// Replace the payload with serialized JSON and set `content_type` to `application/json`.
  pub fn with_json(mut self, value: Value) -> Self {
    self.payload = serde_json::to_vec(&value).unwrap_or_default();
    self.content_type = Some(JSON_CONTENT_TYPE.to_string());
    self
  }

  /// Serialize `value` as JSON into the payload and set `content_type` to `application/json`.
  pub fn try_with_json<T: Serialize>(mut self, value: &T) -> Result<Self> {
    self.payload = serde_json::to_vec(value)?;
    self.content_type = Some(JSON_CONTENT_TYPE.to_string());
    Ok(self)
  }

  /// Parse the payload as a Newboot Superset [`Telemetry`] reading.
  pub fn telemetry(&self) -> Result<Telemetry> {
    self.json_as()
  }

  /// Replace the payload with a serialized [`Telemetry`] reading.
  pub fn with_telemetry(mut self, telemetry: &Telemetry) -> Self {
    self.payload = serde_json::to_vec(telemetry).unwrap_or_default();
    self.content_type = Some(JSON_CONTENT_TYPE.to_string());
    self
  }

  /// Consume the message into its constituent parts without cloning.
  #[cfg(target_arch = "wasm32")]
  pub(crate) fn into_parts(self) -> MessageParts {
    MessageParts {
      content_type: self.content_type,
      payload: self.payload,
      timestamp_ms: self.timestamp_ms,
      topic: self.topic,
      user_properties: self.user_properties,
    }
  }
}

#[cfg(target_arch = "wasm32")]
pub(crate) struct MessageParts {
  pub(crate) content_type: Option<String>,
  pub(crate) payload: Vec<u8>,
  pub(crate) timestamp_ms: Option<u64>,
  pub(crate) topic: String,
  pub(crate) user_properties: Vec<(String, String)>,
}

/// Standard Newboot Superset telemetry payload:
/// `{"variableId": "...", "value": ..., "name"?: "...", "timestamp"?: 1700000000000}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Telemetry {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub timestamp: Option<u64>,
  pub value: Value,
  #[serde(rename = "variableId")]
  pub variable_id: String,
}

impl Telemetry {
  /// Create a telemetry reading with a `variable_id` and JSON `value`.
  pub fn new(variable_id: impl Into<String>, value: impl Into<Value>) -> Self {
    Self {
      name: None,
      timestamp: None,
      value: value.into(),
      variable_id: variable_id.into(),
    }
  }

  /// Attach a human-readable variable name.
  pub fn with_name(mut self, name: impl Into<String>) -> Self {
    self.name = Some(name.into());
    self
  }

  /// Attach a Unix epoch timestamp in milliseconds.
  pub fn with_timestamp(mut self, timestamp_ms: u64) -> Self {
    self.timestamp = Some(timestamp_ms);
    self
  }

  /// Return `value` as `f64` if it is a JSON number.
  pub fn as_f64(&self) -> Option<f64> {
    self.value.as_f64()
  }

  /// Return `value` as `bool` if it is a JSON boolean.
  pub fn as_bool(&self) -> Option<bool> {
    self.value.as_bool()
  }

  /// Return `value` as `&str` if it is a JSON string.
  pub fn as_str(&self) -> Option<&str> {
    self.value.as_str()
  }
}

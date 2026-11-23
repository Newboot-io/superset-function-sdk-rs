//! Internal WebAssembly bindings over `wasm-graph:processor@1.1.0`.
//!
//! Only compiled for `target_arch = "wasm32"`. User crates never need `.wit` files
//! or a direct `wit-bindgen` dependency.

use crate::log::Level;
use crate::message::Message;

pub mod wit_map {
  wit_bindgen::generate!({
    path: "wit",
    world: "newboot:function/map-function@0.1.0",
    generate_all,
    pub_export_macro: true,
    default_bindings_module: "::newboot::__private::wit_map",
  });
}

pub mod wit_filter {
  wit_bindgen::generate!({
    path: "wit",
    world: "newboot:function/filter-function@0.1.0",
    generate_all,
    pub_export_macro: true,
    default_bindings_module: "::newboot::__private::wit_filter",
  });
}

pub use wit_filter::export as export_filter;
pub use wit_map::export as export_map;

pub fn wit_log(level: Level, context: &str, message: &str) {
  use wit_map::wasm_graph::processor::logger;
  let wit_level = match level {
    Level::Trace => logger::Level::Trace,
    Level::Debug => logger::Level::Debug,
    Level::Info => logger::Level::Info,
    Level::Warn => logger::Level::Warn,
    Level::Error => logger::Level::Error,
  };
  logger::log(wit_level, context, message);
}

pub fn wit_add_counter(name: &str, value: u64, labels: &[(&str, &str)]) {
  use wit_map::wasm_graph::processor::metrics;
  let wit_labels: Option<Vec<metrics::Label>> = if labels.is_empty() {
    None
  } else {
    Some(
      labels
        .iter()
        .map(|(k, v)| metrics::Label {
          key: (*k).to_string(),
          value: (*v).to_string(),
        })
        .collect(),
    )
  };
  let _ = metrics::add_to_counter(
    name,
    metrics::CounterValue::U64(value),
    wit_labels.as_deref(),
  );
}

macro_rules! impl_from_data_model {
  ($mod_path:ident, $from_fn:ident) => {
    pub fn $from_fn(
      data: $mod_path::wasm_graph::processor::types::DataModel,
    ) -> Result<
      (
        $mod_path::wasm_graph::processor::types::Timestamp,
        Option<$mod_path::wasm_graph::processor::types::MessageSchema>,
        Message,
      ),
      $mod_path::wasm_graph::processor::types::Error,
    > {
      use $mod_path::wasm_graph::processor::types::{
        BufferOrBytes, BufferOrString, DataModel, Error,
      };
      let read_bytes = |b: BufferOrBytes| match b {
        BufferOrBytes::Buffer(buf) => buf.read(),
        BufferOrBytes::Bytes(bytes) => bytes,
      };
      let read_string = |s: BufferOrString| -> Result<String, Error> {
        match s {
          BufferOrString::Buffer(buf) => String::from_utf8(buf.read()).map_err(|_| Error {
            message: "buffer is not valid UTF-8".into(),
          }),
          BufferOrString::String(s) => Ok(s),
        }
      };

      let DataModel::Message(m) = data else {
        return Err(Error {
          message: "expected a message".into(),
        });
      };
      let topic = String::from_utf8(read_bytes(m.topic)).map_err(|_| Error {
        message: "topic is not valid UTF-8".into(),
      })?;
      let payload = read_bytes(m.payload);
      let ts_ms = m
        .timestamp
        .timestamp
        .secs
        .saturating_mul(1000)
        .saturating_add(u64::from(m.timestamp.timestamp.nanos) / 1_000_000);
      let mut msg = Message::new(topic, payload).with_timestamp_ms(ts_ms);
      if let Some(ct) = m.content_type {
        msg = msg.with_content_type(read_string(ct)?);
      }
      for (k, v) in m.properties.user_properties {
        msg = msg.with_property(read_string(k)?, read_string(v)?);
      }
      Ok((m.timestamp, m.schema, msg))
    }
  };
}

impl_from_data_model!(wit_map, from_map_data_model);
impl_from_data_model!(wit_filter, from_filter_data_model);

pub fn to_map_data_model(
  mut timestamp: wit_map::wasm_graph::processor::types::Timestamp,
  schema: Option<wit_map::wasm_graph::processor::types::MessageSchema>,
  msg: Message,
) -> wit_map::wasm_graph::processor::types::DataModel {
  use wit_map::wasm_graph::processor::types::{
    BufferOrBytes, BufferOrString, DataModel, Message as WitMessage, MessageProperties,
  };
  let parts = msg.into_parts();
  if let Some(ms) = parts.timestamp_ms {
    timestamp.timestamp.secs = ms / 1000;
    timestamp.timestamp.nanos = ((ms % 1000) * 1_000_000) as u32;
  }
  let user_properties = parts
    .user_properties
    .into_iter()
    .map(|(k, v)| (BufferOrString::String(k), BufferOrString::String(v)))
    .collect();
  DataModel::Message(WitMessage {
    timestamp,
    topic: BufferOrBytes::Bytes(parts.topic.into_bytes()),
    content_type: parts.content_type.map(BufferOrString::String),
    payload: BufferOrBytes::Bytes(parts.payload),
    properties: MessageProperties { user_properties },
    schema,
  })
}

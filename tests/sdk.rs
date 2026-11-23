use newboot::{
  filter, map, render_function_toml, Call, Error, FunctionKind, FunctionManifest, Message,
  NoSettings, SettingKind, Settings, Telemetry,
};
use serde::Serialize;
use serde_json::json;

#[derive(Settings, Debug, PartialEq)]
struct AllTypes {
  /// Zone weights string
  ///
  /// Second line
  zone_weights: String,
  #[setting(default = "1.5")]
  scale_f64: f64,
  #[setting(default = "2.5")]
  scale_f32: f32,
  #[setting(default = "-7")]
  offset_i64: i64,
  #[setting(default = "42")]
  count_u64: u64,
  #[setting(default = "-3")]
  delta_i32: i32,
  #[setting(default = "8")]
  window_u32: u32,
  #[setting(default = "16")]
  limit_usize: usize,
  #[setting(default = "true")]
  enabled: bool,
  #[setting(key = "customKey")]
  optional_note: Option<String>,
  #[setting(default = "99")]
  optional_with_default: Option<u32>,
}

#[derive(Settings, Debug, PartialEq)]
struct HygieneCheck {
  properties: String,
  r#type: String,
  _leading: Option<String>,
}

#[derive(Settings, Debug, PartialEq)]
struct EmptyStruct;

#[derive(Settings, Debug, PartialEq)]
struct Threshold {
  /// Minimum value to keep
  min_value: f64,
}

#[map]
fn enrich(Call { message, settings }: Call<AllTypes>) -> newboot::Result<Message> {
  let mut doc = message.json()?;
  doc["weights"] = json!(settings.zone_weights);
  doc["scale"] = json!(settings.scale_f64);
  newboot::log::trace!("trace {}", settings.window_u32);
  newboot::log::debug!("debug {}", settings.offset_i64);
  newboot::log::info!("info {}", settings.count_u64);
  newboot::log::warn!("warn {}", settings.delta_i32);
  newboot::log::error!("error {}", settings.limit_usize);
  newboot::metrics::increment("enriched");
  newboot::metrics::add("enriched_by", 2, &[("enabled", "true")]);
  Ok(message.with_json(doc))
}

#[filter]
fn above_min(Call { message, settings }: Call<Threshold>) -> newboot::Result<bool> {
  let t = message.telemetry()?;
  Ok(t.as_f64().unwrap_or(0.0) >= settings.min_value)
}

#[map]
fn passthrough(Call { message, .. }: Call) -> newboot::Result<Message> {
  Ok(message)
}

#[test]
fn error_helpers_and_conversions_never_leak_payload() {
  let e1 = Error::new("boom");
  assert_eq!(e1.message(), "boom");
  assert_eq!(e1.to_string(), "boom");
  let _: &dyn std::error::Error = &e1;

  let e2 = Error::missing_setting("foo");
  assert!(e2.to_string().contains("foo"));

  let e3 = Error::invalid_setting("bar", "bad");
  assert!(e3.to_string().contains("bar"));
  assert!(e3.to_string().contains("bad"));

  let e4: Error = String::from("from string").into();
  assert_eq!(e4.message(), "from string");

  let e5: Error = "from str".into();
  assert_eq!(e5.message(), "from str");

  // Typed JSON error must not quote raw payload strings.
  let secret_payload = Message::new(
    "t",
    br#"{"variableId":123,"value":"SECRET-PLANT-DATA"}"#.to_vec(),
  );
  let json_err = secret_payload.telemetry().unwrap_err();
  assert!(json_err.message().contains("invalid JSON"));
  assert!(!json_err.message().contains("SECRET-PLANT-DATA"));

  let io_err: Error = std::io::Error::new(std::io::ErrorKind::NotFound, "missing").into();
  assert!(io_err.message().contains("I/O error"));
}

#[test]
fn message_and_telemetry_helpers() {
  let msg = Message::new("plant/t1", b"raw".to_vec())
    .with_topic("plant/t2")
    .with_payload(b"{\"a\":1}".to_vec())
    .with_content_type("application/json")
    .with_timestamp_ms(1_700_000_000_000)
    .with_property("cluster", "c1");

  assert_eq!(msg.topic(), "plant/t2");
  assert_eq!(msg.payload(), b"{\"a\":1}");
  assert_eq!(msg.content_type(), Some("application/json"));
  assert_eq!(msg.timestamp_ms(), Some(1_700_000_000_000));
  assert_eq!(msg.user_properties(), &[("cluster".into(), "c1".into())]);
  assert_eq!(msg.property("cluster"), Some("c1"));
  assert_eq!(msg.property("missing"), None);
  assert_eq!(msg.json().unwrap()["a"], 1);

  #[derive(Serialize)]
  struct Custom {
    x: i32,
  }
  let msg2 = Message::new("t", Vec::new())
    .try_with_json(&Custom { x: 7 })
    .unwrap();
  assert_eq!(msg2.json().unwrap()["x"], 7);

  let bad_json = Message::new("t", b"not-json".to_vec());
  assert!(bad_json.json().is_err());

  let t = Telemetry::new("temp", 22.5)
    .with_name("Temperature")
    .with_timestamp(1234);
  assert_eq!(t.as_f64(), Some(22.5));
  assert_eq!(t.as_bool(), None);
  assert_eq!(t.as_str(), None);

  let t_bool = Telemetry::new("ok", true);
  assert_eq!(t_bool.as_bool(), Some(true));
  let t_str = Telemetry::new("mode", "auto");
  assert_eq!(t_str.as_str(), Some("auto"));

  let m_t = Message::from_telemetry("output/telemetry", &t);
  assert_eq!(m_t.telemetry().unwrap(), t);

  let dir = tempfile::tempdir().unwrap();
  let sample = dir.path().join("in.json");
  std::fs::write(&sample, r#"{"variableId":"v1","value":42}"#).unwrap();
  let loaded = Message::from_sample_file(&sample).unwrap();
  assert_eq!(loaded.topic(), newboot::DEFAULT_SAMPLE_TOPIC);
  assert_eq!(loaded.telemetry().unwrap().as_f64(), Some(42.0));
  assert!(Message::from_sample_file(dir.path().join("missing.json")).is_err());
}

#[test]
fn settings_derive_parsing_hygiene_and_errors() {
  let empty = EmptyStruct::from_properties(&[]).unwrap();
  assert_eq!(empty.clone_for_call(), EmptyStruct);
  assert!(EmptyStruct::specs().is_empty());

  let no = NoSettings::from_properties(&[]).unwrap();
  assert_eq!(no.clone_for_call(), NoSettings);
  assert!(NoSettings::specs().is_empty());

  // Field named `properties`, raw ident `r#type`, and leading `_` all work cleanly.
  let h = HygieneCheck::from_properties(&[
    ("properties".into(), "p".into()),
    ("type".into(), "t".into()),
    ("leading".into(), "l".into()),
  ])
  .unwrap();
  assert_eq!(h.properties, "p");
  assert_eq!(h.r#type, "t");
  assert_eq!(h._leading.as_deref(), Some("l"));
  assert_eq!(h.clone_for_call(), h);

  // Missing required setting fails.
  assert!(AllTypes::from_properties(&[]).is_err());

  // Defaults apply when optional/defaulted settings are absent.
  let parsed = AllTypes::from_properties(&[("zoneWeights".into(), "z1:1.0".into())]).unwrap();
  assert_eq!(parsed.zone_weights, "z1:1.0");
  assert_eq!(parsed.scale_f64, 1.5);
  assert_eq!(parsed.scale_f32, 2.5);
  assert_eq!(parsed.offset_i64, -7);
  assert_eq!(parsed.count_u64, 42);
  assert_eq!(parsed.delta_i32, -3);
  assert_eq!(parsed.window_u32, 8);
  assert_eq!(parsed.limit_usize, 16);
  assert!(parsed.enabled);
  assert_eq!(parsed.optional_note, None);
  assert_eq!(parsed.optional_with_default, Some(99));
  assert_eq!(parsed.clone_for_call(), parsed);

  // Explicit overrides including `false` and `customKey`.
  let overridden = AllTypes::from_properties(&[
    ("zoneWeights".into(), "z2:0.5".into()),
    ("enabled".into(), "false".into()),
    ("customKey".into(), "hello".into()),
    ("optionalWithDefault".into(), "12".into()),
  ])
  .unwrap();
  assert!(!overridden.enabled);
  assert_eq!(overridden.optional_note.as_deref(), Some("hello"));
  assert_eq!(overridden.optional_with_default, Some(12));

  // Parse errors for every numeric/bool helper.
  assert!(newboot::settings::parse_f64("k", "abc").is_err());
  assert!(newboot::settings::parse_f64("k", "NaN").is_err());
  assert!(newboot::settings::parse_f32("k", "abc").is_err());
  assert!(newboot::settings::parse_f32("k", "inf").is_err());
  assert!(newboot::settings::parse_i64("k", "abc").is_err());
  assert!(newboot::settings::parse_u64("k", "-1").is_err());
  assert!(newboot::settings::parse_i32("k", "abc").is_err());
  assert!(newboot::settings::parse_u32("k", "-1").is_err());
  assert!(newboot::settings::parse_usize("k", "-1").is_err());
  assert!(newboot::settings::parse_bool("k", "yes").is_err());

  assert_eq!(SettingKind::Boolean.as_str(), "boolean");
  assert_eq!(SettingKind::Integer.as_str(), "integer");
  assert_eq!(SettingKind::Number.as_str(), "number");
  assert_eq!(SettingKind::String.as_str(), "string");
}

#[test]
fn manifest_rendering_escaping_and_check_file() {
  let manifest = enrich_manifest(
    "enrich",
    "0.1.0",
    "Line 1\n\"quoted\"\t\\slash\r\x08\x0c\x01\x7f",
  );
  let toml = render_function_toml(&manifest);
  assert!(toml.contains("name = \"enrich\""));
  assert!(toml.contains("runs_as = \"map\""));
  assert!(toml.contains("key = \"zoneWeights\""));
  assert!(toml.contains("default = \"1.5\""));
  assert!(toml.contains("key = \"customKey\""));
  assert!(toml.contains("description = \"Zone weights string Second line\""));
  assert!(toml.contains("\\b\\f\\u0001\\u007F"));

  let filter_manifest = FunctionManifest {
    description: String::new(),
    kind: FunctionKind::Filter,
    name: "above-min".into(),
    settings: Threshold::specs(),
    version: "0.1.0".into(),
  };
  assert!(filter_manifest.to_toml().contains("runs_as = \"filter\""));

  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("function.toml");
  assert!(manifest.check_file(&path).is_err());
  std::fs::write(&path, "stale").unwrap();
  assert!(manifest.check_file_with_update(&path, false).is_err());
  std::fs::write(&path, manifest.to_toml()).unwrap();
  manifest.check_file(&path).unwrap();

  // Explicit update mode rewrites the file without mutating process env.
  filter_manifest.check_file_with_update(&path, true).unwrap();
  assert_eq!(
    std::fs::read_to_string(&path).unwrap(),
    filter_manifest.to_toml()
  );
}

#[test]
fn native_test_runner_map_filter_nesting_and_panic_safety() {
  // Logging outside test::run is a no-op.
  newboot::log::info!("outside capture");
  newboot::metrics::increment("outside");

  let run = newboot::test::run(newboot::test::Run {
    function: enrich,
    message: Message::from_json("sensors/c1", json!({"value": 1})),
    settings: &[("zoneWeights", "z1:0.5")],
  })
  .unwrap();

  let out = run.outcome.into_message().unwrap();
  assert_eq!(out.json().unwrap()["weights"], "z1:0.5");
  assert_eq!(run.logs.len(), 5);
  assert_eq!(run.logs[0].level.as_str(), "trace");
  assert_eq!(run.logs[1].level.as_str(), "debug");
  assert_eq!(run.logs[2].level.as_str(), "info");
  assert_eq!(run.logs[3].level.as_str(), "warn");
  assert_eq!(run.logs[4].level.as_str(), "error");
  assert_eq!(run.counters.len(), 2);
  assert_eq!(run.counters[0].name, "enriched");
  assert_eq!(run.counters[0].value, 1);
  assert_eq!(run.counters[1].name, "enriched_by");
  assert_eq!(run.counters[1].value, 2);
  assert_eq!(
    run.counters[1].labels,
    vec![("enabled".into(), "true".into())]
  );

  // Filter keeps message >= minValue.
  let kept = newboot::test::run(newboot::test::Run {
    function: above_min,
    message: Message::from_telemetry("t", &Telemetry::new("v1", 10.0)),
    settings: &[("minValue", "5.0")],
  })
  .unwrap();
  assert!(kept.outcome.into_message().is_some());

  // Filter drops message < minValue.
  let dropped = newboot::test::run(newboot::test::Run {
    function: above_min,
    message: Message::from_telemetry("t", &Telemetry::new("v1", 2.0)),
    settings: &[("minValue", "5.0")],
  })
  .unwrap();
  assert_eq!(dropped.outcome, newboot::test::TestOutcome::Dropped);
  assert!(dropped.outcome.into_message().is_none());

  // NoSettings passthrough map.
  let pass = newboot::test::run(newboot::test::Run {
    function: passthrough,
    message: Message::new("t", b"hi".to_vec()),
    settings: &[],
  })
  .unwrap();
  assert_eq!(pass.outcome.into_message().unwrap().payload(), b"hi");

  // Nested test::run restores outer capture cleanly.
  let outer = newboot::test::run(newboot::test::Run {
    function: |call: Call<NoSettings>| {
      newboot::log::info!("outer-before");
      newboot::metrics::increment("outer");
      let inner = newboot::test::run(newboot::test::Run {
        function: |inner_call: Call<NoSettings>| {
          newboot::log::info!("inner");
          newboot::metrics::increment("inner");
          Ok(inner_call.message)
        },
        message: Message::new("t", b"inner".to_vec()),
        settings: &[],
      })?;
      assert_eq!(inner.logs.len(), 1);
      assert_eq!(inner.counters.len(), 1);
      newboot::log::info!("outer-after");
      Ok(call.message)
    },
    message: Message::new("t", b"outer".to_vec()),
    settings: &[],
  })
  .unwrap();
  assert_eq!(outer.logs.len(), 2);
  assert_eq!(outer.counters.len(), 1);

  // Panicking inside test::run cleans up thread-local capture guards on unwind.
  let panicked = std::panic::catch_unwind(|| {
    let _ = newboot::test::run(newboot::test::Run {
      function: |_call: Call<NoSettings>| -> newboot::Result<Message> {
        newboot::log::info!("before-panic");
        newboot::metrics::increment("before-panic");
        panic!("intentional panic");
      },
      message: Message::new("t", Vec::new()),
      settings: &[],
    });
  });
  assert!(panicked.is_err());

  // Subsequent logs outside test::run must not leak into the next run.
  newboot::log::info!("stray");
  newboot::metrics::increment("stray");
  let after = newboot::test::run(newboot::test::Run {
    function: passthrough,
    message: Message::new("t", Vec::new()),
    settings: &[],
  })
  .unwrap();
  assert!(after.logs.is_empty());
  assert!(after.counters.is_empty());
}

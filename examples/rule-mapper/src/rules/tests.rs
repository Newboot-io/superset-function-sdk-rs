use super::*;
use serde_json::json;

#[test]
fn uns_temperature_example_from_console() {
  let rules = json!({
    "version": 1,
    "topic": {
      "template": "{scope}/{site}/{cluster}/{module}/{topic.last}",
      "vars": {
        "scope": "factory-fr",
        "site": "lyon",
        "cluster": "vision-gemma-ha",
        "module": "smoother"
      }
    },
    "payload": {
      "passthrough": true,
      "convert": [
        { "kind": "epoch_s_to_iso8601", "field": "ts" },
        { "kind": "replace", "field": "unit", "from": "C", "to": "°C" }
      ],
      "rename": [
        { "from": "value", "to": "Value" },
        { "from": "unit", "to": "Unit" },
        { "from": "ts", "to": "Timestamp" }
      ]
    }
  });
  let mapper = CompiledMapper::from_json(&rules.to_string()).unwrap();
  let input = json!({ "value": 23.4, "unit": "C", "ts": 1794216688 });
  let out = mapper
    .apply("sensors/temperature", &serde_json::to_vec(&input).unwrap())
    .unwrap();
  assert_eq!(
    out.topic,
    "factory-fr/lyon/vision-gemma-ha/smoother/temperature"
  );
  let body: Value = serde_json::from_slice(&out.payload).unwrap();
  assert_eq!(
    body,
    json!({
      "Value": 23.4,
      "Unit": "°C",
      "Timestamp": "2026-11-09T09:31:28Z"
    })
  );
}

#[test]
fn vision_gemma_route_1_observation_filter_and_normalize() {
  let rules = json!({
    "version": 1,
    "filter": [
      { "field": "score", "op": "gte", "value": 0.5 }
    ],
    "payload": {
      "passthrough": true,
      "convert": [
        { "kind": "scale", "field": "score", "factor": 100.0, "offset": 0.0, "round": 1 }
      ],
      "rename": [
        { "from": "score", "to": "corrosionPct" }
      ],
      "set": [
        { "field": "normalizedBy", "value": "mapper.wasm" }
      ]
    }
  });
  let mapper = CompiledMapper::from_json(&rules.to_string()).unwrap();

  let low = json!({
    "observation_id": "obs-1", "camera": "cam-01", "zone": "pipe-rack-a",
    "release": "synthetic-replay-1", "model": "corrosion-v1",
    "ts": 1795185000.0, "score": 0.08, "frame": "replay://cam-01/pipe-rack-a/00000001.jpg"
  });
  assert!(!mapper.matches(&serde_json::to_vec(&low).unwrap()).unwrap());

  let high = json!({
    "observation_id": "obs-2", "camera": "cam-01", "zone": "pipe-rack-a",
    "release": "synthetic-replay-1", "model": "corrosion-v1",
    "ts": 1795185001.0, "score": 0.934, "frame": "replay://cam-01/pipe-rack-a/00000002.jpg"
  });
  let high_bytes = serde_json::to_vec(&high).unwrap();
  assert!(mapper.matches(&high_bytes).unwrap());

  let mapped = mapper
    .apply("sample/vision-trigger-gemma/observations", &high_bytes)
    .unwrap();
  assert_eq!(mapped.topic, "sample/vision-trigger-gemma/observations");
  let body: Value = serde_json::from_slice(&mapped.payload).unwrap();
  assert_eq!(body["corrosionPct"], json!(93.4));
  assert_eq!(body["normalizedBy"], json!("mapper.wasm"));
  assert!(body.get("score").is_none());
}

#[test]
fn vision_gemma_route_2_report_to_uns_cloud() {
  let rules = json!({
    "version": 1,
    "topic": {
      "template": "uns/{scope}/{site}/{cluster}/corrosion/{payload.camera}/{payload.zone}/report",
      "vars": {
        "scope": "industrial",
        "site": "lab-fr",
        "cluster": "vision-gemma-ha"
      }
    },
    "filter": [
      { "field": "kind", "op": "eq", "value": "corrosion_inspection_report" }
    ],
    "payload": {
      "passthrough": true,
      "convert": [
        { "kind": "epoch_s_to_iso8601", "field": "fired_ts" }
      ],
      "rename": [
        { "from": "event_id", "to": "incidentId" },
        { "from": "fired_ts", "to": "firedAt" }
      ],
      "set": [
        { "field": "schema", "value": "newboot.uns.corrosion_report.v1" },
        { "field": "cluster", "value": "vision-gemma-ha" }
      ],
      "drop": ["evidence"]
    }
  });
  let mapper = CompiledMapper::from_json(&rules.to_string()).unwrap();

  let report = json!({
    "event_id": "evt-42",
    "kind": "corrosion_inspection_report",
    "camera": "cam-01",
    "zone": "pipe-rack-a",
    "fired_ts": 1794216688,
    "summary": "Severe surface corrosion detected on pipe-rack-a",
    "evidence": [{ "frame": "replay://cam-01/pipe-rack-a/00000025.jpg" }]
  });
  let bytes = serde_json::to_vec(&report).unwrap();
  assert!(mapper.matches(&bytes).unwrap());

  let mapped = mapper
    .apply("sample/vision-trigger-gemma/reports", &bytes)
    .unwrap();
  assert_eq!(
    mapped.topic,
    "uns/industrial/lab-fr/vision-gemma-ha/corrosion/cam-01/pipe-rack-a/report"
  );
  let body: Value = serde_json::from_slice(&mapped.payload).unwrap();
  assert_eq!(body["incidentId"], json!("evt-42"));
  assert_eq!(body["firedAt"], json!("2026-11-09T09:31:28Z"));
  assert_eq!(body["schema"], json!("newboot.uns.corrosion_report.v1"));
  assert!(body.get("evidence").is_none());
}

#[test]
fn rejects_wildcard_injection_from_payload_topic_token() {
  let rules = json!({
    "version": 1,
    "topic": {
      "template": "uns/{payload.camera}/obs"
    }
  });
  let mapper = CompiledMapper::from_json(&rules.to_string()).unwrap();
  let bad = json!({ "camera": "cam/+/evil" });
  let err = mapper
    .apply("in/topic", &serde_json::to_vec(&bad).unwrap())
    .unwrap_err();
  assert!(err.contains("not a valid topic level"));
}

#[test]
fn rejects_unknown_version_and_non_finite_scale() {
  assert!(CompiledMapper::from_json(r#"{"version": 2}"#).is_err());
  assert!(
    CompiledMapper::from_json(r#"{"version": 1, "filter": [{"field": "a", "op": "eq"}]}"#).is_err()
  );
}

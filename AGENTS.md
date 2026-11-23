# Newboot MQ Functions — Agent Rules

This repository is the starting template for a **Newboot MQ Function** built in Rust with
`superset-function-sdk-rs` (`use newboot::...`) over `wasm-graph:processor@1.1.0`.

Follow every rule below when writing or modifying a function.

## 1. Shape and API

- Annotate the entry point with `#[map]` (returns `newboot::Result<Message>`) or `#[filter]`
  (returns `newboot::Result<bool>`: `true` keeps the message, `false` drops it).
- Every `#[map]` or `#[filter]` function takes one parameter:
  `Call { message, settings }: Call<MySettings>` (or `Call<NoSettings>` / `Call` when no
  settings are needed).
- Keep all struct fields in **alphabetical order** and use named parameter structs for
  multi-argument helpers in `src/logic.rs`.
- Declare all route settings with `#[derive(Settings)]` and doc comments (`/// ...`) on each field.
- Keep `function.toml` in sync with `#[derive(Settings)]` via a unit test calling
  `<fn>_manifest(name, version, description).check_file("function.toml")` (re-run with
  `UPDATE_FUNCTION_TOML=1 cargo test` after changing settings).
- Keep domain logic in plain, pure functions in `src/logic.rs` and unit-test both `src/logic.rs`
  and the entry point with `newboot::test::run` on `samples/`.
- When adapting the starter crate to a new function, update `package.name` in root `Cargo.toml`,
  the function name/logic in `src/lib.rs` and `src/logic.rs`, and regenerate `function.toml` with
  `UPDATE_FUNCTION_TOML=1 cargo test`.

## 2. SDK Quick Reference (`use newboot::...`)

- **Settings**: `#[derive(Settings)]` on a struct with named fields of type `String`, `bool`,
  `f64`, `f32`, `i64`, `u64`, `i32`, `u32`, `usize`, or `Option<T>`.
  - Field names map from `snake_case` to `camelCase` (`zone_weights` → `"zoneWeights"`).
  - Override key or provide a default with `#[setting(key = "customKey", default = "1.0")]`.
  - Fields without `Option<T>` or `default` are `required = true` in `function.toml`; missing or
    invalid values fail `init` closed (`failed(init)`).
- **Message & Telemetry**:
  - `message.topic()`, `message.with_topic(...)`
  - `message.payload()`, `message.with_payload(...)`
  - `message.json() -> newboot::Result<serde_json::Value>`, `message.json_as::<T>()`,
    `message.with_json(value)`, `message.try_with_json(&val)`
  - `message.telemetry() -> newboot::Result<Telemetry>`, `message.with_telemetry(&t)`
  - `message.property("key")`, `message.with_property("k", "v")`, `message.timestamp_ms()`,
    `message.with_timestamp_ms(ms)`
  - `Message::from_sample_file("samples/in.json")`
- **Logs & Counters**:
  - `newboot::log::{trace!, debug!, info!, warn!, error!}`
  - `newboot::metrics::increment("name")` and `newboot::metrics::add("name", 1, &[("k", "v")])`
- **Native Test Runner** (no WebAssembly build needed):
  ```rust
  let run = newboot::test::run(newboot::test::Run {
    function: my_fn,
    message: Message::from_sample_file("samples/in.json")?,
    settings: &[("zoneWeights", "z1:0.5")],
  })?;
  let out = run.outcome.into_message(); // None if dropped by #[filter]
  ```

## 3. Ready Examples (`examples/`)

Copy the closest example when starting a new function:

- `examples/rename-convert` (`#[map]`): rename a JSON field and apply linear unit conversion.
- `examples/uns-topic` (`#[map]`): build an ISA-95 Unified Namespace topic.
- `examples/drop-below` (`#[filter]`): drop telemetry readings below a minimum value.
- `examples/running-average` (`#[map]`): bounded sliding-window running average per `variableId`.
- `examples/rule-mapper` (`#[map]`): declarative JSON rule mapper for topics and payloads.

## 4. Security and Privacy

- **Never log message content** (payloads, raw sensor values, or fields that carry plant data).
  Function logs leave the plant with the cluster's observability stream.
- **Settings are never secrets.** Route settings are stored in plain text and shown in the console.
  Never declare API keys, tokens, passwords, or credentials as settings.
- **No network, files, or environment.** WASI filesystem, environment variables, and sockets are
  absent in Newboot MQ; only clocks and random numbers are real.
- **Destination is fixed by the route.** Writing a topic inside `#[map]` does not choose the
  destination Event Hub or Blob container; it only updates the message's topic metadata.
- **Keep error messages generic.** Never quote raw payloads in `Error::new(...)` or `panic!`.
- **Keep counter cardinality low.** Guest counters are capped at 32 series per function and
  4 labels per call (64 B per key/value). Never put dynamic message values into counter labels.

## 5. Cluster Limits

Every function runs inside Newboot MQ under strict per-step limits:

| Resource | Cluster limit |
| --- | --- |
| Linear memory | **16 MiB** per instance (growing past traps) |
| Compute (fuel) | **10,000,000** instructions per message |
| Call timeout | **100 ms** per message (5 consecutive timeouts trip the route to `failed(timeouts)`) |
| Input message | **64 KiB** max |
| Output message (Event Hubs) | **8 KiB** max |
| Guest logs | **10 lines/s**, burst **50**, **1 KiB** per line (default route level: `warn`) |
| Guest counters | **32 series** per function, **4 labels** per call |
| State lifetime | Memory resets on crash, broker restart, or deploy; each broker node runs its own copy |

If your function keeps in-memory state (like `examples/running-average`), bound every collection
(key length, entry count, and buffer length) so memory never grows toward 16 MiB.

## 6. Style, Verification, and Workflow

- Formatting: `rustfmt.toml` uses **2 spaces** (`tab_spaces = 2`).
- Checks that must pass before finishing:
  1. `cargo fmt --all --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace`
  4. `cargo build -p <package> --release --target wasm32-wasip2`
  5. `./run-local.sh template/samples/` (or `./run-local.sh --package <name> template/samples/`;
     set `NBMQ_IMAGE` to override the image or `NBMQ_BIN=/path/to/newboot-mq` to use a local binary)
- **Do not push** to a registry or git remote unless explicitly asked by the user. Show the user
  the `run-local.sh` output (message in, message out, logs, and resource usage) so they can review
  it first.

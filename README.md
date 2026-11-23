# superset-function-sdk-rs

Rust SDK, starter template, examples, and local runner for **Newboot MQ Functions** over the
`wasm-graph:processor@1.1.0` WebAssembly Component interface.

## Layout

```text
src/, macros/, wit/          superset-function-sdk crate (`use newboot::...`)
AGENTS.md                    function rules for coding agents (Claude Code, etc.)
template/                    starter #[map] function (corrosion-risk), samples/, push.yml
examples/
  rename-convert/            rename fields and convert units
  uns-topic/                 build an ISA-95 UNS topic
  drop-below/                #[filter] dropping values below a minimum
  running-average/           bounded sliding-window running average
  rule-mapper/               declarative JSON rule mapper
run-local.sh                 runs any built .wasm in a real local Newboot MQ container
```

## Install

```toml
[dependencies]
superset-function-sdk = { git = "https://github.com/Newboot-io/superset-function-sdk-rs", tag = "v0.1.0" }
```

The library crate name is `newboot`.

## Quick start

```rust
use newboot::{map, Call, Message, Settings};

#[derive(Settings)]
struct Weights {
  /// zone:weight pairs, for example "z1:0.5,z3:1.2"
  zone_weights: String,
}

#[map]
fn corrosion_risk(Call { message, settings }: Call<Weights>) -> newboot::Result<Message> {
  let mut doc = message.json()?;
  doc["zoneWeights"] = settings.zone_weights.into();
  newboot::log::debug!("applied zone weights");
  newboot::metrics::increment("processed");
  Ok(message.with_json(doc))
}
```

## Run locally against Newboot MQ

```sh
cargo test --workspace
./run-local.sh template/samples/
./run-local.sh --watch template/samples/
```

Run an example or test an invalid sample:

```sh
./run-local.sh --package drop-below --setting minValue=0.5 template/samples/
./run-local.sh template/samples/invalid/no-zone.json
```

`run-local.sh` defaults to `NBMQ_IMAGE=ghcr.io/newboot-io/newboot-mq:latest`. Override it with
`--image <ref>` / `NBMQ_IMAGE=<ref>`, or point `NBMQ_BIN=/path/to/newboot-mq` to a local binary
built with `--features wasm,dataflow`.

## Push to your registry

Copy `template/.github/workflows/push.yml` into your function repository (pushing a `v*` tag
builds, pushes with ORAS, and prints `registry/repository@sha256:…`), or push by hand:

```sh
cargo build -p corrosion-risk --release --target wasm32-wasip2
oras push --artifact-type application/vnd.newboot.function.v1 \
  yourregistry.azurecr.io/functions/corrosion-risk:1.0.0 \
  template/function.toml:application/toml \
  target/wasm32-wasip2/release/corrosion_risk.wasm:application/wasm
```

## Checks

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --package superset-function-sdk --lib --examples --target wasm32-wasip2 -- -D warnings
cargo test --workspace
cargo build --workspace --exclude superset-function-sdk --exclude superset-function-sdk-macros --release --target wasm32-wasip2
cargo llvm-cov --package superset-function-sdk --fail-under-lines 100
cargo audit --deny warnings
```

## License

Apache-2.0: see [LICENSE](LICENSE).

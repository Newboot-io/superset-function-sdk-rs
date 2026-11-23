# `wasm-graph:processor@1.1.0` (vendored)

Vendored `wasm-graph:processor@1.1.0` WIT definitions, copied from
[Azure-Samples/explore-iot-operations](https://github.com/Azure-Samples/explore-iot-operations/tree/4a107ee9f169ff90916f423037464ae607960875/samples/wasm-python/schema)
at commit `4a107ee9f169ff90916f423037464ae607960875` (MIT, see `LICENSE.md`).

One local edit: `map.wit` spells the interface `%map`. Newer WIT parsers
(wasmtime 49) reserve `map` as a keyword; `%` only escapes the identifier, so
the exported name stays `wasm-graph:processor/map@1.1.0` and the ABI is
unchanged.

Do not edit these files otherwise so compiled modules stay wire-compatible with
`wasm-graph:processor@1.1.0`. Re-vendor from upstream on an ABI bump.

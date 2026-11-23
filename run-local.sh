#!/usr/bin/env bash
# Build a function to wasm32-wasip2 and run it inside a real local Newboot MQ
# container against one or more sample JSON files.
#
# Usage:
#   ./run-local.sh [--watch] [--package <name>] [--setting key=value]... [--image <ref>] [--] <sample-or-dir>...
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

WATCH=0
PACKAGE=""
IMAGE="${NBMQ_IMAGE:-ghcr.io/newboot-io/newboot-mq:latest}"
SETTINGS=()
SAMPLES=()

require_arg() {
  if [ "$2" -lt 2 ]; then
    echo "error: $1 requires a value" >&2
    exit 2
  fi
}

while [ $# -gt 0 ]; do
  case "$1" in
    --watch)
      WATCH=1
      shift
      ;;
    --package)
      require_arg "$1" $#
      PACKAGE="$2"
      shift 2
      ;;
    --setting)
      require_arg "$1" $#
      SETTINGS+=("--setting" "$2")
      shift 2
      ;;
    --image)
      require_arg "$1" $#
      IMAGE="$2"
      shift 2
      ;;
    -h|--help)
      sed -n '2,8p' "$0"
      exit 0
      ;;
    --)
      shift
      while [ $# -gt 0 ]; do
        SAMPLES+=("$1")
        shift
      done
      ;;
    -*)
      echo "error: unknown option: $1" >&2
      exit 2
      ;;
    *)
      SAMPLES+=("$1")
      shift
      ;;
  esac
done

if [ "${#SAMPLES[@]}" -eq 0 ]; then
  SAMPLES=("template/samples/")
fi

PKG_DIR="template"
CARGO_ARGS=(build --release --target wasm32-wasip2)
if [ -n "$PACKAGE" ]; then
  if ! [[ "$PACKAGE" =~ ^[A-Za-z0-9_-]+$ ]]; then
    echo "error: invalid --package name: $PACKAGE" >&2
    exit 2
  fi
  CARGO_ARGS+=(-p "$PACKAGE")
  if [ -d "examples/$PACKAGE" ]; then
    PKG_DIR="examples/$PACKAGE"
  elif [ "$PACKAGE" != "corrosion-risk" ] && [ -d "$PACKAGE" ]; then
    PKG_DIR="$PACKAGE"
  fi
else
  CRATE_NAME="$(awk -F'"' '/^name[[:space:]]*=/ {print $2; exit}' "$PKG_DIR/Cargo.toml")"
  CARGO_ARGS+=(-p "$CRATE_NAME")
fi

wasm_name() {
  local crate_name
  crate_name="$(awk -F'"' '/^name[[:space:]]*=/ {print $2; exit}' "$PKG_DIR/Cargo.toml")"
  printf '%s.wasm' "${crate_name//-/_}"
}

run_once() {
  cargo "${CARGO_ARGS[@]}" --quiet
  local target_dir="${CARGO_TARGET_DIR:-$ROOT/target}"
  local wasm_file="$target_dir/wasm32-wasip2/release/$(wasm_name)"
  if [ ! -f "$wasm_file" ]; then
    wasm_file="$(find "$target_dir" -name "$(wasm_name)" -path '*/wasm32-wasip2/release/*' 2>/dev/null | head -n 1)"
  fi
  python3 "$ROOT/scripts/run_local.py" \
    --wasm "$wasm_file" \
    --manifest "$PKG_DIR/function.toml" \
    --image "$IMAGE" \
    ${SETTINGS[@]+"${SETTINGS[@]}"} \
    -- "${SAMPLES[@]}"
}

fingerprint() {
  python3 - "$PKG_DIR/src" "$PKG_DIR/Cargo.toml" "$PKG_DIR/function.toml" "${SAMPLES[@]}" <<'PY'
import hashlib, os, sys
h = hashlib.sha256()
entries = []
for raw in sys.argv[1:]:
  if os.path.isdir(raw):
    for root, dirs, files in os.walk(raw):
      dirs.sort()
      for name in sorted(files):
        p = os.path.join(root, name)
        try:
          st = os.stat(p)
          entries.append(f"{st.st_mtime_ns}:{st.st_size}:{p}")
        except OSError:
          pass
  elif os.path.isfile(raw):
    try:
      st = os.stat(raw)
      entries.append(f"{st.st_mtime_ns}:{st.st_size}:{raw}")
    except OSError:
      pass
for e in sorted(entries):
  h.update(e.encode("utf-8"))
  h.update(b"\n")
print(h.hexdigest())
PY
}

if [ "$WATCH" -eq 0 ]; then
  run_once
else
  LAST=""
  while true; do
    CUR="$(fingerprint)"
    if [ "$CUR" != "$LAST" ]; then
      LAST="$CUR"
      printf '\n--- %s ---\n' "$(date +%H:%M:%S)"
      run_once || true
    fi
    sleep 1
  done
fi

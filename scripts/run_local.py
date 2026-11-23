#!/usr/bin/env python3
"""Run a built Newboot MQ Function (.wasm) inside a real local newboot-mq instance
against one or more sample JSON files and print in/out, dropped, logs, errors, and
time/compute/memory against cluster limits."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import tomllib
import urllib.request
from pathlib import Path

FUEL_LIMIT = 10_000_000
MEMORY_LIMIT_BYTES = 16 * 1024 * 1024
TIMEOUT_LIMIT_MS = 100.0
MAX_INPUT_BYTES = 64 * 1024
MAX_EVENTHUB_OUTPUT_BYTES = 8 * 1024


def find_free_port() -> int:
  with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind(("127.0.0.1", 0))
    return int(s.getsockname()[1])


def parse_function_toml(path: Path) -> dict:
  with path.open("rb") as fh:
    doc = tomllib.load(fh)
  defaults: dict[str, str] = {}
  for entry in doc.get("settings", []):
    if isinstance(entry, dict) and "key" in entry and "default" in entry:
      defaults[str(entry["key"])] = str(entry["default"])
  return {
    "name": str(doc.get("name", path.parent.name)),
    "version": str(doc.get("version", "0.1.0")),
    "runs_as": str(doc.get("runs_as", "map")),
    "defaults": defaults,
  }


def toml_quote(s: str) -> str:
  out = ['"']
  for ch in s:
    code = ord(ch)
    if ch == "\\":
      out.append("\\\\")
    elif ch == '"':
      out.append('\\"')
    elif ch == "\b":
      out.append("\\b")
    elif ch == "\f":
      out.append("\\f")
    elif ch == "\n":
      out.append("\\n")
    elif ch == "\r":
      out.append("\\r")
    elif ch == "\t":
      out.append("\\t")
    elif code < 0x20 or code == 0x7F:
      out.append(f"\\u{code:04X}")
    else:
      out.append(ch)
  out.append('"')
  return "".join(out)


def render_dataflow_toml(
  wasm_path_in_broker: str,
  digest: str,
  meta: dict,
  settings: dict[str, str],
) -> str:
  lines = [
    "[[endpoints]]",
    'name = "local"',
    'endpointType = "Mqtt"',
    "",
    "[[dataflows]]",
    'name = "local-test"',
    'logLevel = "trace"',
    "",
    "[[dataflows.operations]]",
    'operationType = "Source"',
    'sourceSettings = { endpointRef = "local", dataSources = ["local/in"] }',
    "",
    "[[dataflows.operations]]",
    'operationType = "Graph"',
    "[dataflows.operations.graphSettings]",
    f'operator = {toml_quote(meta["runs_as"])}',
    f"module = {toml_quote(wasm_path_in_broker)}",
    f"digest = {toml_quote(digest)}",
    f'function = {toml_quote(meta["name"])}',
    f'version = {toml_quote(meta["version"])}',
    "",
    "[dataflows.operations.graphSettings.configuration]",
  ]
  for k in sorted(settings):
    lines.append(f"{toml_quote(k)} = {toml_quote(settings[k])}")
  lines += [
    "",
    "[[dataflows.operations]]",
    'operationType = "Destination"',
    'destinationSettings = { endpointRef = "local", dataDestination = "local/out" }',
    "",
  ]
  return "\n".join(lines)


# --- Minimal stdlib-only MQTT 5 client (CONNECT, SUBSCRIBE, PUBLISH QoS 0) ---


def encode_varint(n: int) -> bytes:
  out = bytearray()
  while True:
    b = n & 0x7F
    n >>= 7
    if n:
      b |= 0x80
    out.append(b)
    if not n:
      return bytes(out)


def decode_varint(sock: socket.socket) -> int:
  mul = 1
  val = 0
  for _ in range(4):
    b = recv_exact(sock, 1)[0]
    val += (b & 0x7F) * mul
    if not (b & 0x80):
      return val
    mul *= 128
  raise ValueError("malformed MQTT variable byte integer")


def recv_exact(sock: socket.socket, n: int) -> bytes:
  buf = bytearray()
  while len(buf) < n:
    chunk = sock.recv(n - len(buf))
    if not chunk:
      raise EOFError("MQTT connection closed")
    buf.extend(chunk)
  return bytes(buf)


def encode_str(s: str) -> bytes:
  b = s.encode("utf-8")
  return struct.pack("!H", len(b)) + b


class MiniMqtt5:
  def __init__(self, host: str, port: int) -> None:
    self.sock = socket.create_connection((host, port), timeout=5.0)
    self.pkt_id = 1
    self._connect()

  def _send_packet(self, header: int, body: bytes) -> None:
    self.sock.sendall(bytes([header]) + encode_varint(len(body)) + body)

  def _read_packet(self, timeout: float = 1.0) -> tuple[int, bytes] | None:
    self.sock.settimeout(timeout)
    try:
      first = self.sock.recv(1)
    except socket.timeout:
      return None
    if not first:
      return None
    self.sock.settimeout(2.0)
    rem = decode_varint(self.sock)
    body = recv_exact(self.sock, rem) if rem else b""
    return first[0], body

  def _connect(self) -> None:
    vh = encode_str("MQTT") + bytes([5, 0x02]) + struct.pack("!H", 30) + b"\x00"
    pl = encode_str("run-local")
    self._send_packet(0x10, vh + pl)
    pkt = self._read_packet(timeout=5.0)
    if not pkt or (pkt[0] >> 4) != 2:
      raise RuntimeError("expected MQTT 5 CONNACK")

  def subscribe(self, topic: str) -> None:
    pid = self.pkt_id
    self.pkt_id += 1
    vh = struct.pack("!H", pid) + b"\x00"
    pl = encode_str(topic) + bytes([0x00])
    self._send_packet(0x82, vh + pl)
    pkt = self._read_packet(timeout=5.0)
    if not pkt or (pkt[0] >> 4) != 9:
      raise RuntimeError("expected MQTT 5 SUBACK")

  def publish(self, topic: str, payload: bytes) -> None:
    vh = encode_str(topic) + b"\x00"
    self._send_packet(0x30, vh + payload)

  def recv_publish(self, timeout: float = 0.4) -> tuple[str, bytes] | None:
    deadline = time.monotonic() + timeout
    while True:
      left = deadline - time.monotonic()
      if left <= 0:
        return None
      pkt = self._read_packet(timeout=left)
      if pkt is None:
        return None
      hdr, body = pkt
      if (hdr >> 4) != 3 or len(body) < 2:
        continue
      qos = (hdr >> 1) & 0x03
      tlen = struct.unpack("!H", body[:2])[0]
      pos = 2 + tlen
      if pos > len(body):
        continue
      topic = body[2:pos].decode("utf-8", errors="replace")
      if qos > 0:
        pos += 2
      if pos >= len(body):
        continue
      mul = 1
      prop_len = 0
      valid = False
      for _ in range(4):
        if pos >= len(body):
          break
        b = body[pos]
        pos += 1
        prop_len += (b & 0x7F) * mul
        if not (b & 0x80):
          valid = True
          break
        mul *= 128
      if not valid or pos + prop_len > len(body):
        continue
      pos += prop_len
      return topic, body[pos:]

  def close(self) -> None:
    try:
      self.sock.close()
    except OSError:
      pass


# --- Prometheus metrics scraper ---


def scrape_metrics(port: int) -> dict:
  url = f"http://127.0.0.1:{port}/metrics"
  with urllib.request.urlopen(url, timeout=2.0) as resp:
    text = resp.read().decode("utf-8", errors="replace")
  out = {
    "duration_sum": 0.0,
    "fuel": 0.0,
    "memory": 0.0,
    "messages_in": 0.0,
    "messages_out": 0.0,
    "invocations": {},
    "errors": {},
    "flow_failed": None,
  }
  for line in text.splitlines():
    if not line or line.startswith("#"):
      continue
    if line.startswith("nbmq_wasm_execution_duration_seconds_sum"):
      out["duration_sum"] += float(line.split()[-1])
    elif line.startswith("nbmq_wasm_fuel_consumed_total"):
      out["fuel"] += float(line.split()[-1])
    elif line.startswith("nbmq_wasm_memory_bytes"):
      out["memory"] = max(out["memory"], float(line.split()[-1]))
    elif line.startswith("nbmq_wasm_messages_in_total"):
      out["messages_in"] += float(line.split()[-1])
    elif line.startswith("nbmq_wasm_messages_out_total"):
      out["messages_out"] += float(line.split()[-1])
    elif line.startswith("nbmq_wasm_invocations_total"):
      m = re.search(r'outcome="([^"]+)"', line)
      if m:
        k = m.group(1)
        out["invocations"][k] = out["invocations"].get(k, 0.0) + float(line.split()[-1])
    elif line.startswith("nbmq_wasm_errors_total"):
      m = re.search(r'reason="([^"]+)"', line)
      if m:
        k = m.group(1)
        out["errors"][k] = out["errors"].get(k, 0.0) + float(line.split()[-1])
    elif line.startswith("nbmq_flow_status{") and 'state="failed"' in line:
      val = float(line.split()[-1])
      if val > 0:
        m = re.search(r'reason="([^"]+)"', line)
        out["flow_failed"] = m.group(1) if m else "failed"
  return out


def parse_guest_logs(lines: list[str]) -> tuple[list[tuple[str, str]], list[str]]:
  guest_logs: list[tuple[str, str]] = []
  error_details: list[str] = []
  for raw in lines:
    clean = re.sub(r"\x1b\[[0-9;]*m", "", raw).strip()
    if not clean:
      continue
    if clean.startswith("{"):
      try:
        ev = json.loads(clean)
      except json.JSONDecodeError:
        ev = None
      if isinstance(ev, dict):
        if ev.get("target") == "wasm_guest":
          lvl = str(ev.get("level", "info")).lower()
          msg = str(ev.get("guest_message", ""))
          if len(msg) >= 2 and msg[0] == '"' and msg[-1] == '"':
            msg = msg[1:-1]
          guest_logs.append((lvl, msg))
          continue
        err = ev.get("error") or ev.get("err")
        if err and ("wasm" in str(ev.get("target", "")) or "dataflow" in str(ev.get("target", ""))):
          error_details.append(str(err).strip('"'))
          continue
    if "wasm_guest" in clean:
      lvl_m = re.search(r"\b(TRACE|DEBUG|INFO|WARN|ERROR)\b", clean)
      msg_m = re.search(r'guest_message="((?:\\.|[^"\\])*)"', clean)
      if msg_m:
        lvl = lvl_m.group(1).lower() if lvl_m else "info"
        guest_logs.append((lvl, msg_m.group(1)))
    elif "operator" in clean and (
      "error" in clean.lower() or "trap" in clean.lower() or "failed" in clean.lower()
    ):
      err_m = re.search(r'(?:error|err)="?([^"\n]+)"?', clean)
      if err_m:
        error_details.append(err_m.group(1).strip())
  return guest_logs, error_details


def collect_samples(paths: list[str]) -> list[Path]:
  files: list[Path] = []
  for raw in paths:
    p = Path(raw)
    if p.is_dir():
      files.extend(sorted(p.glob("*.json")))
    elif p.is_file():
      files.append(p)
    else:
      raise SystemExit(f"sample path not found: {raw}")
  if not files:
    raise SystemExit("no .json sample files found")
  return files


def validate_image_ref(image: str) -> None:
  if not image or image.startswith("-") or any(c.isspace() for c in image):
    raise SystemExit(f"error: invalid container image reference: {image!r}")


def main() -> int:
  parser = argparse.ArgumentParser()
  parser.add_argument("--wasm", required=True)
  parser.add_argument("--manifest", required=True)
  parser.add_argument("--image", default="ghcr.io/newboot-io/newboot-mq:latest")
  parser.add_argument("--setting", action="append", default=[])
  parser.add_argument("samples", nargs="+")
  args = parser.parse_args()

  wasm_path = Path(args.wasm).resolve()
  manifest_path = Path(args.manifest).resolve()
  if not wasm_path.is_file():
    print(f"error: wasm artifact not found: {wasm_path}", file=sys.stderr)
    return 1

  meta = parse_function_toml(manifest_path)
  settings: dict[str, str] = dict(meta["defaults"])
  for kv in args.setting:
    if "=" not in kv:
      print(f"error: --setting must be key=value, got {kv!r}", file=sys.stderr)
      return 1
    k, v = kv.split("=", 1)
    settings[k] = v

  wasm_bytes = wasm_path.read_bytes()
  digest = "sha256:" + hashlib.sha256(wasm_bytes).hexdigest()
  sample_files = collect_samples(args.samples)

  mqtt_port = find_free_port()
  peer_port = find_free_port()
  ready_port = find_free_port()
  metrics_port = find_free_port()

  nbmq_bin = os.environ.get("NBMQ_BIN")
  if not nbmq_bin:
    validate_image_ref(args.image)

  tmpdir = Path(tempfile.mkdtemp(prefix="nbmq-run-local-"))
  if "," in str(tmpdir):
    shutil.rmtree(tmpdir, ignore_errors=True)
    print(f"error: temporary directory path must not contain ',': {tmpdir}", file=sys.stderr)
    return 1

  log_lines: list[str] = []
  log_lock = threading.Lock()
  proc: subprocess.Popen[str] | None = None
  container_name = f"nbmq-run-local-{os.getpid()}"

  try:
    wasm_copy = tmpdir / wasm_path.name
    shutil.copy2(wasm_path, wasm_copy)
    cfg_path = tmpdir / "dataflow.toml"
    bind_host = "127.0.0.1" if nbmq_bin else "0.0.0.0"
    if nbmq_bin:
      broker_wasm = str(wasm_copy)
      broker_cfg = str(cfg_path)
    else:
      broker_wasm = f"/run-local/{wasm_path.name}"
      broker_cfg = "/run-local/dataflow.toml"
    cfg_path.write_text(
      render_dataflow_toml(broker_wasm, digest, meta, settings),
      encoding="utf-8",
    )

    env_pairs = {
      "NBMQ_LISTEN": f"{bind_host}:{mqtt_port}",
      "NBMQ_PEER_LISTEN": f"{bind_host}:{peer_port}",
      "NBMQ_READY_LISTEN": f"{bind_host}:{ready_port}",
      "NBMQ_METRICS_LISTEN": f"{bind_host}:{metrics_port}",
      "NBMQ_DATAFLOW_CONFIG": broker_cfg,
      "NBMQ_PEERS": "",
      "NBMQ_LOG_FORMAT": "json",
      "RUST_LOG": "info,wasm_guest=trace",
    }

    if nbmq_bin:
      env = dict(os.environ)
      env.update(env_pairs)
      proc = subprocess.Popen(
        [nbmq_bin],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        env=env,
      )
    else:
      cmd = [
        "docker",
        "run",
        "--rm",
        "--name",
        container_name,
        "-p",
        f"127.0.0.1:{mqtt_port}:{mqtt_port}",
        "-p",
        f"127.0.0.1:{metrics_port}:{metrics_port}",
      ]
      for k, v in env_pairs.items():
        cmd += ["-e", f"{k}={v}"]
      cmd += [
        "--mount",
        f"type=bind,source={tmpdir},target=/run-local,readonly",
        args.image,
      ]
      proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
      )

    def reader() -> None:
      assert proc is not None and proc.stdout is not None
      for line in proc.stdout:
        with log_lock:
          log_lines.append(line.rstrip("\n"))

    t = threading.Thread(target=reader, daemon=True)
    t.start()

    deadline = time.monotonic() + 15.0
    ready = False
    while time.monotonic() < deadline:
      if proc.poll() is not None:
        break
      try:
        scrape_metrics(metrics_port)
        ready = True
        break
      except Exception:
        time.sleep(0.1)

    if not ready:
      with log_lock:
        tail = "\n".join(log_lines[-30:])
      print(f"error: newboot-mq did not start cleanly:\n{tail}", file=sys.stderr)
      return 1

    time.sleep(0.03)
    m0 = scrape_metrics(metrics_port)
    if m0["flow_failed"]:
      with log_lock:
        init_logs, init_errs = parse_guest_logs(log_lines)
      print(f"error {m0['flow_failed']} in {meta['name']}: flow failed to load")
      for lvl, msg in init_logs:
        print(f"  log {lvl} {msg}")
      for detail in init_errs:
        print(f"  detail {detail}")
      return 1

    client = MiniMqtt5("127.0.0.1", mqtt_port)
    client.subscribe("local/out")

    pad = max(len(p.name) for p in sample_files) + 2
    had_error = False
    for sample in sample_files:
      raw_payload = sample.read_bytes().strip()
      label = sample.name.ljust(pad)
      indent = " " * pad
      in_str = raw_payload.decode("utf-8", errors="replace")
      print(f"{label}in  {in_str}")

      if len(raw_payload) > MAX_INPUT_BYTES:
        had_error = True
        print(
          f"{indent}error input_too_large in {meta['name']}: "
          f"{len(raw_payload)} bytes exceeds {MAX_INPUT_BYTES} byte limit"
        )
        continue

      before = scrape_metrics(metrics_port)
      with log_lock:
        log_start = len(log_lines)

      client.publish("local/in", raw_payload)
      received = client.recv_publish(timeout=0.5)
      # Allow broker log pipe and metrics counters to settle before reading.
      time.sleep(0.04)
      after = scrape_metrics(metrics_port)
      with log_lock:
        new_logs = log_lines[log_start:]

      guest_logs, err_details = parse_guest_logs(new_logs)
      dur_ms = max(0.0, (after["duration_sum"] - before["duration_sum"]) * 1000.0)
      fuel_delta = max(0.0, after["fuel"] - before["fuel"])
      fuel_pct = (fuel_delta / FUEL_LIMIT) * 100.0
      mem_mib = after["memory"] / (1024.0 * 1024.0)

      err_reason = None
      for reason, val in after["errors"].items():
        if val > before["errors"].get(reason, 0.0):
          err_reason = reason
          break
      in_delta = after["messages_in"] - before["messages_in"]
      out_delta = after["messages_out"] - before["messages_out"]
      dropped = (
        after["invocations"].get("dropped", 0.0) > before["invocations"].get("dropped", 0.0)
        or after["invocations"].get("drop", 0.0) > before["invocations"].get("drop", 0.0)
        or (in_delta > 0 and out_delta == 0 and err_reason is None)
      )

      if received is not None:
        _topic, out_bytes = received
        if len(out_bytes) > MAX_EVENTHUB_OUTPUT_BYTES:
          had_error = True
          print(
            f"{indent}error output_too_large in {meta['name']}: "
            f"{len(out_bytes)} bytes exceeds {MAX_EVENTHUB_OUTPUT_BYTES} byte Event Hubs limit"
          )
        else:
          out_str = out_bytes.decode("utf-8", errors="replace")
          print(f"{indent}out {out_str}")
      elif err_reason is not None:
        had_error = True
        detail = f": {err_details[0]}" if err_details else ""
        print(f"{indent}error {err_reason} in {meta['name']}{detail}")
      elif dropped:
        print(f"{indent}dropped by filter")
      else:
        had_error = True
        print(f"{indent}error no output received within 500 ms")

      for lvl, msg in guest_logs:
        print(f"{indent}log {lvl} {msg}")

      print(
        f"{indent}{dur_ms:.2f} ms (limit {TIMEOUT_LIMIT_MS:.0f} ms) · "
        f"{fuel_pct:.1f} % of compute · "
        f"{mem_mib:.1f} MiB (limit {MEMORY_LIMIT_BYTES // (1024 * 1024)} MiB)"
      )

    client.close()
    return 1 if had_error else 0
  finally:
    if proc is not None and proc.poll() is None:
      if not nbmq_bin:
        subprocess.run(
          ["docker", "rm", "-f", container_name],
          stdout=subprocess.DEVNULL,
          stderr=subprocess.DEVNULL,
          check=False,
        )
      proc.send_signal(signal.SIGTERM)
      try:
        proc.wait(timeout=3)
      except subprocess.TimeoutExpired:
        proc.kill()
    shutil.rmtree(tmpdir, ignore_errors=True)


if __name__ == "__main__":
  sys.exit(main())

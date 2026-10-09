#!/usr/bin/env bash
set -euo pipefail

python3 - <<'PY'
import tomllib
from pathlib import Path

root = Path.cwd()
expected = {
    "i2pr-mail-domain": set(),
    "i2pr-mail-mime": {"i2pr-mail-domain"},
    "i2pr-mail-proto": {"i2pr-mail-domain"},
    "i2pr-mail-store": {"i2pr-mail-domain"},
    "i2pr-mail-runtime": {"i2pr-mail-domain", "i2pr-mail-mime", "i2pr-mail-proto", "i2pr-mail-store"},
    "i2pr-mail-sam": set(),
    "i2pr-mail-managed-app": set(),
}
def dependency_set_is_valid(actual, allowed):
    return actual == allowed

if dependency_set_is_valid(expected["i2pr-mail-domain"] | {"tokio"}, expected["i2pr-mail-domain"]):
    raise SystemExit("boundary guard positive control failed to reject an injected dependency")
for name, allowed in expected.items():
    manifest = tomllib.loads((root / "crates" / name.removeprefix("i2pr-") / "Cargo.toml").read_text())
    deps = set(manifest.get("dependencies", {}))
    project = {d for d in deps if d.startswith("i2pr-mail-")}
    if not dependency_set_is_valid(project, allowed):
        raise SystemExit(f"{name}: project dependencies {sorted(project)} != {sorted(allowed)}")
    if name in {"i2pr-mail-domain", "i2pr-mail-mime", "i2pr-mail-proto", "i2pr-mail-sam"}:
        source = "\n".join(p.read_text() for p in (root / "crates" / name.removeprefix("i2pr-") / "src").glob("*.rs"))
        forbidden = ("std::net", "tokio::", "async_std::", "std::fs", "std::process", "std::time", "tauri", "gtk::")
        found = [word for word in forbidden if word in source]
        if found:
            raise SystemExit(f"{name}: forbidden capability references {found}")
print("crate dependency and source boundary checks passed")
PY

python3 - <<'PY'
import re
from pathlib import Path

root = Path.cwd()
# Durable state must stay typed at the public store and runtime API boundary. An
# arbitrary-string state or integer stage parameter would let any caller persist a
# combination the domain model cannot represent. Internal decoders still read raw
# persisted text, so only public signatures are inspected.
forbidden = ("state: &str", "state: String", "stage: i64", "receive_state: String")
signature = re.compile(r"pub (?:fn|trait)\s+[\s\S]*?\([^()]*(?:\([^()]*\)[^()]*)*\)")

def untyped_state_declarations(source):
    return [token for token in forbidden if token in source]

if not untyped_state_declarations("pub fn set_outbox_stage(id: &str, stage: i64, state: &str);"):
    raise SystemExit("typed-state guard positive control failed to reject an untyped signature")
if untyped_state_declarations("pub fn set_outbox_stage(id: &str, progress: SubmissionProgress, state: SubmissionState);"):
    raise SystemExit("typed-state guard positive control rejected a typed signature")

for crate in ("mail-store", "mail-runtime"):
    for path in sorted((root / "crates" / crate / "src").rglob("*.rs")):
        source = path.read_text()
        for match in signature.finditer(source):
            found = untyped_state_declarations(match.group(0))
            if found:
                raise SystemExit(
                    f"{path}: untyped durable-state API {found}; use mail-domain state types"
                )
print("typed durable-state API check passed")
PY

python3 - <<'PY'
import tomllib
from pathlib import Path

# The SAM 3.1 codec is the client half of the future i2pr adapter. It sits at or
# above the transport seam and must stay a pure state machine over bytes: it
# opens no socket, reads no clock, and touches no filesystem. It is deliberately
# NOT a dependency of any existing crate, so nothing below the seam acquires a
# router-protocol type.
root = Path.cwd()
manifest_path = root / "crates" / "mail-sam" / "Cargo.toml"
manifest = tomllib.loads(manifest_path.read_text())
deps = set(manifest.get("dependencies", {}))
if deps:
    raise SystemExit(f"i2pr-mail-sam must have no dependencies, found {sorted(deps)}")

if not manifest_path.exists():
    raise SystemExit("i2pr-mail-sam boundary guard positive control passed with no manifest")

source_dir = root / "crates" / "mail-sam" / "src"
if not source_dir.is_dir():
    raise SystemExit("i2pr-mail-sam boundary guard positive control passed with no source")

runtime_forbidden = ("async fn", "tokio::", "std::net", "std::fs", "std::process", "std::time")
for path in sorted(source_dir.rglob("*.rs")):
    text = path.read_text()
    found = [word for word in runtime_forbidden if word in text]
    if found:
        raise SystemExit(f"{path}: SAM codec is sans-I/O but references {found}")

# No crate below the transport seam may depend on the router protocol codec.
for crate in ("mail-domain", "mail-mime", "mail-proto", "mail-store", "mail-runtime"):
    lower = tomllib.loads((root / "crates" / crate / "Cargo.toml").read_text())
    if "i2pr-mail-sam" in set(lower.get("dependencies", {})):
        raise SystemExit(f"{crate} must not depend on i2pr-mail-sam")
    if "i2pr-mail-managed-app" in set(lower.get("dependencies", {})):
        raise SystemExit(f"{crate} must not depend on i2pr-mail-managed-app")

print("SAM codec sans-I/O and seam-isolation checks passed")
PY

python3 - <<'PY'
import tomllib
from pathlib import Path

# The managed-app v1 client/multiplexer is adapter-layer async code over
# injected byte I/O. It must speak only the documented application-role wire
# contract: no direct network, DNS, loopback, filesystem, process-launch, or
# router-control authority, no ambient stdio/env identity, and no unbounded
# channel constructor. Lower crates must not gain a dependency on it.
root = Path.cwd()
manifest_path = root / "crates" / "mail-managed-app" / "Cargo.toml"
manifest = tomllib.loads(manifest_path.read_text())
deps = set(manifest.get("dependencies", {}))
project = {d for d in deps if d.startswith("i2pr-mail-")}
if project:
    raise SystemExit(f"i2pr-mail-managed-app must have no project dependencies, found {sorted(project)}")
allowed_external = {"serde", "serde_json", "thiserror", "tokio"}
external = {d for d in deps if not d.startswith("i2pr-mail-")}
unexpected = {d for d in external if d not in allowed_external}
if unexpected:
    raise SystemExit(f"i2pr-mail-managed-app has unexpected dependencies {sorted(unexpected)}")

if not manifest_path.exists():
    raise SystemExit("managed-app boundary guard positive control passed with no manifest")

source_dir = root / "crates" / "mail-managed-app" / "src"
if not source_dir.is_dir():
    raise SystemExit("managed-app boundary guard positive control passed with no source")

# Async is confined here by design (ADR-0002 allows exactly one adapter owner),
# so `tokio::` alone is not forbidden. Only ambient/direct authority and
# unbounded construction are rejected.
authority_forbidden = (
    "std::net",
    "tokio::net",
    "tokio::fs",
    "std::fs",
    "std::process",
    "tokio::process",
    "std::env",
    "std::os::",
    "std::time",
    "unbounded_channel",
    "TcpStream",
    "UdpSocket",
    "lookup_host",
)
for path in sorted(source_dir.rglob("*.rs")):
    text = path.read_text()
    found = [word for word in authority_forbidden if word in text]
    if found:
        raise SystemExit(f"{path}: managed-app client has direct authority references {found}")

# Lower crates must not depend on the adapter; the adapter must not depend on
# lower mail crates (it speaks only the wire contract).
for crate in ("mail-domain", "mail-mime", "mail-proto", "mail-store", "mail-runtime", "mail-sam"):
    lower = tomllib.loads((root / "crates" / crate / "Cargo.toml").read_text())
    if "i2pr-mail-managed-app" in set(lower.get("dependencies", {})):
        raise SystemExit(f"{crate} must not depend on i2pr-mail-managed-app")

print("managed-app adapter authority and seam-isolation checks passed")
PY
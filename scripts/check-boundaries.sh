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
    if name in {"i2pr-mail-domain", "i2pr-mail-mime", "i2pr-mail-proto"}:
        source = "\n".join(p.read_text() for p in (root / "crates" / name.removeprefix("i2pr-") / "src").glob("*.rs"))
        forbidden = ("std::net", "tokio::", "async_std::", "std::fs", "std::process", "tauri", "gtk::")
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
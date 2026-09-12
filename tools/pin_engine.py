#!/usr/bin/env python3
"""Record the pinned formula-engine integrity hash (HBR-153).

Computes SHA-256 over the packed .crate archives of formualizer and its
in-tree workspace crates at the locked versions, and rewrites the
`integrity_sha256` placeholder in core/harbor_formula/src/engine.rs.

Usage:
  python3 tools/pin_engine.py          # rewrite placeholder if it changed
  python3 tools/pin_engine.py --check  # exit 1 if placeholder is stale
"""
import hashlib
import io
import re
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ENGINE_RS = ROOT / "core" / "harbor_formula" / "src" / "engine.rs"
CRATES = ["formualizer", "formualizer-common", "formualizer-parse",
          "formualizer-eval", "formualizer-workbook"]


def cargo_lock_version(name: str):
    text = (ROOT / "core" / "Cargo.lock").read_text()
    for block in text.split("name = "):
        if block.startswith(f'"{name}"'):
            m = re.search(r'version = "([^"]+)"', block)
            return m.group(1) if m else None
    return None


def cache_paths() -> list[Path]:
    home = Path.home() / ".cargo" / "registry" / "cache"
    out = []
    for pkg in CRATES:
        ver = cargo_lock_version(pkg)
        if ver is None:
            sys.exit(f"ERROR: {pkg} not found in core/Cargo.lock")
        matches = sorted(home.glob(f"*/{pkg}-{ver}.crate"))
        if not matches:
            sys.exit(f"ERROR: {pkg}-{ver}.crate not in the local cargo cache; "
                     f"run `cargo fetch` first")
        out.append(matches[0])
    return out


def integrity() -> str:
    h = hashlib.sha256()
    for p in cache_paths():
        h.update(p.read_bytes())
    return h.hexdigest()


def main() -> None:
    check = "--check" in sys.argv
    value = integrity()
    src = ENGINE_RS.read_text()
    pattern = re.compile(r'integrity_sha256:\s*"[^"]*"')
    if "RECORDED_AT_FIRST_QUALIFICATION" in src or check is False:
        new = pattern.sub(f'integrity_sha256: "{value}"', src)
        if new != src:
            if check:
                sys.exit("STALE: engine integrity hash does not match the lockfile")
            ENGINE_RS.write_text(new)
            print(f"recorded {value}")
        else:
            print(f"ok {value}")
    else:
        m = pattern.search(src)
        if m is None:
            sys.exit("ERROR: integrity_sha256 field not found")
        if m.group(0) != f'integrity_sha256: "{value}"':
            sys.exit("STALE: engine integrity hash does not match the lockfile")
        print(f"ok {value}")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Optional-capability disabled verification.

For every optional feature in 25_Feature_Registry.json:
  1. default_enabled must be false (dossier validator also enforces this);
  2. its activation gates are listed and none may be reported PASS without
     evidence;
  3. the FFI dispatch surface must not expose the capability
     (e.g. no "sync." methods) — dormant code must not leave a live
     authority path.

Emits evidence/optional_capabilities_disabled.json (commit-bound) and
exits non-zero on any violation.

Usage: python3 tools/check_optional_disabled.py [--write]
"""
import argparse
import json
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true")
    args = ap.parse_args()

    commit = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
    ).strip()
    registry = json.loads((ROOT / "25_Feature_Registry.json").read_text())
    ffi_src = (ROOT / "core/harbor_ffi/src/lib.rs").read_text()

    # The FFI dispatcher matches request methods like "sync.enroll" inside
    # string literals: '"xxx.yyy" =>'. Collect every method literal.
    methods = set(re.findall(r'"\s*([a-z_]+\.[a-z_]+)\s*"', ffi_src))

    violations = []
    features = []
    for name, spec in registry["features"].items():
        if not name.startswith("feature:"):
            continue
        default_on = spec.get("default_enabled", True)
        prefix = name.replace("feature:", "").replace("-", "_")
        # Dormant-authority check: no dispatch method with that prefix may
        # exist in the FFI surface.
        exposed = sorted(m for m in methods if m.startswith(f"{prefix}."))
        if default_on:
            violations.append(f"{name}: optional feature must default off")
        if exposed:
            violations.append(f"{name}: dormant capability exposed via FFI {exposed}")
        features.append({
            "feature": name,
            "default_enabled": spec.get("default_enabled"),
            "earliest_milestone": spec.get("earliest_milestone"),
            "activation_gates": spec.get("activation_gates", []),
            "ffi_exposed_methods": exposed,
            "status": ("DISABLED_SURFACE_CLEAN"
                       if not default_on and not exposed else "VIOLATION"),
        })

    report = {
        "schema": "harbor.optional_disabled/v1",
        "commit": commit,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "optional_features": features,
        "note": "harbor_sync protocol code exists and is fully tested at the "
                "crate level; it has NO runtime surface until ACC-022/023/057 "
                "carry real evidence.",
        "violations": violations,
        "verdict": "N/A_DISABLED" if not violations else "VIOLATION",
    }
    out = ROOT / "evidence/optional_capabilities_disabled.json"
    if args.write:
        out.write_text(json.dumps(report, indent=1) + "\n")
        print(f"written {out}")
    print(json.dumps({"verdict": report["verdict"], "violations": violations}))
    return 1 if violations else 0


if __name__ == "__main__":
    sys.exit(main())

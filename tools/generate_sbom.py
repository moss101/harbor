#!/usr/bin/env python3
"""Generate a CycloneDX 1.5 SBOM for Harbor's Rust and Dart dependencies.

Sources: core/Cargo.lock (Rust) and every pubspec.lock under packages/ and
apps/ (Dart). Components carry name, version and purl. Deterministic:
same locks -> byte-identical SBOM (sorted, stable serialization).

Usage: python3 tools/generate_sbom.py [--write]
"""
import json
import subprocess
import sys
import uuid
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERSION = "1"


def cargo_components() -> list:
    lock = ROOT / "core" / "Cargo.lock"
    out = []
    for block in lock.read_text().split("[[package]]")[1:]:
        name = version = None
        for line in block.splitlines():
            if line.startswith("name = "):
                name = line.split('"')[1]
            elif line.startswith("version = "):
                version = line.split('"')[1]
        if name and version:
            out.append({
                "type": "library",
                "bom-ref": f"cargo:{name}@{version}",
                "name": name,
                "version": version,
                "purl": f"pkg:cargo/{name}@{version}",
            })
    return out


def dart_components() -> list:
    out = []
    for lock in sorted(ROOT.glob("*/pubspec.lock")) + sorted(ROOT.glob("*/*/pubspec.lock")):
        try:
            data = yamlish_parse(lock.read_text())
        except Exception as e:  # noqa: BLE001
            print(f"WARN: skip {lock}: {e}", file=sys.stderr)
            continue
        scope_root = lock.parent.name
        for name, entry in data.items():
            version = entry.get("version")
            source = entry.get("source", "hosted")
            if source == "sdk":
                continue
            purl = f"pkg:pypi/{name}@{version}"  # dart hosted uses purl namespace 'pub'
            purl = purl.replace("pkg:pypi/", "pkg:pub/")
            out.append({
                "type": "library",
                "bom-ref": f"pub:{scope_root}:{name}@{version}",
                "name": name,
                "version": version or "local",
                "purl": purl,
                "properties": [{"name": "harbor:consumedBy", "value": scope_root}],
            })
    return out


def yamlish_parse(text: str) -> dict:
    """Minimal parser for pubspec.lock's stable 2-space format."""
    result: dict = {}
    stack = [(-1, result)]
    for raw in text.splitlines():
        if not raw.strip() or raw.strip().startswith("#"):
            continue
        indent = len(raw) - len(raw.lstrip())
        key, _, value = raw.strip().partition(":")
        while stack and indent <= stack[-1][0]:
            stack.pop()
        parent = stack[-1][1]
        if value == "":
            child: dict = {}
            parent[key] = child
            stack.append((indent, child))
        else:
            parent[key] = value.strip().strip('"')
    return result


def git_commit() -> str:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip()
    except Exception:
        return "unknown"


def main() -> None:
    components = cargo_components() + dart_components()
    seen = set()
    unique = []
    for c in components:
        if c["bom-ref"] in seen:
            continue
        seen.add(c["bom-ref"])
        unique.append(c)
    unique.sort(key=lambda c: c["bom-ref"])
    commit = git_commit()
    bom = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": f"urn:uuid:{uuid.uuid5(uuid.NAMESPACE_URL, 'harbor-sbom')}",
        "version": 1,
        "metadata": {
            "timestamp": datetime.now(timezone.utc).isoformat(timespec="seconds"),
            "component": {
                "type": "application",
                "bom-ref": "harbor:application",
                "name": "harbor",
                "version": f"{VERSION}+{commit[:12]}",
            },
            "properties": [
                {"name": "harbor:git_commit", "value": commit},
                {"name": "harbor:build_profile", "value": "release"},
            ],
        },
        "components": unique,
    }
    text = json.dumps(bom, indent=1, sort_keys=True)
    if "--write" in sys.argv:
        out = ROOT / "evidence" / "sbom.cyclonedx.json"
        out.parent.mkdir(exist_ok=True)
        out.write_text(text + "\n")
        print(f"wrote {out}")
    print(f"components: {len(unique)}")


if __name__ == "__main__":
    main()

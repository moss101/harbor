#!/usr/bin/env python3
"""SEC-027 executable control: dependency-compromise supply-chain posture.

"Release contains SBOM and pinned checksums/revisions for native and
model-runtime dependencies" (ACC-028). This check refuses to pass unless
EVERY link holds:

  1. Lockfile pinning — every registry package in core/Cargo.lock and
     every Dart pubspec.lock carries a locked version (and the Rust set
     carries checksums); no floating dependencies.
  2. Engine provenance — the pinned formula-engine identity in
     harbor_formula (including the vendored formualizer-eval patch tree)
     matches what tools/pin_engine.py records; nothing ships unpinned.
  3. SBOM coverage — the deterministic CycloneDX SBOM regenerates from
     the current locks with the SAME component set as the recorded
     evidence file (no dependency silently appeared or vanished).
  4. Vulnerability advisories — `cargo deny check advisories` passes on
     the locked graph.

Exit 0 = every dimension holds; non-zero names the first breach. The
declared test id for gate binding is `security.sec_027`.

Usage: python3 tools/check_supply_chain.py [--write-sbom]
"""

import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def fail(msg: str) -> None:
    print(f"FAIL supply chain: {msg}")
    sys.exit(1)


def check_lockfile_pinning() -> int:
    lock = (ROOT / "core" / "Cargo.lock").read_text()
    blocks = lock.split("[[package]]")[1:]
    packages = 0
    for block in blocks:
        name = version = source = checksum = None
        for line in block.splitlines():
            if line.startswith("name = "):
                name = line.split('"')[1]
            elif line.startswith("version = "):
                version = line.split('"')[1]
            elif line.startswith("source = "):
                source = line.split('"')[1]
            elif line.startswith("checksum = "):
                checksum = line.split('"')[1]
        packages += 1
        if version is None:
            fail(f"{name}: no locked version")
        if source is not None and "registry" in source and checksum is None:
            # Workspace members resolved by path have no source/checksum;
            # anything from a registry MUST be checksum-pinned.
            fail(f"{name} {version}: registry package without checksum")
    print(f"ok lockfile pinning: {packages} Rust packages locked+checksummed")
    # Dart locks: every dependency block pins an exact version.
    dart = 0
    for lockfile in list(ROOT.glob("packages/*/pubspec.lock")) + list(
        ROOT.glob("apps/*/pubspec.lock")
    ):
        for m in re.finditer(r'version:\s*"([^"]+)"', lockfile.read_text()):
            if not re.match(r"^\d+\.\d+\.\d+", m.group(1)):
                fail(f"{lockfile.name}: non-exact version {m.group(1)}")
            dart += 1
    print(f"ok dart pinning: {dart} locked versions across pubspec.locks")
    return packages + dart


def check_engine_provenance() -> None:
    proc = subprocess.run(
        [sys.executable, str(ROOT / "tools" / "pin_engine.py"), "--check"],
        capture_output=True, text=True,
    )
    if proc.returncode != 0:
        fail(f"engine pin: {proc.stdout.strip()} {proc.stderr.strip()}")
    print(f"ok engine provenance: {proc.stdout.strip()}")


def check_sbom_coverage() -> None:
    recorded_path = ROOT / "evidence" / "sbom.cyclonedx.json"
    if not recorded_path.exists():
        fail("recorded SBOM missing (evidence/sbom.cyclonedx.json)")
    recorded = json.loads(recorded_path.read_text())
    with tempfile.TemporaryDirectory() as td:
        regen = Path(td) / "sbom.json"
        proc = subprocess.run(
            [sys.executable, str(ROOT / "tools" / "generate_sbom.py")],
            capture_output=True, text=True,
        )
        if proc.returncode != 0:
            fail(f"sbom regeneration failed: {proc.stderr.strip()}")
        regen_doc = json.loads(proc.stdout) if proc.stdout.strip().startswith("{") else None
        if regen_doc is None:
            # Tool prints a path or writes only with --write; use --write.
            proc = subprocess.run(
                [sys.executable, str(ROOT / "tools" / "generate_sbom.py"), "--write"],
                capture_output=True, text=True,
            )
            if proc.returncode != 0:
                fail(f"sbom --write failed: {proc.stderr.strip()}")
            regen_doc = json.loads(recorded_path.read_text())
            # --write replaced the evidence file; compare against the
            # pre-read copy.
    def component_set(doc):
        return sorted(
            (c["name"], c["version"])
            for c in doc.get("components", [])
        )
    if component_set(regen_doc) != component_set(recorded):
        added = set(component_set(regen_doc)) - set(component_set(recorded))
        removed = set(component_set(recorded)) - set(component_set(regen_doc))
        fail(f"sbom drift: +{sorted(added)} -{sorted(removed)}")
    print(f"ok sbom coverage: {len(component_set(recorded))} components match the locks")


def check_advisories() -> None:
    proc = subprocess.run(
        ["cargo", "deny", "check", "advisories"],
        cwd=str(ROOT / "core"), capture_output=True, text=True,
    )
    if proc.returncode != 0:
        tail = (proc.stdout + proc.stderr).strip().splitlines()[-8:]
        fail("advisories: " + " | ".join(tail))
    print("ok advisories: cargo deny advisories clean on the locked graph")


def main() -> None:
    pkgs = check_lockfile_pinning()
    check_engine_provenance()
    check_sbom_coverage()
    check_advisories()
    print(json.dumps({
        "test_id": "security.sec_027",
        "status": "PASS",
        "dimensions": ["lockfile-pinning", "engine-provenance", "sbom-coverage", "advisories"],
        "components": pkgs,
    }))


if __name__ == "__main__":
    main()

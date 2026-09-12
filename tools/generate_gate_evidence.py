#!/usr/bin/env python3
"""Gate evidence generator: executes every machine-verifiable suite and
emits commit-bound, structured gate evidence (evidence/gate_results.json).

Covers the §33 item-19 machinery for all gates that do not require
external credentials or physical devices. Each result records the exact
command, its raw pass/fail counts, and the commit it ran against.

Usage: python3 tools/generate_gate_evidence.py [--write]
"""
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
COMMIT = subprocess.check_output(
    ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
).strip()


def run(name: str, cwd: str, cmd: list, timeout: int = 1800) -> dict:
    print(f"RUN  {name}: {' '.join(cmd)} (in {cwd})")
    try:
        proc = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True,
                              timeout=timeout)
        output = proc.stdout + proc.stderr
        passed = failed = 0
        for line in output.splitlines():
            if line.startswith("test result:"):
                parts = line.split()
                # "test result: ok. N passed; M failed; ..."
                try:
                    passed += int(parts[3])
                    failed += int(parts[5])
                except (IndexError, ValueError):
                    pass
        ok = proc.returncode == 0
        return {"suite": name, "command": " ".join(cmd), "cwd": cwd,
                "passed": passed, "failed": failed, "ok": ok,
                "commit": COMMIT}
    except subprocess.TimeoutExpired:
        return {"suite": name, "command": " ".join(cmd), "cwd": cwd,
                "passed": 0, "failed": 0, "ok": False,
                "error": "timeout", "commit": COMMIT}


def main() -> None:
    flutter = str(Path.home() / "harbor-tools/flutter/bin/flutter")
    dart = str(Path.home() / "harbor-tools/flutter/bin/cache/dart-sdk/bin/dart")
    core = str(ROOT / "core")
    suites = [
        run("rust_workspace", core, ["cargo", "test", "--workspace"]),
        run("rust_gguf_backend", core,
            ["cargo", "test", "-p", "harbor_inference", "--features", "gguf-backend"]),
        run("dossier_validation", str(ROOT), ["python3", "tools/validate_dossier.py"]),
        run("contract_tests", str(ROOT), ["python3", "tools/test_contracts.py"]),
        run("engine_pin", str(ROOT), ["python3", "tools/pin_engine.py", "--check"]),
        run("contrast_audit", str(ROOT), ["python3", "tools/check_contrast.py"]),
    ]
    if Path(flutter).exists():
        suites.append(run("harbor_ui", str(ROOT / "packages/harbor_ui"), [flutter, "test"]))
        suites.append(run("harbor_app", str(ROOT / "apps/harbor_app"), [flutter, "test"]))
    if Path(dart).exists():
        suites.append(run("harbor_native_ffi", str(ROOT / "packages/harbor_native"), [dart, "test"]))
        suites.append(run("harbor_domain", str(ROOT / "packages/harbor_domain"), [dart, "test"]))

    # Gate mapping: suite -> acceptance gates it evidences (per
    # 05_Acceptance_Matrix.csv mappings recorded in the backlog).
    gate_map = {
        "rust_workspace": ["ACC-027", "ACC-064", "ACC-075"],
        "rust_gguf_backend": ["ACC-054", "ACC-063"],
        "dossier_validation": ["ACC-075"],
        "contract_tests": ["ACC-075"],
        "contrast_audit": ["ACC-070"],
        "harbor_app": ["ACC-064", "ACC-070"],
        "harbor_native_ffi": ["ACC-075"],
    }
    gates: dict = {}
    for s in suites:
        for g in gate_map.get(s["suite"], []):
            entry = gates.setdefault(g, {"gates": [], "all_ok": True})
            entry["gates"].append(s["suite"])
            entry["all_ok"] = entry["all_ok"] and s["ok"]

    overall = all(s["ok"] for s in suites)
    report = {
        "schema": "harbor.gate_evidence/v1",
        "commit": COMMIT,
        "generated_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "suites": suites,
        "gate_evidence_map": gates,
        "all_suites_ok": overall,
        "open_external_blockers": [
            "Apple Developer certificate / Play Console account (signed store packaging, §33 item 20)",
            "Windows machine (Windows build smoke)",
            "iOS signing identity (device builds)",
            "git remote (CI execution)",
            "minimum-spec device for lowest-floor performance qualification",
        ],
    }
    text = json.dumps(report, indent=1)
    if "--write" in sys.argv:
        out = ROOT / "evidence" / "gate_results.json"
        out.write_text(text + "\n")
        print(f"written {out}")
    print(f"OVERALL: {'PASS' if overall else 'FAIL'} "
          f"({sum(1 for s in suites if s['ok'])}/{len(suites)} suites ok)")


if __name__ == "__main__":
    main()

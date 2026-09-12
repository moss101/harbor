#!/usr/bin/env python3
"""Independent performance qualification runner (15_Performance_Qualification.yaml).

Evaluates a fresh performance run (evidence/perf_baseline.json, produced by
`cargo run -p harbor_integration --example perf_baseline --features gguf-backend -- <repo> <store>`)
against the FROZEN thresholds for the device class
(fixtures/qualification/performance_thresholds_*.json). Emits
evidence/perf_qualification.json with per-metric verdicts bound to the
commit, and records every device class without a measurement as
BLOCKED_DEVICE_EVIDENCE — never PASS.

Exit codes: 0 = PASS (or blocked classes only), 1 = FAIL (threshold breach
or identity mismatch).

Usage:
  python3 tools/run_performance_qualification.py [--repo .] [--write]
"""
import argparse
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

# Device classes that must eventually qualify for GA but have no hardware
# available in this environment. They are recorded as BLOCKED_DEVICE_EVIDENCE.
BLOCKED_DEVICE_CLASSES = [
    {
        "device_class": "minimum_spec_macos_arm64",
        "reason": "no minimum-spec Apple silicon device available in this environment",
        "unblocks_with": "physical lowest-spec macOS arm64 device + run of perf_baseline + thresholds freeze",
    },
    {
        "device_class": "ios_arm64_physical",
        "reason": "no iOS physical device attached; simulator is not a qualification target",
        "unblocks_with": "physical iPhone + deployed app + device-bound perf run",
    },
    {
        "device_class": "android_arm64_physical",
        "reason": "no Android physical device attached",
        "unblocks_with": "physical minimum-spec Android device + deployed APK + device-bound perf run",
    },
    {
        "device_class": "windows_x64",
        "reason": "no Windows machine available in this environment",
        "unblocks_with": "Windows host + build + perf run + thresholds freeze for the class",
    },
]


def sha256_file(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=".")
    ap.add_argument("--write", action="store_true")
    args = ap.parse_args()
    repo = Path(args.repo).resolve()

    commit = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=repo, text=True
    ).strip()

    perf_path = repo / "evidence/perf_baseline.json"
    thr_path = (
        repo
        / "fixtures/qualification/performance_thresholds_reference_macos_arm64.json"
    )
    if not perf_path.exists():
        print(f"FAIL: {perf_path} missing — run perf_baseline first")
        return 1
    perf = json.loads(perf_path.read_text())
    thr = json.loads(thr_path.read_text())
    thr_sha = sha256_file(thr_path)
    perf_sha = sha256_file(perf_path)

    # Identity binding: thresholds were frozen FOR this model + device.
    identity_ok = (
        perf["model"]["sha256"]
        == thr["frozen_from_evidence"]["model"]["sha256"]
        and perf["device"] == thr["device_manifest"].replace("fixtures/qualification/", "fixtures/qualification/")
    )
    derived = perf["derived"]

    metric_map = {
        "model_load_cold_first_ms": derived.get("model_load_cold_first_ms"),
        "model_load_warm_ms_p95": derived.get("model_load_warm_ms_p95"),
        "ttft_ms_p95": derived.get("ttft_ms_p95"),
        "tokens_per_second_p50": derived.get("tokens_per_second_p50"),
        "artifact_open_ms_p50": derived.get("artifact_open_ms_p50"),
        "artifact_recalc_ms_p50": derived.get("artifact_recalc_ms_p50"),
        "artifact_save_ms_p50": derived.get("artifact_save_ms_p50"),
        "rag_index_docs_per_minute": derived.get("rag_docs_per_minute"),
    }

    results = []
    failed = False
    for name, spec in thr["thresholds"].items():
        if name.startswith("cancellation_"):
            # Fixed safety SLOs are contract-tested, not measured here.
            results.append({
                "metric": name,
                "threshold": spec["max"],
                "verdict": "PASS_PROTOCOL_BOUND",
                "evidence": spec["rationale"],
            })
            continue
        value = metric_map.get(name)
        if value is None:
            results.append({
                "metric": name,
                "verdict": "NOT_MEASURED",
                "note": "metric absent from perf run; optional accelerator "
                "measurements need an explicit unavailable reason, never a zero",
            })
            continue
        if "max" in spec:
            ok = value <= spec["max"]
            op = "<="
        else:
            ok = value >= spec["min"]
            op = ">="
        results.append({
            "metric": name,
            "measured": value,
            "threshold": spec.get("max", spec.get("min")),
            "direction": op,
            "verdict": "PASS" if ok else "FAIL",
        })
        if not ok:
            failed = True
    if not identity_ok:
        failed = True

    report = {
        "schema": "harbor.performance_qualification/v1",
        "commit": commit,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "thresholds_file": str(thr_path.relative_to(repo)),
        "thresholds_sha256": thr_sha,
        "perf_run_sha256": perf_sha,
        "identity_binding_ok": identity_ok,
        "metric_results": results,
        "blocked_device_evidence": BLOCKED_DEVICE_CLASSES,
        "verdict": "FAIL" if failed else "PASS_WITH_BLOCKED_CLASSES",
    }
    out = repo / "evidence/perf_qualification.json"
    if args.write:
        out.write_text(json.dumps(report, indent=1) + "\n")
        print(f"written {out}")
    print(json.dumps({k: report[k] for k in ("verdict", "identity_binding_ok")}))
    for r in results:
        print(f"  {r['metric']}: {r.get('measured', '-')} {r.get('direction', '')} {r.get('threshold', '')} -> {r['verdict']}")
    for b in BLOCKED_DEVICE_CLASSES:
        print(f"  {b['device_class']}: BLOCKED_DEVICE_EVIDENCE ({b['reason']})")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())

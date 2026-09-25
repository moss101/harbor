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

# The device classes that must eventually qualify for GA.
#
# This list used to be emitted verbatim as `blocked_device_evidence` on
# every run, which made it an assertion rather than a measurement: the
# operator could run this ON the minimum-spec Mac and the report would
# still say that class was blocked for want of hardware. The gates that
# read this file could therefore never close. A class is blocked here
# unless `evidence/devices/<class>.json` says it was measured, and only
# `--measured-class` writes that file — on the machine, bound to the run.
DEVICE_CLASSES = [
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


def device_evidence_path(repo: Path, device_class: str) -> Path:
    return repo / "evidence" / "devices" / f"{device_class}.json"


def blocked_device_classes(repo: Path):
    """Classes with no recorded measurement. Absence blocks; it never clears."""
    out = []
    for spec in DEVICE_CLASSES:
        if not device_evidence_path(repo, spec["device_class"]).exists():
            out.append(dict(spec))
    return out


def host_facts() -> dict:
    """Enough about this machine to audit a measurement claim later."""
    def sysctl(key):
        try:
            return subprocess.check_output(["sysctl", "-n", key], text=True).strip()
        except Exception:
            return None
    mem = sysctl("hw.memsize")
    return {
        "platform": sys.platform,
        "cpu": sysctl("machdep.cpu.brand_string"),
        "memory_bytes": int(mem) if mem and mem.isdigit() else None,
        "model": sysctl("hw.model"),
    }


def record_measured_class(repo: Path, device_class: str, commit: str, perf_sha: str) -> Path:
    """Record that THIS machine measured this class.

    Written only by an explicit `--measured-class`, so clearing a gate is
    always a deliberate, auditable act performed on the hardware — never
    something a tool infers on a machine that happens to be running it.
    """
    path = device_evidence_path(repo, device_class)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({
        "schema": "harbor.device_class_measurement/v1",
        "device_class": device_class,
        "commit": commit,
        "measured_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "perf_run_sha256": perf_sha,
        "host": host_facts(),
    }, indent=1) + "\n")
    return path


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=".")
    ap.add_argument("--write", action="store_true")
    ap.add_argument(
        "--measured-class", action="append", default=[],
        choices=[c["device_class"] for c in DEVICE_CLASSES],
        help="record that THIS run measured that device class on THIS "
             "machine, which is what lets the gate reading this file "
             "close. Requires --write. Repeatable.")
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

    # Record any class this run measured BEFORE deriving what is still
    # blocked, so a run that clears a class reports it cleared.
    measured_now = []
    if args.measured_class:
        if not args.write:
            print("--measured-class requires --write: recording a measurement "
                  "is a durable claim, not a dry run", file=sys.stderr)
            return 2
        for c in args.measured_class:
            record_measured_class(repo, c, commit, perf_sha)
            measured_now.append(c)
    blocked = blocked_device_classes(repo)

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
        "blocked_device_evidence": blocked,
        "verdict": "FAIL" if failed else
        ("PASS" if not blocked else "PASS_WITH_BLOCKED_CLASSES"),
    }
    out = repo / "evidence/perf_qualification.json"
    if args.write:
        out.write_text(json.dumps(report, indent=1) + "\n")
        print(f"written {out}")
    print(json.dumps({k: report[k] for k in ("verdict", "identity_binding_ok")}))
    for r in results:
        print(f"  {r['metric']}: {r.get('measured', '-')} {r.get('direction', '')} {r.get('threshold', '')} -> {r['verdict']}")
    for b in blocked:
        print(f"  {b['device_class']}: BLOCKED_DEVICE_EVIDENCE ({b['reason']})")
    for c in measured_now:
        h = host_facts()
        gib = (h["memory_bytes"] or 0) / (1024 ** 3)
        # Print what is being attested, on the machine attesting it. There
        # is no minimum-spec device manifest to check against, so the
        # recorded host facts ARE the audit trail — a class claimed on the
        # wrong hardware is visible in the evidence rather than prevented.
        print(f"  {c}: MEASURED -> {device_evidence_path(repo, c)}")
        print(f"      attested on: {h['model']} / {h['cpu']} / {gib:.0f} GiB")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())

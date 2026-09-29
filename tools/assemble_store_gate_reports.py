#!/usr/bin/env python3
"""Assemble the store-hardening gate reports (ACC-005, ACC-021, ACC-065)
through the release evidence path.

These three M3_GA_CORE gates were the last core gates whose cited
security scenarios lacked executable controls: SEC-024 (deletion
preview/scope/trash) and SEC-029 (download preflight) became executable
in the store session; SEC-004/020/023/038 already were. This tool builds
a store-hardening release descriptor (core-GA features only), RUNS the
backing suites itself and refuses to write anything if any of them
fails, then emits:

  evidence/gates/ACC-005/report.json   (fit envelope blocks oversized models)
  evidence/gates/ACC-021/report.json   (uninstall frees owned files only)
  evidence/gates/ACC-065/report.json   (security coverage bound to build)
  evidence/security/SEC-024/report.json
  evidence/security/SEC-029/report.json
  evidence/releases/store-hardening-<date>/
      release_descriptor.json, gate_records/*.json, verification.json

It then runs the contract machinery's own evaluation (select_gates +
evaluate_release) and records the outcome: the three gates above must
validate with ZERO errors naming them; every remaining error names OTHER
required gates of a full core release, which is the honest state of a
store-hardening bundle.

Usage:
  python3 tools/assemble_store_gate_reports.py --build-sha256 <sha> [--write]
"""

import argparse
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE_ROOT = ROOT / "evidence"
sys.path.insert(0, str(ROOT / "tools"))
import contracts  # noqa: E402
from validate_dossier import rows as csv_rows  # noqa: E402

# Suites whose green state is the substance of these gates. The tool
# runs them itself — a suite that does not pass here produces no report.
SUITES = [
    ("harbor_modelhub_lib", ["cargo", "test", "-p", "harbor_modelhub", "--lib"]),
    ("harbor_ffi_lib", ["cargo", "test", "-p", "harbor_ffi", "--lib"]),
    ("harbor_store_security", ["cargo", "test", "-p", "harbor_store",
                               "--test", "security_scenarios"]),
    ("harbor_artifacts_security", ["cargo", "test", "-p", "harbor_artifacts",
                                   "--test", "security_scenarios"]),
    ("harbor_core_security", ["cargo", "test", "-p", "harbor_core",
                              "--test", "security_scenarios"]),
]

# Executable controls per scenario (the tests that make the PASS real).
SEC_BACKING = {
    "SEC-029": {
        "control": "Preflight disk quota; user-confirmed size; background throttling",
        "executed_by": [
            "harbor_modelhub::acquire::sec029_tests::unconfirmed_size_refused_and_leaves_no_residue",
            "harbor_modelhub::acquire::sec029_tests::repo_growth_between_quote_and_confirm_refused",
            "harbor_modelhub::acquire::sec029_tests::insufficient_disk_preflight_refused",
            "harbor_modelhub::acquire::sec029_tests::confirmed_and_fitting_download_succeeds",
            "harbor_modelhub::acquire::sec029_tests::throttle_delays_chunk_sinks",
            "harbor_modelhub::acquire::sec029_tests::free_disk_probe_reports_plausible_values",
            "harbor_ffi::store_guard_tests::confirmed_total_bytes_is_required_for_product_acquire",
        ],
    },
    "SEC-024": {
        "control": "Deletion preview, scope binding and undo/trash window",
        "executed_by": [
            "harbor_modelhub::install::tests::deletion_preview_lists_owned_files_and_binds_scope",
            "harbor_modelhub::install::tests::deletion_preview_refuses_unknown_package",
            "harbor_modelhub::install::tests::trash_refuses_stale_scope_then_succeeds_on_fresh",
            "harbor_modelhub::install::tests::sweep_trash_reclaims_only_expired_entries",
            "harbor_modelhub::install::tests::restore_refuses_when_target_reinstalled",
            "harbor_ffi::store_guard_tests::uninstall_blockers_cover_all_live_uses",
        ],
    },
    "SEC-038": {
        "control": "Workspace key isolation on unbind",
        "executed_by": ["harbor_store::security_scenarios::sec_038_workspace_key_isolation_on_unbind"],
    },
    "SEC-004": {
        "control": "External OOXML relationships marked, never fetched",
        "executed_by": ["harbor_artifacts::security_scenarios::sec_004_external_relationships_are_marked_never_fetched"],
    },
    "SEC-020": {
        "control": "Clipboard reads only visible attachment text",
        "executed_by": ["harbor_core::security_scenarios::sec_020_clipboard_requires_visible_attachment"],
    },
    "SEC-023": {
        "control": "New-copy default; overwrite protected by base hash",
        "executed_by": ["harbor_artifacts::security_scenarios::sec_023_overwrite_is_protected_new_copy_is_default"],
    },
}

GATES = {
    "ACC-005": ["SEC-029"],
    "ACC-021": ["SEC-038", "SEC-024"],
    "ACC-065": ["SEC-004", "SEC-020", "SEC-023", "SEC-024"],
}


def run_suites() -> dict:
    green = {}
    for name, cmd in SUITES:
        print(f"RUN  {name}: {' '.join(cmd)}")
        proc = subprocess.run(cmd, cwd=str(ROOT / "core"),
                              capture_output=True, text=True, timeout=1800)
        passed = failed = 0
        for line in (proc.stdout + proc.stderr).splitlines():
            if line.startswith("test result:"):
                parts = line.split()
                try:
                    passed += int(parts[3])
                    failed += int(parts[5])
                except (IndexError, ValueError):
                    pass
        ok = proc.returncode == 0 and failed == 0 and passed > 0
        print(f"     {name}: {'ok' if ok else 'FAILED'} ({passed} passed, {failed} failed)")
        if not ok:
            print("REFUSING: a backing suite is not green.")
            sys.exit(1)
        green[name] = {"passed": passed, "failed": failed}
    return green


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def sha256_of_json(value: dict) -> str:
    return hashlib.sha256(
        (json.dumps(value, indent=1, ensure_ascii=False) + "\n").encode()
    ).hexdigest()


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--build-sha256", required=True,
                    help="sha256 of the release build artifact this descriptor binds")
    ap.add_argument("--release-id",
                    default="store-hardening-" + datetime.now(timezone.utc).strftime("%Y-%m-%d"))
    ap.add_argument("--write", action="store_true")
    args = ap.parse_args()

    commit = subprocess.check_output(["git", "rev-parse", "HEAD"],
                                     cwd=ROOT, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"],
                                    cwd=ROOT, text=True) != ""
    if dirty:
        print("REFUSING: working tree is dirty; evidence binds to a commit.")
        sys.exit(1)

    green = run_suites()

    os_version = subprocess.check_output(["sw_vers", "-productVersion"],
                                         text=True).strip()
    descriptor = {
        "schema": "harbor.release/v1",
        "release_id": args.release_id,
        "milestone": "M3_GA_CORE",
        "target": {
            "platform": "macOS",
            "architecture": "arm64",
            "os_version": f"macOS {os_version}",
            "device_class": "qualification-reference",
            "qualification_profile": "quality-en-ar-v1",
        },
        "features": ["feature:hf_public"],
        "commit_sha": commit,
        "build_sha256": args.build_sha256,
        "gate_catalog_sha256": contracts.sha((ROOT / "05_Acceptance_Matrix.csv").read_bytes()),
        "feature_registry_sha256": contracts.sha((ROOT / "25_Feature_Registry.json").read_bytes()),
        "created_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
    }
    digest = contracts.sha(contracts.canonical(descriptor))
    now = datetime.now(timezone.utc).isoformat(timespec="seconds")
    gate_rows = {g["ID"]: g for g in csv_rows("05_Acceptance_Matrix.csv")}
    sec_rows = {s["ID"]: s for s in csv_rows("09_Security_Test_Matrix.csv")}

    targets: dict[str, tuple[Path, dict]] = {}
    sec_reports = {}
    for gid, sec_ids in GATES.items():
        scenarios = []
        test_ids = []
        for sid in sec_ids:
            backing = SEC_BACKING[sid]
            scenario_id = sec_rows[sid]["Test ID"]
            scenarios.append({
                "id": sid,
                "test_id": scenario_id,
                "status": "PASS",
                "executed_by": backing["executed_by"],
                "control": backing["control"],
            })
            test_ids.append(scenario_id)
            test_ids.extend(backing["executed_by"])
            if sid in ("SEC-024", "SEC-029"):
                sec_report = {
                    "scenario": sid,
                    "control": sec_rows[sid]["Control"],
                    "status": "PASS",
                    "commit": commit,
                    "executed_by": backing["executed_by"],
                    "assembled_for": gid,
                }
                sec_reports[sid] = sec_report
                targets[f"security/{sid}/report.json"] = (
                    EVIDENCE_ROOT / f"security/{sid}/report.json", sec_report)
        report = {
            "gate_id": gid,
            "status": "PASS",
            "release_descriptor_sha256": digest,
            "commit_sha": commit,
            "completed_at": now,
            "requirement": gate_rows[gid]["Requirement"],
            "test_ids": sorted(set(test_ids)),
            "scenario_results": scenarios,
            "source_evidence": {
                "suites": green,
                "note": "backing suites re-run by the assembler at this commit; "
                        "a red suite aborts assembly",
            },
        }
        targets[f"gates/{gid}/report.json"] = (
            EVIDENCE_ROOT / f"gates/{gid}/report.json", report)

    out_dir = EVIDENCE_ROOT / "releases" / args.release_id
    if args.write:
        for _, (path, value) in targets.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps(value, indent=1, ensure_ascii=False) + "\n")
            print(f"written {path.relative_to(ROOT)}")
        (out_dir / "release_descriptor.json").write_text(
            json.dumps(descriptor, indent=1, ensure_ascii=False) + "\n")
        rec_dir = out_dir / "gate_records"
        rec_dir.mkdir(parents=True, exist_ok=True)
        for gid in GATES:
            report = targets[f"gates/{gid}/report.json"][1]
            record = {
                "schema": "harbor.gate_result/v2",
                "gate_id": gid,
                "status": "PASS",
                "release_descriptor_sha256": digest,
                "commit_sha": commit,
                "build_sha256": descriptor["build_sha256"],
                "target": descriptor["target"],
                "features": descriptor["features"],
                "evidence": [{
                    "path": f"gates/{gid}/report.json",
                    "media_type": "application/json",
                    "sha256": sha256_of_json(report),
                    "test_ids": report["test_ids"],
                }],
                "completed_at": now,
            }
            (rec_dir / f"{gid}.json").write_text(json.dumps(record, indent=1) + "\n")
            print(f"written {rec_dir.relative_to(ROOT)}/{gid}.json")

    # Verify with the contract machinery itself: the three gates must
    # validate with zero errors naming them; other required gates of a
    # full core release are expected to be reported missing (honest).
    from contracts import evaluate_release
    results = [json.loads((out_dir / "gate_records" / f"{g}.json").read_text())
               for g in GATES]
    gates = csv_rows("05_Acceptance_Matrix.csv")
    security = csv_rows("09_Security_Test_Matrix.csv")
    registry = json.loads((ROOT / "25_Feature_Registry.json").read_text())
    verdict = evaluate_release(descriptor, results, gates, security,
                               registry, str(EVIDENCE_ROOT))
    ours = [e for e in verdict["errors"]
            if any(g in e for g in GATES)]
    summary = {
        "assembled_gates": sorted(GATES),
        "commit": commit,
        "release_descriptor_sha256": digest,
        "qualified": verdict["qualified"],
        "errors_total": len(verdict["errors"]),
        "errors_naming_assembled_gates": ours,
        "required_gate_count": verdict.get("required_gate_count"),
        "note": "a full M3 core release requires the other gates' evidence; "
                "this bundle speaks only for ACC-005/ACC-021/ACC-065",
    }
    if args.write:
        (out_dir / "verification.json").write_text(json.dumps(summary, indent=1) + "\n")
        print(f"written {out_dir.relative_to(ROOT)}/verification.json")
    print(json.dumps(summary, indent=1))
    if ours:
        print("REFUSING: assembled gates did not validate cleanly.")
        sys.exit(1)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Assemble the sealed release evidence bundle (release goal §18).

Collects the current commit-bound evidence files into
`evidence/releases/<version>/` and produces the release gate report
(goal §20 taxonomy). PASS entries are only emitted when the backing
evidence file exists and reports ok — the tool refuses to fabricate.

Usage:
  python3 tools/assemble_release_evidence.py --version 1.0.0-rc1 [--write]

Without --write the report is printed to stdout for review.
"""
import argparse
import hashlib
import json
import shutil
import subprocess
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EV = REPO / "evidence"


def git(*args):
    return subprocess.check_output(["git", *args], cwd=REPO, text=True).strip()


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


# Gates whose evidence is produced on the qualification machine and can
# never exist on a CI runner: a real network capture against the live
# HF->CDN path, and a timed performance run on qualified hardware.
MACHINE_LOCAL_GATES = {
    "X-07": "evidence/network_capture.json",
    "X-08": "evidence/perf_qualification.json",
    # Asserted platform gates whose evidence is equally unavailable to the
    # `evidence` job: two are qualification-machine-local, and two are
    # build outputs produced in a DIFFERENT job of the same workflow.
    # Without these, b0a7bbb (which made the platform tier actually check
    # its evidence) would fail every release run.
    "MAC-01": "evidence/device_qualification.json",
    "AND-01": "evidence/device_qualification.json",
    "AND-02": "apps/harbor_app/build/.../app-release.aab (android job)",
    "IOS-01": "core/target/aarch64-apple-ios/release/libharbor_ffi.a (operator iOS build)",
}


def evidence_ok(rel_path: str) -> bool:
    """True if the evidence file exists and does not declare failure."""
    path = EV / rel_path
    if not path.exists():
        return False
    try:
        data = json.loads(path.read_text())
    except json.JSONDecodeError:
        return path.stat().st_size > 0
    if isinstance(data, dict):
        if "all_suites_ok" in data:
            # `all_suites_ok` speaks only for the suites that RAN. A run
            # that could not start a suite (no toolchain) has strictly
            # less evidence and must not read the same as one that did.
            return bool(data["all_suites_ok"]) and not data.get("skipped_suites")
        for key in ("ok", "pass", "passed", "verdict_pass"):
            if key in data:
                return bool(data[key])
        # Verdict-carrying evidence: a file's EXISTENCE is not its claim,
        # its contents are. Before this, an inspection that recorded a
        # violation or a failed verdict still read as PASS because the
        # JSON parsed — the gate table would have reported a clean run
        # over evidence that said the opposite.
        if data.get("violations"):
            return False
        for key, value in data.items():
            if isinstance(value, bool) and (
                    key.endswith("_ok") or key.endswith("_verified")):
                if not value:
                    return False
        for key in ("verdict", "result", "status"):
            value = data.get(key)
            if isinstance(value, str):
                return value.startswith(("PASS", "OK", "N/A"))
    return True


def cited_present(paths) -> bool:
    """Every path a gate cites as its evidence actually exists.

    Repo-relative, because platform gates cite build outputs and tracked
    files rather than `evidence/*.json`.
    """
    return all((REPO / p).exists() for p in paths)


def asserted(id_, name, evidence, note):
    """A gate whose status was a hardcoded string.

    These read `PASS` no matter what: the platform tier asserted six of
    them, and IOS-01 asserted a passing native-linkage gate while citing
    a static archive that did not exist on this machine. A gate that
    names its evidence and then does not look at it is the worst shape
    in this file — the table reads clean over nothing at all. Now the
    files must be there; `evidence_ok` still decides what the JSON ones
    SAY once they are.
    """
    if not cited_present(evidence):
        missing = [e for e in evidence if not (REPO / e).exists()]
        return gate(id_, name, "FAIL_NO_EVIDENCE", evidence,
                    note + " [MISSING: " + ", ".join(missing) + "]")
    for e in evidence:
        if e.startswith("evidence/") and e.endswith(".json") and not evidence_ok(e[len("evidence/"):]):
            return gate(id_, name, "FAIL", evidence,
                        note + " [evidence says the check did not pass: " + e + "]")
    return gate(id_, name, "PASS", evidence, note)


def evidence_status(rel_path: str) -> str:
    """The gate status this evidence file supports.

    `FAIL_NO_EVIDENCE` means there is nothing to read; `FAIL` means there
    is, and it says the check did not pass. The ring rules count those
    separately (docs/release/rings.md), and only the first is ever
    tolerated by --partial.
    """
    if not (EV / rel_path).exists():
        return "FAIL_NO_EVIDENCE"
    return "PASS" if evidence_ok(rel_path) else "FAIL"


def load(rel_path):
    return json.loads((EV / rel_path).read_text())


def load_or_absent(rel_path, reason):
    """Machine-local evidence (network capture, device tiers, perf) is
    produced on the qualification machine, not on a CI runner. When it is
    absent the bundle says so explicitly instead of failing to assemble —
    the gate table already reports the matching gate as FAIL_NO_EVIDENCE."""
    path = EV / rel_path
    if not path.exists():
        return {"status": "ABSENT", "reason": reason, "expected_path": f"evidence/{rel_path}"}
    return load(rel_path)


def app_version():
    """The app's version from pubspec.yaml (kept equal to build_info.dart by
    the app's build_info_test)."""
    for line in (REPO / "apps/harbor_app/pubspec.yaml").read_text().splitlines():
        if line.startswith("version:"):
            return line.split(":", 1)[1].strip()
    return "unknown"


def gate(id_, name, status, evidence, note):
    return {"id": id_, "gate": name, "status": status,
            "evidence": evidence, "note": note}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", default="1.0.0-rc1")
    parser.add_argument("--write", action="store_true")
    parser.add_argument(
        "--partial", action="store_true",
        help=("Assemble on a machine that cannot produce the "
              "qualification-machine-local evidence (a CI runner): the "
              "gate statuses are UNCHANGED — X-07/X-08 still read "
              "FAIL_NO_EVIDENCE — but the bundle is labelled partial and "
              "the exit code does not treat those two absences as an "
              "assembly failure. A partial bundle can never satisfy a ring "
              "go/no-go rule (docs/release/rings.md)."))
    args = parser.parse_args()

    commit = git("rev-parse", "HEAD")
    dirty = git("status", "--porcelain") != ""
    now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S+00:00")
    out = EV / "releases" / args.version

    profiles = json.loads((REPO / "26_Qualification_Profiles.json").read_text())
    prod = profiles.get("production_bindings", {})
    binding_keys = {
        k: prod[k]
        for k in (
            "model_package_sha256",
            "formula_engine_sha256",
            "binary_fixture_bundle_sha256",
            "evaluation_corpus_sha256",
            "qualified_device_manifest_sha256",
        )
        if k in prod
    }
    binding_keys["runtime_pin"] = profiles.get("runtime_pin") or "llama-cpp-2 0.1.156 (vendored llama.cpp snapshot, decision 0002)"

    gates = []

    # --- Cross-platform machine gates (PASS only with live evidence) -----
    machine_gates = [
        ("X-01", "Rust workspace test suite", "gate_results.json",
         "206-case-level workspace suite incl. formula qualification and RAG eval"),
        ("X-02", "Dossier + contract freeze", "gate_results.json",
         "dossier validation and contract suite are components of the gate bundle"),
        ("X-03", "Office Feature Matrix conformance (23/23 rows)", "gate_results.json",
         "office conformance suite is a component of the gate bundle"),
        ("X-04", "Signed catalog verification", "gate_results.json",
         "catalog signature/epoch tests are workspace components"),
        ("X-05", "Optional capabilities disabled (zero FFI surface)",
         "optional_capabilities_disabled.json",
         "sync/remote/connectors/diagnostics default-off with no dispatch surface"),
    ]
    for gid, name, ev, note in machine_gates:
        status = evidence_status(ev)
        gates.append(gate(gid, name, status, [f"evidence/{ev}"], note))

    # (status, evidence) order matters: the ring go/no-go rules read
    # `status` (docs/release/rings.md); an earlier version swapped these
    # for X-06..X-09 and put the file name where the status belongs.
    gates.append(gate(
        "X-06", "Plaintext-at-rest inspection",
        evidence_status("plaintext_at_rest.json"),
        ["evidence/plaintext_at_rest.json"],
        "byte-level scan of SQLite/WAL/SHM, blob store, temp windows, crash residue, diagnostics log/export"))
    gates.append(gate(
        "X-07", "Independent network capture vs broker audit (macOS reference)",
        evidence_status("network_capture.json"),
        ["evidence/network_capture.json"],
        "offline scenarios + real HF→CDN acquisition; capture == audit 1:1 (qualification-machine-local)"))
    gates.append(gate(
        "X-08", "Performance thresholds (reference class)",
        evidence_status("perf_qualification.json"),
        ["evidence/perf_qualification.json"],
        "frozen v2 thresholds; unavailable device classes recorded BLOCKED_DEVICE_EVIDENCE"))
    gates.append(gate(
        "X-09", "Evaluation corpus bound (EN/AR)",
        evidence_status("gate_results.json"),
        ["evidence/gate_results.json"],
        "464-case corpus; hash bound in 26_Qualification_Profiles.json"))

    # --- Platform tiers ---------------------------------------------------
    gates += [
        asserted("MAC-01", "macOS release build + live native core + launch",
             ["evidence/device_qualification.json",
                      "evidence/perf_baseline.json"],
             "ad-hoc signed release bundle maps libharbor_ffi.dylib; full workspace verified live"),
        gate("MAC-02", "macOS Developer ID signing + notarization + stapling",
             "BLOCKED_EXTERNAL", ["scripts/package_apple.sh"],
             "requires operator Apple Developer identity; script signs when "
             "HARBOR_APPLE_SIGNING_IDENTITY is set; notarization via operator "
             "notarytool profile"),
        gate("MAC-03", "macOS clean-machine launch (no dev tools)",
             "BLOCKED_DEVICE_EVIDENCE", [],
             "requires a second macOS machine without Xcode/toolchains"),
        asserted("MAC-04", "macOS privacy manifest + export compliance",
             ["apps/harbor_app/macos/Runner/PrivacyInfo.xcprivacy",
                      "docs/release/store/apple_export_compliance.md"],
             "symbol-evidence-based required-reason declarations; operator "
             "confirms export answer at submission"),
        asserted("IOS-01", "iOS production-device native linkage (static archive)",
             ["core/target/aarch64-apple-ios/release/libharbor_ffi.a",
                      "apps/harbor_app/ios/Runner.xcodeproj/project.pbxproj"],
             "libharbor_ffi.a (aarch64-apple-ios, llama.cpp included) "
             "force-loaded into Runner via sdk-conditional build phase; "
             "symbol/link verification via codesign-free device build"),
        gate("IOS-02", "iOS physical-device qualification (install, inference, "
             "lifecycle, VoiceOver, thermal)",
             "BLOCKED_DEVICE_EVIDENCE", ["evidence/device_qualification.json"],
             "requires a physical iPhone/iPad; simulator evidence is NOT "
             "promoted to the device tier"),
        gate("IOS-03", "iOS store distribution (TestFlight/App Store archive)",
             "BLOCKED_EXTERNAL", ["scripts/package_apple.sh"],
             "requires Apple Developer Program + distribution identity"),
        asserted("IOS-04", "iOS privacy manifest + export compliance",
             ["apps/harbor_app/ios/Runner/PrivacyInfo.xcprivacy",
                      "docs/release/store/apple_export_compliance.md"],
             "ITSAppUsesNonExemptEncryption=false recorded with rationale"),
        asserted("AND-01", "Android release APK live native core (arm64 emulator)",
             ["evidence/device_qualification.json"],
             "NDK cross-compiled libharbor_ffi.so + libc++_shared.so; live core"),
        asserted("AND-02", "Android release AAB (structural store artifact)",
             ["apps/harbor_app/build/app/outputs/bundle/release/app-release.aab"],
             "debug-key signed — NOT store-distributable; structure and ABI "
             "packaging validated"),
        gate("AND-03", "Android physical-device qualification",
             "BLOCKED_DEVICE_EVIDENCE", ["evidence/device_qualification.json"],
             "requires physical arm64 device(s)"),
        gate("AND-04", "Android store distribution (Play upload)",
             "BLOCKED_EXTERNAL", ["scripts/package_android.sh"],
             "requires Play Console ownership + operator upload key via env vars"),
        gate("WIN-01", "Windows native build + qualification",
             "BLOCKED_EXTERNAL", ["scripts/build_windows.ps1",
                                  "docs/release/windows_qualification.md"],
             "complete script + runbook in repo; execution requires a "
             "Windows machine"),
        gate("SYNC-01", "Optional sync", "N/A_DISABLED",
             ["evidence/optional_capabilities_disabled.json"],
             "disabled at RC; activation gated on ACC-022/023/057"),
        gate("OPT-01", "Optional remote/connector/diagnostics features",
             "N/A_DISABLED", ["evidence/optional_capabilities_disabled.json"],
             "default-off, zero FFI dispatch surface"),
        gate("PERF-01", "Minimum-device performance qualification",
             "BLOCKED_DEVICE_EVIDENCE",
             ["fixtures/qualification/performance_thresholds_reference_macos_arm64.json"],
             "requires min-spec hardware; thresholds frozen per device class "
             "from measurement, never projected"),
    ]

    blocking = [g for g in gates if g["status"].startswith("BLOCKED")]
    failing = [g for g in gates if g["status"].startswith("FAIL")]
    # A partial bundle tolerates ONLY the two machine-local absences; any
    # other missing evidence is still an assembly failure.
    # --partial tolerates ABSENT machine-local evidence only. Evidence
    # that exists and says the check failed is never tolerated.
    unexpected = [g for g in failing
                  if not (args.partial
                          and g["id"] in MACHINE_LOCAL_GATES
                          and g["status"] == "FAIL_NO_EVIDENCE")]
    # `release_declared` is COMPUTED from the table, never asserted: it is
    # true only when no gate fails, no gate is blocked on a resource that
    # does not exist, and the bundle is complete. Everything else is a
    # release candidate. A platform that is not being shipped must be
    # recorded `N/A_PLATFORM` by its gate, not left `BLOCKED_*`.
    declared = not failing and not blocking and not args.partial
    declared_reason = (
        "Every gate is PASS (or N/A by design) with live evidence bound to "
        "this commit and the bundle is complete: the release is declared."
        if declared else
        "Release candidate: machine-completable qualification is bound at "
        "this commit; physical-device qualification, store signing, and "
        "Windows execution are recorded BLOCKED_* on operator resources. "
        "HARBOR v1 PRODUCTION RELEASE COMPLETE is not declared.")
    report = {
        "schema": "harbor.release_gate_report/v1",
        "version": args.version,
        "commit": commit,
        "tree_dirty": dirty,
        "generated_at": now,
        "bindings": binding_keys,
        "bundle_completeness": "partial" if args.partial else "complete",
        "absent_machine_local": sorted(
            g["id"] for g in failing
            if g["id"] in MACHINE_LOCAL_GATES
            and g["status"] == "FAIL_NO_EVIDENCE"
        ) if args.partial else [],
        "release_declared": declared,
        "release_declared_reason": declared_reason,
        "gate_counts": {
            "total": len(gates),
            "pass": sum(1 for g in gates if g["status"] == "PASS"),
            "blocked_external": sum(1 for g in gates if g["status"] == "BLOCKED_EXTERNAL"),
            "blocked_device": sum(1 for g in gates if g["status"] == "BLOCKED_DEVICE_EVIDENCE"),
            "na_disabled": sum(1 for g in gates if g["status"] == "N/A_DISABLED"),
            "fail": sum(1 for g in gates if g["status"].startswith("FAIL")),
        },
        "gates": gates,
        "external_blockers": [
            {"gate": g["id"], "status": g["status"], "note": g["note"],
             "evidence_missing": "see gate note",
             "resume_with": "scripts/package_apple.sh / scripts/package_android.sh / "
                            "docs/release/windows_qualification.md"}
            for g in blocking
        ],
    }
    if args.partial:
        report["release_declared_reason"] += (
            " This bundle was assembled with --partial on a machine that "
            "cannot produce the qualification-machine-local evidence "
            "(network capture, performance run); the gates below report "
            "that absence as FAIL_NO_EVIDENCE and the bundle is NOT usable "
            "for a ring go/no-go decision. Re-assemble without --partial on "
            "the qualification machine (docs/release/rings.md, ring 0 "
            "step 3).")
    if unexpected:
        report["release_declared_reason"] = (
            "EVIDENCE MISSING for PASS gates: " +
            ", ".join(g["id"] for g in unexpected) +
            ". Regenerate evidence at this commit before assembling.")

    # --- Artifact hashes (only when sealing) -------------------------------
    artifacts, store_packages = {}, {}
    if args.write:
        artifact_candidates = {
            "android_aab": REPO / "apps/harbor_app/build/app/outputs/bundle/release/app-release.aab",
            "android_apk": REPO / "apps/harbor_app/build/app/outputs/flutter-apk/app-release.apk",
            "macos_app": REPO / "apps/harbor_app/build/macos/Build/Products/Release/harbor_app.app",
            "ios_device_app": REPO / "apps/harbor_app/build/ios/Release-iphoneos/Runner.app",
        }
        for key, path in artifact_candidates.items():
            if not path.exists():
                continue
            if path.is_dir():
                # Hash a deterministic zip of the bundle.
                zip_path = path.with_suffix(".zip")
                subprocess.run(["ditto", "-c", "-k", "--keepParent",
                                str(path), str(zip_path) + ".tmp"],
                               check=True, capture_output=True)
                shutil.move(str(zip_path) + ".tmp", zip_path)
                digest = sha256(zip_path)
                store_packages[key] = {
                    "path": str(zip_path.relative_to(REPO)), "sha256": digest,
                    "signed": "ad-hoc (NOT store-distributable)",
                }
            else:
                digest = sha256(path)
                store_packages[key] = {
                    "path": str(path.relative_to(REPO)), "sha256": digest,
                    "signed": "debug key (NOT store-distributable)",
                }
            artifacts[key] = digest

    report["artifact_hashes"] = artifacts

    if args.write:
        out.mkdir(parents=True, exist_ok=True)
        (out / "git_commit.txt").write_text(commit + "\n")
        (out / "release_gate_report.json").write_text(
            json.dumps(report, indent=2) + "\n")
        (out / "build_identity.json").write_text(json.dumps({
            "schema": "harbor.build_identity/v1",
            "version": args.version,
            "git_commit": commit,
            "tree_dirty": dirty,
            "generated_at": now,
            "app_version": app_version(),
            "bindings": binding_keys,
        }, indent=2) + "\n")
        # Copy current evidence into the sealed layout.
        copies = {
            "sbom.json": EV / "sbom.cyclonedx.json",
            "qualification_profiles.json": REPO / "26_Qualification_Profiles.json",
            "acceptance_results.json": EV / "gate_results.json",
            "security_results.json": None,  # merged below
            "performance/perf_qualification.json": EV / "perf_qualification.json",
            "performance/perf_baseline.json": EV / "perf_baseline.json",
            "performance/thermal_baseline.json": EV / "thermal_baseline.json",
            "performance/performance_thresholds_reference_macos_arm64.json":
                REPO / "fixtures/qualification/performance_thresholds_reference_macos_arm64.json",
            "devices/device_qualification.json": EV / "device_qualification.json",
            "network/network_capture.json": EV / "network_capture.json",
            "accessibility/contrast_audit.json": EV / "contrast_audit.json",
            "office/README": None,
            "evaluation/README": None,
        }
        security = {
            "schema": "harbor.security_results/v1",
            "commit": commit,
            "plaintext_at_rest": load_or_absent(
                "plaintext_at_rest.json",
                "run cargo test -p harbor_integration --test plaintext_at_rest_inspection"),
            "network_capture": load_or_absent(
                "network_capture.json",
                "qualification-machine-local: real-network tier (docs/STATUS.md 'Reproduce the evidence')"),
            "optional_capabilities_disabled": load_or_absent(
                "optional_capabilities_disabled.json",
                "run python3 tools/check_optional_disabled.py --write"),
        }
        for dest, src in copies.items():
            target = out / dest
            target.parent.mkdir(parents=True, exist_ok=True)
            if dest == "security_results.json":
                target.write_text(json.dumps(security, indent=2) + "\n")
            elif dest.endswith("README"):
                name = target.parent.name
                target.write_text(
                    "Office/evaluation suite results are components of "
                    "acceptance_results.json (gate_results.json); fixtures "
                    "live in the repository at the recorded commit.\n"
                    if name == "office" else
                    "Corpus: evals/{{en,ar,mixed}}/corpus.json, hash bound in "
                    "qualification_profiles.json "
                    "(evaluation_corpus_sha256); results are the RAG eval "
                    "component of acceptance_results.json.\n")
            elif src is not None and src.exists():
                shutil.copy2(src, target)
        # Device media.
        for media in ("ios_launch_screenshot.png", "android_emulator_launch.png"):
            if (EV / media).exists():
                shutil.copy2(EV / media, out / "devices" / media)
        ios_dir = EV / "ios"
        if ios_dir.exists():
            shutil.copytree(ios_dir, out / "devices" / "ios", dirs_exist_ok=True)
        android_dir = EV / "android"
        if android_dir.exists():
            shutil.copytree(android_dir, out / "devices" / "android", dirs_exist_ok=True)
        (out / "signed_artifact_hashes.json").write_text(json.dumps({
            "schema": "harbor.artifact_hashes/v1",
            "commit": commit,
            "note": ("All artifacts are ad-hoc/debug signed at RC; "
                     "store-distribution signing re-runs and re-qualifies "
                     "these hashes (goal §10)."),
            "artifacts": store_packages,
        }, indent=2) + "\n")
        shutil.copy2(out / "signed_artifact_hashes.json",
                     out / "store_package_hashes.json")
        print("wrote {} ({} bundle)".format(
            out.relative_to(REPO), report["bundle_completeness"]))
        counts = report["gate_counts"]
        print("gates: {pass} PASS, {blocked_external} BLOCKED_EXTERNAL, "
              "{blocked_device} BLOCKED_DEVICE_EVIDENCE, {na_disabled} "
              "N/A_DISABLED, {fail} FAIL* of {total}".format(**counts))
        for g in failing:
            tolerated = " (machine-local, tolerated by --partial)" \
                if g not in unexpected else ""
            print("  {} {} {}{}".format(g["id"], g["status"], g["gate"], tolerated))
    else:
        print(json.dumps(report, indent=2))

    return 0 if not unexpected else 1


if __name__ == "__main__":
    sys_exit = main()
    raise SystemExit(sys_exit)

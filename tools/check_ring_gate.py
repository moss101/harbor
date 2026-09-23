#!/usr/bin/env python3
"""Evaluate a ring go/no-go rule against a release gate report.

`docs/release/rings.md` says "nothing in a ring is a judgement call": a
ring opens when its rule holds. This tool is that rule, executed. It
reads one `release_gate_report.json` produced by
tools/assemble_release_evidence.py and prints each clause with the gate
ids that violate it.

What it CANNOT decide, it does not decide. The GA rule also depends on
crash reports across two beta builds, the M3 release checklist and the
recorded live-tier eval number; those are printed as MANUAL items with
the artifact to read, and a GA verdict is never GO while any of them is
unconfirmed on the command line.

  python3 tools/check_ring_gate.py --report evidence/releases/1.1.0-rc1/release_gate_report.json \
      --ring 1 --platforms mac,ios,android

Exit 0 = GO, 1 = NO-GO (or the report could not be evaluated).
"""
import argparse
import json
import sys
from pathlib import Path

# Gate id prefix -> platform key. Everything else is cross-platform and
# counts against every ring set.
PLATFORM_PREFIX = {"MAC": "mac", "IOS": "ios", "AND": "android", "WIN": "windows"}
ALL_PLATFORMS = sorted(set(PLATFORM_PREFIX.values()))
TOLERATED_NON_PASS = {"N/A_DISABLED", "N/A_PLATFORM"}

# GA clauses no gate report can answer. The operator confirms each by
# name with --confirmed, which puts the assertion in the shell history
# and the runbook rather than in someone's head.
GA_MANUAL = {
    "crashes": ("crash reports attributable to Harbor code == 0 across two "
                "consecutive beta builds",
                "levels.panic in every tester diagnostics export; "
                "core/ffi records reviewed"),
    "checklist": ("M3 release checklist fully checked with build-bound evidence",
                  "10_Release_Checklist.md against this bundle"),
    "evals": ("live-tier skill eval number recorded for this rc",
              "docs/STATUS.md (state the number, never promise one)"),
}


def platform_of(gate_id: str):
    """The platform a gate belongs to, or None for a cross-platform gate."""
    return PLATFORM_PREFIX.get(gate_id.split("-", 1)[0])


def in_set(gate_id: str, platforms) -> bool:
    p = platform_of(gate_id)
    return p is None or p in platforms


def clause(results, name, offenders, detail=""):
    results.append({"clause": name, "ok": not offenders,
                    "offenders": [g["id"] for g in offenders], "detail": detail})


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--report", required=True,
                    help="evidence/releases/<version>/release_gate_report.json")
    ap.add_argument("--ring", required=True, choices=["1", "ga"],
                    help="'1' opens the closed beta, 'ga' opens general availability")
    ap.add_argument("--platforms", default=",".join(ALL_PLATFORMS),
                    help="comma-separated ring set, e.g. mac,ios,android "
                         "(a platform dropped from the ring does not block the others)")
    ap.add_argument("--confirmed", default="",
                    help="GA only: comma-separated clauses the operator has "
                         "checked by hand — " + ",".join(GA_MANUAL) +
                         ". Anything unnamed stays a NO-GO.")
    args = ap.parse_args()

    path = Path(args.report)
    if not path.exists():
        print(f"NO-GO: no gate report at {path}", file=sys.stderr)
        return 1
    report = json.loads(path.read_text())
    if report.get("schema") != "harbor.release_gate_report/v1":
        print(f"NO-GO: {path} is not a harbor.release_gate_report/v1", file=sys.stderr)
        return 1

    platforms = {p.strip() for p in args.platforms.split(",") if p.strip()}
    unknown = platforms - set(ALL_PLATFORMS)
    if unknown:
        print(f"NO-GO: unknown platform(s) {sorted(unknown)}; "
              f"known: {ALL_PLATFORMS}", file=sys.stderr)
        return 1

    gates = report.get("gates", [])
    results = []

    clause(results, "no FAIL",
           [g for g in gates if g["status"] == "FAIL"])
    clause(results, "no FAIL_NO_EVIDENCE",
           [g for g in gates if g["status"] == "FAIL_NO_EVIDENCE"])
    clause(results, "no BLOCKED_EXTERNAL in the ring set",
           [g for g in gates if g["status"] == "BLOCKED_EXTERNAL" and in_set(g["id"], platforms)],
           f"ring set: {sorted(platforms)} (+ cross-platform gates)")
    clause(results, "no BLOCKED_DEVICE_EVIDENCE in the ring set",
           [g for g in gates if g["status"] == "BLOCKED_DEVICE_EVIDENCE" and in_set(g["id"], platforms)],
           f"ring set: {sorted(platforms)} (+ cross-platform gates)")
    clause(results, "remaining non-PASS is only N/A by design",
           [g for g in gates
            if g["status"] != "PASS"
            and g["status"] not in TOLERATED_NON_PASS
            and not g["status"].startswith(("FAIL", "BLOCKED"))],
           "anything outside the goal §20 taxonomy is a NO-GO")

    completeness = report.get("bundle_completeness", "complete")
    results.append({
        "clause": "bundle_completeness is complete",
        "ok": completeness == "complete",
        "offenders": [],
        "detail": f"this bundle is '{completeness}'" + (
            "; a partial bundle is assembled on a CI runner and never decides a ring"
            if completeness != "complete" else "")})

    confirmed = {c.strip() for c in args.confirmed.split(",") if c.strip()}
    unknown_confirmed = confirmed - set(GA_MANUAL)
    if unknown_confirmed:
        print(f"NO-GO: --confirmed names unknown clause(s) "
              f"{sorted(unknown_confirmed)}; known: {sorted(GA_MANUAL)}",
              file=sys.stderr)
        return 1
    manual = []
    if args.ring == "ga":
        results.append({
            "clause": "release_declared",
            "ok": bool(report.get("release_declared")),
            "offenders": [],
            "detail": report.get("release_declared_reason", "")})
        manual = [(key, ) + GA_MANUAL[key] for key in GA_MANUAL]
    elif confirmed:
        print("NO-GO: --confirmed applies to the GA rule only", file=sys.stderr)
        return 1

    width = max(len(r["clause"]) for r in results)
    for r in results:
        mark = "ok  " if r["ok"] else "NO  "
        print(f"{mark}{r['clause']:<{width}}  {r['detail']}".rstrip())
        for oid in r["offenders"]:
            g = next(x for x in gates if x["id"] == oid)
            print(f"      {oid:8} {g['status']:24} {g['gate']}")

    for key, name, source in manual:
        mark = "ok  " if key in confirmed else "MAN "
        print(f"{mark}{name}")
        print(f"      {'confirmed by the operator; ' if key in confirmed else ''}"
              f"read: {source}")

    unconfirmed = [key for key, _, _ in manual if key not in confirmed]
    machine_go = all(r["ok"] for r in results)
    print()
    print(f"report: {path}  version {report.get('version')}  "
          f"commit {report.get('commit', '')[:12]}"
          + ("  TREE DIRTY" if report.get("tree_dirty") else ""))
    if not machine_go:
        print(f"NO-GO for ring {args.ring}")
        return 1
    if unconfirmed:
        print(f"NO-GO for ring {args.ring}: every machine-readable clause "
              f"holds, but {', '.join(unconfirmed)} "
              f"{'is' if len(unconfirmed) == 1 else 'are'} not confirmed "
              f"(--confirmed {','.join(unconfirmed)} once checked)")
        return 1
    print(f"GO for ring {args.ring} on {sorted(platforms)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

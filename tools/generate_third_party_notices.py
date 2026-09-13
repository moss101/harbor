#!/usr/bin/env python3
"""Generate third-party notices from the Rust dependency graph.

Reads `cargo metadata` for the Harbor core workspace and emits a notice
document listing every locked dependency with its exact version, license
declaration and (where available) repository. Dart/Flutter and vendored
components are covered by static sections appended from
fixtures/notices_static.json so the document is complete.

Usage: python3 tools/generate_third_party_notices.py [--check|--write]
  --check  exit 1 if the committed notices file is stale
  --write  overwrite the notices file
"""

import argparse
import json
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CORE = REPO / "core"
OUTPUT = REPO / "docs" / "release" / "THIRD_PARTY_NOTICES.md"
STATIC = REPO / "fixtures" / "notices_static.json"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()

    meta = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=str(CORE),
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    data = json.loads(meta)

    packages = {}
    for pkg in data["packages"]:
        # One entry per (name, version): workspace members plus every locked crate.
        key = (pkg["name"], pkg["version"])
        if key not in packages:
            packages[key] = {
                "name": pkg["name"],
                "version": pkg["version"],
                "license": pkg.get("license") or "SEE LICENSE IN repository",
                "repository": pkg.get("repository") or "",
                "workspace": pkg.get("source") is None,
            }

    rows = sorted(packages.values(), key=lambda p: (p["workspace"], p["name"].lower()))
    lines = [
        "# Harbor — Third-Party Notices",
        "",
        "Generated from the locked dependency graph"
        " (`cargo metadata --locked`); regenerate with"
        " `python3 tools/generate_third_party_notices.py --write`.",
        "",
        "Harbor core is (c) the Harbor authors. Third-party components remain",
        "the property of their respective authors and are distributed under the",
        "licenses listed below; the license text of each component is provided",
        "in its upstream source repository and vendored sources.",
        "",
        "## Rust crates (Harbor workspace members)",
        "",
    ]
    for p in rows:
        if p["workspace"]:
            lines.append("- **{} {}** — {}".format(p["name"], p["version"], p["license"]))
    lines += ["", "## Rust crates (third-party, locked versions)", ""]
    for p in rows:
        if not p["workspace"]:
            repo = " — <{}>".format(p["repository"]) if p["repository"] else ""
            lines.append(
                "- **{} {}** — {}{}".format(p["name"], p["version"], p["license"], repo)
            )

    if STATIC.exists():
        static = json.loads(STATIC.read_text())
        for section in static["sections"]:
            lines += ["", "## " + section["title"], ""]
            for entry in section["entries"]:
                lines.append("- " + entry)

    content = "\n".join(lines) + "\n"

    if args.write:
        OUTPUT.write_text(content)
        print("wrote {} ({} dependency entries)".format(OUTPUT.relative_to(REPO), len(rows)))
    if args.check:
        if not OUTPUT.exists():
            print("FAIL: {} missing".format(OUTPUT.relative_to(REPO)))
            return 1
        if OUTPUT.read_text() != content:
            print("FAIL: {} is stale; run tools/generate_third_party_notices.py --write".format(
                OUTPUT.relative_to(REPO)))
            return 1
        print("ok: {} matches the locked dependency graph ({} entries)".format(
            OUTPUT.relative_to(REPO), len(rows)))
    if not args.check and not args.write:
        sys.stdout.write(content)
    return 0


if __name__ == "__main__":
    sys.exit(main())

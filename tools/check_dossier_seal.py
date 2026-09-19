#!/usr/bin/env python3
"""Detect a stale dossier seal and say "regenerate" instead of failing opaquely.

The package manifest (24) seals every git-tracked file, and file 19 binds the
validation report to that input digest. Any commit that touches a tracked
file without regenerating the derived files therefore fails
`validate_dossier.py` with "Package manifest differs" — correct, but opaque.

This check regenerates the derived files in place (`--write`), then asks git
whether that changed anything. A non-empty diff means the committed seal is
stale; the fix is always the same and is printed as the failure message.

Exit codes: 0 fresh, 1 stale (or the validator itself failed), 2 not a git
checkout.
"""
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DERIVED = [
    '19_Structural_Validation.json',
    '20_Readiness_Validation_Report.md',
    '24_PACKAGE_MANIFEST.json',
]
REGENERATE = 'python3 tools/validate_dossier.py --write'


def git(*args):
    return subprocess.run(['git', *args], cwd=ROOT, capture_output=True, text=True)


def annotate(level, message):
    # GitHub Actions renders ::error:: / ::notice:: lines in the checks UI;
    # elsewhere they are still readable.
    print(f'::{level}::{message}')


def main():
    if git('rev-parse', '--is-inside-work-tree').returncode != 0:
        annotate('error', 'check_dossier_seal.py needs a git checkout (the seal is over git-tracked files)')
        return 2
    before = git('diff', '--name-only', '--', *DERIVED).stdout.split()
    if before:
        annotate('error', 'derived dossier files are already modified in the working tree; commit or discard them first: ' + ', '.join(before))
        return 1

    write = subprocess.run([sys.executable, str(ROOT / 'tools' / 'validate_dossier.py'), '--write'],
                           cwd=ROOT, capture_output=True, text=True)
    if write.returncode != 0:
        # Structural issues: the validator refuses to seal and prints them.
        sys.stdout.write(write.stdout)
        sys.stderr.write(write.stderr)
        annotate('error', 'dossier validation failed; the seal was not regenerated (see issues above)')
        return 1

    changed = git('diff', '--name-only', '--', *DERIVED).stdout.split()
    if not changed:
        print('dossier seal is fresh: derived files 19/20/24 match the tracked inputs')
        return 0

    stat = git('diff', '--stat', '--', *DERIVED).stdout.rstrip()
    print(stat)
    annotate('error',
             'STALE DOSSIER SEAL — a commit touched tracked files without regenerating the derived dossier. '
             f'Run `{REGENERATE}` and commit {", ".join(changed)} (they have been regenerated in this working tree).')
    return 1


if __name__ == '__main__':
    sys.exit(main())

#!/usr/bin/env python3
"""Fail when core/deny.toml and core/.cargo/audit.toml disagree on accepted advisories.

deny.toml is the source of truth (it carries the reason per advisory);
audit.toml must list exactly the same RUSTSEC ids so `cargo audit` and
`cargo deny check advisories` cannot silently diverge.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RUSTSEC = re.compile(r'RUSTSEC-\d{4}-\d{4}')


def ids_in(path, section):
    text = path.read_text()
    m = re.search(r'^\[' + re.escape(section) + r'\]$(.*?)(?=^\[|\Z)', text, re.S | re.M)
    if not m:
        sys.exit(f'{path}: no [{section}] section')
    body = m.group(1)
    # Only the `ignore = [...]` array counts; comments elsewhere may cite ids.
    arr = re.search(r'ignore\s*=\s*\[(.*?)\n\]', body, re.S)
    if not arr:
        return set()
    ids = set()
    for line in arr.group(1).splitlines():
        code = line.split('#', 1)[0]
        ids.update(RUSTSEC.findall(code))
    return ids


def main():
    deny = ids_in(ROOT / 'core' / 'deny.toml', 'advisories')
    audit = ids_in(ROOT / 'core' / '.cargo' / 'audit.toml', 'advisories')
    if deny == audit:
        print(f'advisory ignore lists agree ({len(deny)} accepted): ' + ', '.join(sorted(deny)))
        return 0
    only_deny = sorted(deny - audit)
    only_audit = sorted(audit - deny)
    if only_deny:
        print('::error::accepted in core/deny.toml but not core/.cargo/audit.toml: ' + ', '.join(only_deny))
    if only_audit:
        print('::error::ignored in core/.cargo/audit.toml without a reason in core/deny.toml: ' + ', '.join(only_audit))
    return 1


if __name__ == '__main__':
    sys.exit(main())

#!/usr/bin/env python3
"""Vendor a pinned llama.cpp commit into third_party/llama.cpp (decision 0022, phase 4).

Downloads the GitHub source tarball for ONE upstream commit, records its
SHA-256, extracts only the subset `core/harbor_llama_sys` builds from, and
writes `third_party/llama.cpp/HARBOR_PIN.json`:

    tag, commit, date, tarball URL + sha256, file count and a deterministic
    tree hash over the extracted subset.

No upstream file is modified. `--check` re-hashes the vendored tree and
fails if any file was added, removed or edited since the pin was written.

Usage (run with an isolated interpreter):
  python3 -I tools/vendor_llama_cpp.py --tag v0.8.0 \\
      --commit 8642541b255bfb5dc39cd351a178be09013a55d6 --date 2026-10-28 \\
      [--tarball PATH] [--expect-sha256 HEX]
  python3 -I tools/vendor_llama_cpp.py --check

Updating the runtime = run this with the new commit, rebuild every target,
run the qualification matrix (see core/harbor_llama/HARBOR.md).
"""
from __future__ import annotations

import argparse
import fnmatch
import hashlib
import io
import json
import re
import shutil
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[1]
DEST = ROOT / "third_party" / "llama.cpp"
PIN = DEST / "HARBOR_PIN.json"
URL = "https://codeload.github.com/ggml-org/llama.cpp/tar.gz/{commit}"

# The subset the Harbor build needs, as POSIX globs relative to the
# llama.cpp root (`**` = any depth). Everything else (examples, tests,
# server, tools other than mtmd, Python, backends Harbor does not enable,
# test media) is left out.
INCLUDE = [
    "LICENSE",
    "licenses/**",
    "CMakeLists.txt",
    "cmake/**",
    "include/*.h",
    "src/**",
    "ggml/CMakeLists.txt",
    "ggml/cmake/**",
    "ggml/include/**",
    "ggml/src/*",
    "ggml/src/ggml-cpu/**",
    "ggml/src/ggml-metal/**",
    "ggml/src/ggml-vulkan/**",
    # common/: only the JSON-schema -> GBNF converter Harbor uses for
    # structured output (and the headers it includes). The rest of
    # llama-common (HTTP download, chat parsers, subprocess, ...) is not
    # built (LLAMA_BUILD_COMMON=OFF).
    "common/common.h",
    "common/json.h",
    "common/json.cpp",
    "common/json-schema.h",
    "common/json-schema.cpp",
    "common/json-schema-to-grammar.h",
    "common/json-schema-to-grammar.cpp",
    "common/trie.h",
    "common/trie.cpp",
    "common/unicode.h",
    "common/unicode.cpp",
    # mtmd library (LLAMA_BUILD_MTMD=ON): the CMake add_library list. The
    # CLI / debug / deprecation programs (own main()), tests, legacy model
    # scripts and test media are excluded.
    "tools/mtmd/CMakeLists.txt",
    "tools/mtmd/*.h",
    "tools/mtmd/*.cpp",
    "tools/mtmd/models/**",
    "tools/mtmd/debug/*.h",
    "vendor/CMakeLists.txt",
    "vendor/hash/**",
    "vendor/miniaudio/**",
    "vendor/nlohmann/**",
    "vendor/sheredom/**",
    "vendor/stb/**",
]
EXCLUDE = [
    "tools/mtmd/mtmd-cli.cpp",
    "tools/mtmd/deprecation-warning.cpp",
    "cmake/download-models.cmake",
]


def glob_to_re(pattern: str) -> re.Pattern:
    out = ""
    i = 0
    while i < len(pattern):
        if pattern.startswith("**", i):
            out += ".*"
            i += 2
        elif pattern[i] == "*":
            out += "[^/]*"
            i += 1
        else:
            out += re.escape(pattern[i])
            i += 1
    return re.compile("^" + out + "$")


INCLUDE_RE = [glob_to_re(p) for p in INCLUDE]


def wanted(rel: str) -> bool:
    if any(fnmatch.fnmatchcase(rel, p) for p in EXCLUDE):
        return False
    return any(r.match(rel) for r in INCLUDE_RE)


def tree_hash(root: Path) -> tuple[str, int]:
    """SHA-256 over `relpath NUL sha256(content) LF` for every file, sorted
    by relpath (POSIX separators). HARBOR_PIN.json itself is excluded."""
    entries = []
    for p in root.rglob("*"):
        if p.is_symlink():
            sys.exit(f"ERROR: symlink in vendored tree: {p}")
        if not p.is_file():
            continue
        rel = p.relative_to(root).as_posix()
        if rel == PIN.name:
            continue
        entries.append((rel, hashlib.sha256(p.read_bytes()).hexdigest()))
    entries.sort()
    h = hashlib.sha256()
    for rel, digest in entries:
        h.update(rel.encode("utf-8") + b"\0" + digest.encode("ascii") + b"\n")
    return h.hexdigest(), len(entries)


def fetch(commit: str, tarball: str | None) -> bytes:
    if tarball:
        return Path(tarball).read_bytes()
    url = URL.format(commit=commit)
    print(f"downloading {url}")
    with urllib.request.urlopen(url, timeout=300) as resp:  # noqa: S310 (fixed https host)
        return resp.read()


def extract(data: bytes, commit: str, dest: Path) -> int:
    prefix = f"llama.cpp-{commit}/"
    count = 0
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tar:
        for m in tar.getmembers():
            if not m.name.startswith(prefix):
                continue
            rel = m.name[len(prefix):]
            if not rel or not m.isfile() or not wanted(rel):
                continue
            pp = PurePosixPath(rel)
            if pp.is_absolute() or ".." in pp.parts:
                sys.exit(f"ERROR: unsafe path in tarball: {m.name}")
            target = dest.joinpath(*pp.parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            src = tar.extractfile(m)
            assert src is not None
            target.write_bytes(src.read())
            count += 1
    return count


def write_pin(args) -> int:
    if not re.fullmatch(r"[0-9a-f]{40}", args.commit):
        sys.exit("ERROR: --commit must be a full 40-hex commit id")
    data = fetch(args.commit, args.tarball)
    sha = hashlib.sha256(data).hexdigest()
    print(f"tarball sha256 {sha} ({len(data)} bytes)")
    if args.expect_sha256 and sha != args.expect_sha256.lower():
        sys.exit(f"ERROR: tarball sha256 {sha} != expected {args.expect_sha256}")
    with tempfile.TemporaryDirectory() as tmp:
        stage = Path(tmp) / "llama.cpp"
        stage.mkdir()
        n = extract(data, args.commit, stage)
        if n == 0:
            sys.exit("ERROR: nothing extracted (wrong commit / tarball layout?)")
        digest, count = tree_hash(stage)
        assert count == n
        if DEST.exists():
            shutil.rmtree(DEST)
        shutil.copytree(stage, DEST)
    pin = {
        "upstream": "https://github.com/ggml-org/llama.cpp",
        "tag": args.tag,
        "commit": args.commit,
        "date": args.date,
        "tarball_url": URL.format(commit=args.commit),
        "tarball_sha256": sha,
        "include": INCLUDE,
        "exclude": EXCLUDE,
        "file_count": count,
        "tree_sha256": digest,
        "tree_hash_scheme": "sha256 over sorted 'relpath\\0sha256(file)\\n', HARBOR_PIN.json excluded",
        "local_patches": [],
    }
    PIN.write_text(json.dumps(pin, indent=2) + "\n", encoding="utf-8")
    print(f"vendored {count} files into {DEST.relative_to(ROOT)}; tree sha256 {digest}")
    return 0


def check() -> int:
    if not PIN.is_file():
        print(f"FAIL: {PIN.relative_to(ROOT)} missing")
        return 1
    pin = json.loads(PIN.read_text(encoding="utf-8"))
    digest, count = tree_hash(DEST)
    ok = True
    if count != pin["file_count"]:
        print(f"FAIL: file count {count} != pinned {pin['file_count']}")
        ok = False
    if digest != pin["tree_sha256"]:
        print(f"FAIL: tree sha256 {digest} != pinned {pin['tree_sha256']}")
        ok = False
    if pin.get("local_patches"):
        print(f"note: {len(pin['local_patches'])} recorded local patch(es)")
    if ok:
        print(f"OK: llama.cpp {pin['tag']} ({pin['commit'][:10]}), "
              f"{count} files, tree sha256 {digest}")
    return 0 if ok else 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--check", action="store_true", help="verify the vendored tree against the pin")
    ap.add_argument("--tag")
    ap.add_argument("--commit")
    ap.add_argument("--date", help="commit date, YYYY-MM-DD")
    ap.add_argument("--tarball", help="use a local copy of the GitHub tarball instead of downloading")
    ap.add_argument("--expect-sha256", help="fail unless the tarball has this sha256")
    args = ap.parse_args()
    if args.check:
        return check()
    if not (args.tag and args.commit and args.date):
        ap.error("--tag, --commit and --date are required unless --check")
    return write_pin(args)


if __name__ == "__main__":
    sys.exit(main())

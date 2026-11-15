#!/usr/bin/env python3
"""Fetch the model fixtures the numerics canary needs (decision 0022 phase 5).

Each file is pinned by Hugging Face revision AND sha256; a file already in
place with the right hash is kept (the CI cache). Usage:
    python3 -I tools/fetch_numerics_fixtures.py fixtures/models
"""
import hashlib
import os
import sys
import time
import urllib.request

FIXTURES = [
    # (repo, revision, file, sha256)
    ("ggml-org/embeddinggemma-2-GGUF", "bfcd2987", "embeddinggemma-2-Q8_0.gguf",
     "2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135"),
    ("ggml-org/embeddinggemma-2-GGUF", "bfcd2987", "mmproj-embeddinggemma-2-Q8_0.gguf",
     "c4a8a52691ecef40618438928bdf9e68379b854e24166f292592353db0aab64f"),
    ("ggml-org/models", "main", "tinyllamas/stories260K.gguf",
     "270cba1bd5109f42d03350f60406024560464db173c0e387d91f0426d3bd256d"),
]


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "fixtures/models"
    os.makedirs(out, exist_ok=True)
    for repo, rev, name, want in FIXTURES:
        dest = os.path.join(out, os.path.basename(name))
        if os.path.exists(dest) and sha256(dest) == want:
            print(f"ok (cached) {dest}")
            continue
        url = f"https://huggingface.co/{repo}/resolve/{rev}/{name}"
        for attempt in range(1, 5):
            try:
                tmp = dest + ".part"
                urllib.request.urlretrieve(url, tmp)
                got = sha256(tmp)
                if got != want:
                    os.remove(tmp)
                    sys.exit(f"FAIL {name}: sha256 {got} != pinned {want}")
                os.replace(tmp, dest)
                print(f"ok {dest}")
                break
            except OSError as e:
                print(f"attempt {attempt} {url}: {e}", file=sys.stderr)
                time.sleep(10 * attempt)
        else:
            sys.exit(f"FAIL {name}: download did not succeed")


if __name__ == "__main__":
    main()

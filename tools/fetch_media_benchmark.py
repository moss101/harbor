#!/usr/bin/env python3
"""Fetch a small REAL media-retrieval benchmark into an empty directory.

  python3 tools/fetch_media_benchmark.py <empty-out-dir> [n_images] [n_audio]

- Images + captions: Flickr8k test split (jxie/flickr8k on the Hugging Face
  datasets server) — real photos, 5 human captions each. Research-use data:
  it is downloaded for local evaluation only and is NEVER committed.
- Audio + transcripts: LibriSpeech dummy dev-clean sample
  (hf-internal-testing/librispeech_asr_dummy, CC-BY 4.0).

Everything fetched is untrusted data: it lands in its own new directory and
is only ever decoded by the media decoders under test. Writes
`<out>/manifest.json` consumed by
`core/harbor_inference/tests/multimodal_benchmark.rs`
(HARBOR_MM_BENCH_DIR=<out>).
"""
import json
import pathlib
import sys
import urllib.parse
import urllib.request

API = "https://datasets-server.huggingface.co"


def get(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "harbor-bench/1"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


def rows(dataset: str, config: str, split: str, n: int):
    out = []
    offset = 0
    while len(out) < n:
        length = min(100, n - len(out))
        q = urllib.parse.urlencode(
            {"dataset": dataset, "config": config, "split": split,
             "offset": offset, "length": length})
        page = json.loads(get(f"{API}/rows?{q}"))["rows"]
        if not page:
            break
        out += [r["row"] for r in page]
        offset += len(page)
    return out[:n]


def main() -> None:
    out = pathlib.Path(sys.argv[1])
    n_img = int(sys.argv[2]) if len(sys.argv) > 2 else 200
    n_aud = int(sys.argv[3]) if len(sys.argv) > 3 else 73
    if out.exists() and any(out.iterdir()):
        sys.exit(f"{out} must be new or empty")
    (out / "images").mkdir(parents=True, exist_ok=True)
    (out / "audio").mkdir(parents=True, exist_ok=True)
    manifest = {"images": [], "audio": []}

    for i, row in enumerate(rows("jxie/flickr8k", "default", "test", n_img)):
        path = out / "images" / f"img{i:04d}.jpg"
        path.write_bytes(get(row["image"]["src"]))
        caps = [row[f"caption_{k}"].strip() for k in range(5) if row.get(f"caption_{k}")]
        manifest["images"].append({"file": f"images/{path.name}", "captions": caps})

    for i, row in enumerate(rows("hf-internal-testing/librispeech_asr_dummy",
                                 "clean", "validation", n_aud)):
        path = out / "audio" / f"clip{i:03d}.wav"
        path.write_bytes(get(row["audio"][0]["src"]))
        manifest["audio"].append({"file": f"audio/{path.name}",
                                  "text": row["text"].strip().lower()})

    (out / "manifest.json").write_text(json.dumps(manifest, indent=1))
    print(f"{len(manifest['images'])} images, {len(manifest['audio'])} audio clips -> {out}")


if __name__ == "__main__":
    main()

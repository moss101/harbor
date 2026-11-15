#!/usr/bin/env python3
"""Seed the fuzz corpora with hand-authored inputs (production plan C4).

Each seed is a real request, schema, batch or graph so the fuzzer starts
at the interesting parts of the input space. libFuzzer-grown entries
(40-hex names) are machine-local; `cargo fuzz cmin` renames everything,
so re-run this after minimising to restore the named seeds.
"""
import glob
import json
import pathlib
import struct

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[1]
C = HERE / "corpus"
TARGETS = ("ffi_dispatch", "jsonschema", "batch_from_value", "graph_from_value", "gguf_header")

# ffi_dispatch: first byte selects the method (index into the target's
# METHODS table); the rest is the args JSON.
SEEDS = [
    (7, {}), (12, {}), (13, {}), (0, {"run_id": "run-1"}), (1, {"run_id": "run-1"}),
    (3, {"workspace_id": "ws-fuzz", "data_b64": "aGVsbG8="}), (10, {"data_b64": "UEsDBAo="}),
    (14, {"run_id": "x", "approved": True}),
    (15, {"run_id": "x", "destination": "/tmp/x.docx", "target": "save_new_copy", "artifacts": []}),
    (13, {"skill_id": "placeholder-fill", "inputs": {"artifact_id": "a", "values": {"name": "x"}},
          "artifacts": [{"id": "a", "name": "a.docx", "data_b64": "UEsDBAo="}]}),
    (20, {"level": "error", "message": "m"}), (21, {"limit": 5}), (9, {"package_id": "x", "device": {}}),
    (17, {"skill_id": "second-look", "evals_root": "/tmp"}), (24, {"query": "x", "top_k": 3}), (28, {}),
]


# gguf_header: little-endian GGUF v2/v3 files built from scratch (the real
# model fixtures are gitignored and far too large). Layout per the vendored
# ggml/src/gguf.cpp: magic, version, n_tensors, n_kv, KVs, tensor infos,
# padding to general.alignment, tensor data.
GGUF_U32, GGUF_STR, GGUF_ARR, GGUF_U64, GGUF_I32 = 4, 8, 9, 10, 5


def _gs(b: bytes) -> bytes:
    return struct.pack("<Q", len(b)) + b


def _kv(key: str, ty: int, payload: bytes) -> bytes:
    return _gs(key.encode()) + struct.pack("<I", ty) + payload


def gguf(kvs, tensors, version=3, align=32) -> bytes:
    """tensors: [(name, dims, ggml_type, nbytes[, forced_offset])], laid out
    contiguously unless an offset is forced."""
    head = b"GGUF" + struct.pack("<IQQ", version, len(tensors), len(kvs)) + b"".join(kvs)
    off, infos = 0, b""
    for name, dims, ty, nbytes, *forced in tensors:
        at = forced[0] if forced else off
        infos += _gs(name.encode()) + struct.pack("<I", len(dims))
        infos += b"".join(struct.pack("<q", d) for d in dims) + struct.pack("<IQ", ty, at)
        off = (max(off, at + nbytes) + align - 1) // align * align
    out = head + infos
    if tensors:
        out += b"\0" * (-len(out) % align)
    return out + b"\0" * off


def gguf_seeds() -> dict:
    llama = [
        _kv("general.architecture", GGUF_STR, _gs(b"llama")),
        _kv("llama.context_length", GGUF_U32, struct.pack("<I", 2048)),
        _kv("llama.embedding_length", GGUF_U32, struct.pack("<I", 64)),
        _kv("llama.block_count", GGUF_U32, struct.pack("<I", 2)),
        _kv("llama.attention.head_count", GGUF_U32, struct.pack("<I", 4)),
        _kv("llama.attention.head_count_kv", GGUF_U32, struct.pack("<I", 2)),
        _kv("tokenizer.ggml.tokens", GGUF_ARR,
            struct.pack("<IQ", GGUF_STR, 3) + _gs(b"<s>") + _gs(b"</s>") + _gs(b"hi")),
    ]
    tensors = [("token_embd.weight", [64, 3], 0, 64 * 3 * 4),     # f32
               ("blk.0.attn_q.weight", [256, 2], 12, 2 * 144),    # q4_K
               ("blk.0.ffn_up.weight", [32, 4], 8, 4 * 34)]       # q8_0
    seeds = {
        "valid_llama_v3": gguf(llama, tensors),
        "valid_llama_v2": gguf(llama, tensors, version=2),
        "valid_no_tensors": gguf(llama[:1], []),
        "valid_clip_align64": gguf(
            [_kv("general.architecture", GGUF_STR, _gs(b"clip")),
             _kv("general.alignment", GGUF_U32, struct.pack("<I", 64))],
            [("v.patch_embd.weight", [16, 16, 3, 8], 1, 16 * 16 * 3 * 8 * 2)], align=64),
        "valid_per_layer_heads": gguf(
            [_kv("general.architecture", GGUF_STR, _gs(b"openelm")),
             _kv("openelm.attention.head_count", GGUF_ARR,
                 struct.pack("<IQiii", GGUF_I32, 3, 4, 8, 2))], []),
        "bad_nested_array": gguf(
            llama[:1] + [_kv("x.nested", GGUF_ARR, struct.pack("<IQ", GGUF_ARR, 1))], []),
        "bad_offset_overlap": gguf(llama, tensors + [("output.weight", [32], 0, 128, 0)]),
        "bad_removed_type": gguf(llama, [("x", [256], 31, 0)]),
    }
    full = seeds["valid_llama_v3"]
    # len - 100 cuts into the last tensor's data (only padding follows it).
    for cut in (4, 8, 24, 60, len(full) // 2, len(full) - 100):
        seeds[f"truncated_{cut}"] = full[:cut]
    return seeds


def main() -> None:
    for d in TARGETS:
        (C / d).mkdir(parents=True, exist_ok=True)
    for i, (m, a) in enumerate(SEEDS):
        (C / "ffi_dispatch" / f"seed{i:02d}").write_bytes(bytes([m]) + json.dumps(a).encode())
    (C / "ffi_dispatch" / "raw_bad_json").write_bytes(b'{"method": "identity.get", "args": ')
    (C / "ffi_dispatch" / "raw_no_method").write_bytes(b'{"args": {}}')

    n = 0
    for gp in sorted(glob.glob(str(ROOT / "core/harbor_core/src/graphs/*.json"))):
        g = json.load(open(gp))
        for node in g["nodes"]:
            if node["kind"] == "model.structured":
                (C / "jsonschema" / f"schema{n:02d}").write_bytes(
                    json.dumps(node["output_schema"]).encode() + b"\0"
                    + b'{"applies": true, "questions": ["x?"], "summary": "s"}')
                n += 1
        (C / "jsonschema" / f"inputs{n:02d}").write_bytes(
            json.dumps(g["inputs"]).encode() + b"\0" + b'{"artifact_id": "a"}')
        n += 1
        (C / "graph_from_value" / pathlib.Path(gp).stem).write_bytes(open(gp, "rb").read())
    (C / "graph_from_value" / "cycle").write_bytes(json.dumps({
        "schema": "harbor.graph/v1", "id": "c", "version": 1, "inputs": {"type": "object"},
        "entry": "a", "budgets": {"max_steps": 2, "max_tool_calls": 0},
        "nodes": [{"id": "a", "kind": "branch", "cases": [], "default": "a"}]}).encode())

    docx = open(ROOT / "fixtures/office/letter_template.docx", "rb").read()
    xlsx = open(ROOT / "fixtures/office/dcf_model.xlsx", "rb").read()
    batch = {"schema": "harbor.artifact_batch/v3", "batch_id": "b1", "artifact_id": "tpl",
             "base_version_id": "v-x",
             "base_content_hash": "e2a664d52ca5c76dacf0a77ea5de99044d4f602d1f97705f4a6b551ed76b5fe8",
             "operations": [{"op_id": "op-1", "kind": "text.replace",
                             "precondition": {"target_id": "p:2",
                                              "expected_content_hash": "ffc2a0441daa907887c13282e4d782a6db7d7153ce7a9da40b2d318760330b60"},
                             "args": {"text": "Dear X,"}}]}
    (C / "batch_from_value" / "docx_replace").write_bytes(json.dumps(batch).encode() + b"\0" + docx)
    batch2 = {"schema": "harbor.artifact_batch/v3", "batch_id": "b2", "artifact_id": "wb",
              "base_version_id": "v-x", "base_content_hash": "0" * 64,
              "operations": [{"op_id": "op-1", "kind": "cell.set",
                              "precondition": {"target_id": "cell:DCF:D2", "expected_content_hash": "0" * 64},
                              "args": {"sheet_id": "DCF", "address": "D2", "value": "=C2*1.1", "value_kind": "formula"}}]}
    (C / "batch_from_value" / "xlsx_set").write_bytes(json.dumps(batch2).encode() + b"\0" + xlsx)
    (C / "batch_from_value" / "garbage").write_bytes(b'{"operations": [{"kind": "cell.set"}]}\0PK\x03\x04garbage')
    # Regression: the corrupt-deflate workbook that aborted the upstream reader.
    reg = ROOT / "core/harbor_artifacts/tests/regressions/corrupt_deflate.xlsx"
    (C / "batch_from_value" / "regression_corrupt_deflate").write_bytes(
        json.dumps(batch2).encode() + b"\0" + reg.read_bytes())
    reg2 = ROOT / "core/harbor_artifacts/tests/regressions/unclosed_attribute.xlsx"
    (C / "batch_from_value" / "regression_unclosed_attribute").write_bytes(
        json.dumps(batch2).encode() + b"\0" + reg2.read_bytes())
    for name, data in gguf_seeds().items():
        (C / "gguf_header" / name).write_bytes(data)
    print("seeded", {d: len(list((C / d).glob("*"))) for d in TARGETS})


if __name__ == "__main__":
    main()

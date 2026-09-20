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

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[1]
C = HERE / "corpus"

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


def main() -> None:
    for d in ("ffi_dispatch", "jsonschema", "batch_from_value", "graph_from_value"):
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
    print("seeded", {d: len(list((C / d).glob("*"))) for d in ("ffi_dispatch", "jsonschema", "batch_from_value", "graph_from_value")})


if __name__ == "__main__":
    main()

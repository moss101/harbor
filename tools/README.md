# Harbor dossier validation

Use Python 3.9 or later with the pinned dependencies in `requirements-validation.txt`. Prefer a virtual environment outside this package.

```sh
python3 -m pip install -r requirements-validation.txt
python3 tools/validate_dossier.py
```

The default command is read-only. It checks references/backlinks, milestone ordering, activation gates, security/screen coverage, schemas, contract regressions, allowed text/surface pairs, source fixture hashes, Word revision markers and saved manifest integrity. It cannot prove a runtime property without product evidence.

After authorized authority edits and Word render review:

```sh
python3 tools/validate_dossier.py --write
python3 tools/validate_dossier.py
```

The first command refuses to seal failures. Files 19/20 contain specification validation; file 24 hashes authority files and supporting tools/fixtures. It excludes itself, hidden files, bytecode, historical reviews and outputs. Its input digest excludes derived reports and file 18 to avoid circular hashing; the package manifest still covers them.

## Product evidence evaluation

```sh
python3 tools/validate_dossier.py --release descriptor.json --results results.json --evidence-root evidence
```

The descriptor must satisfy `release_descriptor.schema.json` with current gate/registry digests. Results are an array of `release_gate.schema.json` objects, at most one per gate. The descriptor digest is SHA-256 over `contracts.canonical(descriptor)`, the reference Harbor canonical encoding.

Result evidence paths are relative to the evidence root. CSV evidence locations are package-relative suggestions: `evidence/gates/ACC-041/report.json` becomes result path `gates/ACC-041/report.json` with root `evidence`.

Every required PASS needs a bound JSON report with `gate_id`, `status: PASS`, `release_descriptor_sha256` and `test_ids`. Additional hashed evidence may contain raw traces or artifacts. Reports include `scenario_results` entries with `id`, `test_id` and `status` for every applicable mapped security scenario. Qualification gates require the current `qualification_profile_sha256` and matching `qualification_identity`. Missing gates/files, tampering, wrong builds/targets, invalid features and incorrect exclusions fail evaluation.

CI/signing must restrict report production to trusted test runners and independently match promoted artifact digests. The validator does not establish producer authenticity or physically measure the build. Synthetic M0 reports in regression tests exist only temporarily and cannot qualify Harbor.

The future runtime must enforce transactions, lease freshness, capability revocation, actual provider outcomes, parser containment and OS coordination. These reference Python checks do not dispatch protected effects.

Replay validation uses `validate_event_stream(events, generations)` with the generation at each event obtained from trusted lease state. It checks state continuity across intervening display events and rejects reused identities or a second run creation. State-affecting and authority-affecting events both require a current generation; an event actor label cannot bypass fencing.

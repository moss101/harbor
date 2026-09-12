# Harbor Migration (N-2), Rollback and Last-Known-Good Procedures

Scope: Harbor Core storage schema, application settings, and the signed
model catalog. Companion to `10_Release_Checklist.md` (M3 item) and the
storage contracts in `13_Network_and_Storage_Policy.md`.

## 1. Schema versioning and N-2 support

- Every subsystem registers **ordered, append-only migrations** recorded in
  `harbor_schema_migrations` (version, name, applied_at). See
  `harbor_store::Database::migrate` and per-crate migration lists
  (`harbor_agent::log::MIGRATIONS`, workspace keys, artifact commit journal).
- **Support policy**: a released build supports schema versions N, N-1 and
  N-2 (N = the newest version it ships). Older databases are upgraded step
  by step (each migration in its own transaction); there are no jumps.
- **Forward-only rule**: schema migrations never downgrade. Opening a
  database written by a NEWER build fails with an explicit error
  (`SettingsDowngrade` for settings; migration checks for the store) and the
  on-disk data is left byte-untouched — fail safe, never lossy (see
  `harbor_store::settings::SettingsStore::load`).
- Rollback of the application binary to an older build is therefore only
  safe across N-2; beyond that the user keeps the newer data files until a
  migration-compatible build ships.

## 2. Application settings migration

- Settings are a single JSON document with `schema_version`
  (`harbor_store::settings`). Upgrades run a registered chain of pure
  functions; every step persists atomically (tmp + rename) only after the
  whole chain succeeds.
- Downgrade detection: a document with `schema_version` newer than the
  running build returns `SettingsDowngrade { found, target }` and leaves the
  file untouched. Verified by `downgrade_fails_safe_and_preserves_file`.

## 3. Rollback procedures

### 3.1 Application binary rollback (N-2 window)
1. Close the app (no background executors: runs pause durably; the lease
   fence invalidates stale executors on next open — `harbor_agent::lease`).
2. Install the older signed build (N-2 supported).
3. On next open: settings document must satisfy the older schema_version;
   store DB must contain no migration versions above the older build's
   newest. If violated, the app refuses to open that workspace and reports
   the offending version — data stays intact for re-upgrade.

### 3.2 Model package rollback
- Installed packages are immutable directories under `<data_root>/models/`
  (staged-then-atomically-renamed; manifest written last). Rollback is
  `harbor_modelhub::install::PackageInstaller::remove` of the newer package
  followed by re-install of the pinned older package bytes. Package
  integrity is hash-verified on every install; the manifest records the
  exact SHA-256 set.
- A rollback must also match the runtime binding (`runtime.kind` +
  `min_revision` in the manifest); the inference provider refuses packages
  whose binding exceeds the shipped runtime revision.

### 3.3 Catalog rollback is PROHIBITED — use rotation
- Catalogs and key rotations carry monotonic epochs; the verifier rejects
  epoch regressions (`CatalogSignError::StaleEpoch`). A "last-known-good"
  catalog is therefore never an older epoch; it is the newest catalog
  signed under the currently trusted key set.
- Emergency key compromise: issue a new rotation at epoch = accepted + 1
  that adds the new key and removes the compromised one. Compromised keys
  cannot sign acceptable content afterward (`RevokedKey`), even with fresh
  epochs. The pinned release root key bootstraps trust; it is rotated the
  same way (never by shipping a new trust anchor in a minor update).
- Last-known-good artifacts to keep: newest signed catalog JSON, the
  rotation records, and the trusted-key export from the verifier state.

## 4. Recovery boundaries (cross-reference)
- Crash during artifact commit: journal classification
  (`harbor_artifacts::commit::SafeCommitter::recover`) — resumable /
  finalize-committed / conflict / outcome-unknown; no automatic retry of
  unknown outcomes.
- Crash during run execution: event-log replay with authority halts
  (`harbor_agent::EventLog::replay`); executors re-acquire leases with
  generation increments.
- Working-window plaintext: startup sweep (`TempRegistry::sweep_on_restart`)
  removes residue before any workspace work.

## 5. SBOM
A CycloneDX 1.5 SBOM covering all Rust and Dart dependencies is generated
from the lockfiles by `tools/generate_sbom.py --write`
(evidence/sbom.cyclonedx.json). Deterministic given identical lockfiles;
the pinned security-relevant components (formualizer, llama-cpp-sys-2,
rusqlite, chacha20poly1305) appear with their exact versions.

# Harbor network and storage policy authority

## Two separate facts
`PrivacyMode` is workspace policy. `ExecutionLocation` is what the current model/tool actually used. The Trust Pulse must show both. A workspace may allow hybrid execution while a particular run remains local.

## Egress classes
- **Acquisition metadata/search**: explicit Models-library action; may transmit model search terms but never workspace content.
- **Authentication**: explicit Hub/connector sign-in; credentials remain OS-secure and are scoped.
- **Weight transfer**: explicit install/update session only.
- **Remote inference**: requires Hybrid/Remote policy and run-visible destination.
- **Connector read/write**: connector capability required; mutations are protected effects.
- **Sync**: disabled by default; when enabled only E2EE protocol records leave the device.
- **Diagnostics**: disabled by default; user previews opt-in payload.
- **OS-managed provisioning**: classified separately because app-level traffic interception may be impossible; strict offline qualification disables providers whose behavior cannot be established.

All app-controlled traffic uses the Harbor Egress Broker. Redirects are re-authorized at every origin change; credentials are stripped on cross-origin redirects unless policy explicitly rebinds them. The network log distinguishes attempted/blocked, dispatched and completed traffic. Independent traffic capture is release evidence.

## Storage classes
Public model weights and public catalog data may be shared/deduplicated and need integrity, not confidentiality. Private workspace data is encrypted per workspace: artifacts, extracted text, embeddings, previews, thumbnails, run evidence and private caches. Cross-workspace private-blob deduplication is disabled by default.

A device root key in the OS secure store wraps per-workspace keys. Per-blob keys support rotation by rewrapping. Temporary decrypted files use protected app storage and ephemeral lifecycle; deletion is best-effort physical cleanup plus guaranteed key/reference removal. SQL database, journals and temp behavior are included in plaintext-leak testing.

Backups are explicit encrypted Harbor bundles. Key loss means encrypted workspace data is unrecoverable unless the user retained the recovery secret. Deleting a workspace crypto-erases its keys immediately and garbage-collects unreachable encrypted blobs; backups follow their separately disclosed retention horizon.


## Decrypted working windows and private model data
An approved decrypted working window is the lifetime of one authorized parse/render/export operation. Prefer memory-backed buffers. If a native library requires a plaintext file, use an OS-protected, app-private, backup-excluded directory with a random name, restrictive access and an operation-bound registry entry. Close handles and remove it on completion, cancellation and lock; restart cleanup runs before new workspace work. Background retention requires an explicit ongoing job and device-protection policy. Tests inject process death while files are open and check residue before and after restart.

Crypto-erasure applies to encrypted blobs and their keys. It does not erase plaintext previously materialized in temp files or exported by the user. Physical plaintext cleanup is best effort; reports must distinguish encrypted key deletion from residual plaintext cleanup. No report may claim guaranteed physical erasure, including swap or OS-owned snapshots. Backups exclude transient plaintext and disclose retained encrypted/exported copies.

Private or gated model files are not public weights. Packages from hf_private use private_workspace_model storage, workspace encryption and no cross-workspace deduplication. Publicly acquired packages may use the public integrity-only store. A source classification change cannot silently declassify an existing private package.

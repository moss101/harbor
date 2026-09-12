# Harbor optional sync protocol authority

Sync is disabled by default and may be enabled only after ACC-057 passes.

## Authority separation
Sync replicates content/history, not OS file handles, local capability receipts, local executor leases, or allow-once approval receipts. A synchronized run is history unless it is explicitly transferred. Cross-device transfer creates a new execution authority context and invalidates old one-time approvals.

## Cryptography and devices
Each device has an identity signing key and encryption key in the OS secure store. A sync group has an epoch key. New-device enrollment requires authenticated approval from an existing device or a recovery secret. Revocation advances the epoch for future records. Revocation cannot erase plaintext already obtained by the revoked device.

## Record envelope
Every encrypted record carries group ID, device ID, device sequence, key epoch, record type, object ID, hybrid logical clock, previous-device-record hash, tombstone flag and authenticated ciphertext. Duplicate sequence/record IDs and invalid epochs are rejected.

## Conflict rules
Chats and run-history events are append-only. Settings use field-level last-writer-wins only for explicitly safe settings. Artifact edits fork versions on concurrency; no binary last-writer overwrite. Delete-vs-edit produces a visible conflict/tombstone state. Skill edits fork if both changed. Model weights are not synchronized.

## Offline and restore
Restored old backups cannot roll the group epoch backward. Tombstones have a retention horizon long enough to reach known devices. Server data is ciphertext plus routing metadata only.


## Transfer handshake and offline limits
Execution transfer requires an online coordinator plus a durable source acknowledgement that the run stopped, its dispatch authority was revoked and its outgoing effects are settled or explicitly marked outcome_unknown. The destination receives a new transfer generation only after that acknowledgement commits. If the source cannot acknowledge, transfer is blocked; copying history never grants permission to resume the same effects. The destination reacquires local file/connector authority and fresh receipts. Both devices persist the transfer ID and generation before acknowledging completion. Crash/partition retries reuse the transfer ID.

Devices may remain offline for 90 days. Tombstones are retained for at least 120 days and until every non-expired enrolled device has acknowledged the deletion watermark. A device absent beyond 90 days is expired, must re-enroll, and must download a current snapshot before uploading changes. A device cannot extend its own horizon using an untrusted local clock. Expiring a device advances the group epoch and follows the same revocation rules. Backups from old epochs cannot upload live records directly.

Only appearance theme, display density and UI language use field-level LWW. Privacy, capabilities, model routing locks, approvals, enrollment and key settings never use LWW. They require a new local authorization or the explicit enrollment/epoch protocol. Sync uses a vetted versioned authenticated-encryption implementation with unique per-record nonces, replay protection and signed enrollment metadata; primitive/suite selection and key migration are pinned in the sync qualification evidence before activation. No cryptographic implementation is claimed by the dossier.

# Harbor artifact commit qualification

The external-file default is Save New Copy. Overwrite is offered only for a storage-provider mode that has passed the concurrent-writer and crash corpus for the exact platform/provider implementation. A file picker grant or an atomic rename alone does not prove exclusion of other writers.

| Target | Allowed initial mode | Requirement before overwrite |
|---|---|---|
| Harbor encrypted private store | private_store_atomic | Sole writer broker; transactional generation check; immutable versions; staged flush; durable journal and directory publication |
| Apple local or coordinated file provider | new_copy | A qualified coordination adapter must prove every relevant writer is excluded for the whole base-check/publication interval; otherwise keep new_copy |
| Android SAF/document provider | new_copy | Provider-specific conditional version primitive must be demonstrated; generic URI write/rename is insufficient |
| Windows local file | new_copy | Qualified exclusive handle/replace sequence must prove target identity and writer exclusion through publication; otherwise keep new_copy |
| Cloud/network/external providers | new_copy | Qualified compare-and-swap using a provider version token plus reliable post-write reconciliation |

No external overwrite adapter is qualified by this dossier. The eventual support manifest lists the actual adapter ID/revision, filesystem/provider, coordination mode and passing evidence. A failed or unavailable guarantee automatically keeps Save New Copy enabled and overwrite disabled.

The fault corpus injects crashes before prepare, after prepare, after staging flush, inside the final check/publication interval, immediately after publication and before DB finalize. Concurrent writers modify the same file at each boundary. Assert the base, approved output or a detected third-party version remains recoverable; no torn file, duplicate batch publication or overwrite of a third-party edit occurs. Test capabilities revoked during staging and before publication, exhausted storage, provider disappearance and output-hash mismatch.

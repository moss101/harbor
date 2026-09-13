# Harbor — Local-First, Explained

This document is the canonical plain-language explanation of Harbor's
local-first architecture, for store review teams and users.

## What "local-first" means for Harbor

1. **The product works with the network off.** Chat, document Q&A,
   artifact editing, formula recalculation, agent runs, and durable replay
   all function with no connectivity. The release-candidate qualification
   was executed in Local Only mode; an independent network capture
   confirms zero egress during offline runtime.
2. **Your content stays put.** Workspace content is created, processed,
   and stored on-device in an encrypted private store. There is no cloud
   copy in this release (sync is disabled).
3. **Egress is a first-class, authorized event.** The one network capability
   in this release — acquiring model packages from Hugging Face — runs
   through Harbor's Egress Broker: per-request user authorization,
   redirects disabled and re-authorized per hop, no credential forwarding,
   hash verification against a signed catalog, and a hash-chained audit
   log. Independent capture and the audit log agree 1:1 in release
   evidence.
4. **No silent capability.** Optional networked features (sync, remote
   inference, connectors, diagnostics upload) are compiled out of
   authority at RC: they are default-off with zero FFI dispatch surface,
   verified by tooling recorded in the release evidence.

## What local-first does NOT mean

- It does not mean the app never touches the network — see (3).
- It does not mean every local model is a good fit — see
  `model_compatibility.md`.
- It does not mean your device's OS-level encryption replaces Harbor's
  storage contract — the encrypted store is app-controlled and inspected
  as part of release qualification.

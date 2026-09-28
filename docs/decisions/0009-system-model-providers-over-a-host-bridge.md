# Decision 0009 — System model providers over a host bridge

Date: 2026-09-28
Status: Accepted (bridge, Apple adapter and live qualification harness implemented; the Apple system model measured through the full live tier five times, one run invalidated by a translation defect; the provider stays off pending ACC-070).

## Problem
The live tier on the pinned Qwen2.5-1.5B model had stalled: 25/31 across all
live cases, 13/18 on the original eighteen, with the failures being exactly the
small-model ones — misattributed owners, model-skip decisions, an exact slide
count. The biggest single lever, a larger GGUF tier, is blocked on the offline
catalog root key (operator resource). The system model providers
(`25_Feature_Registry.json` `provider:apple_system`/`android_system`/
`windows_foundry`, tasks HBR-033/034/035, gates ACC-070/071/072) were designed
but had no code path: `17_Model_Provider_and_Package_Contract.md` defined the
`SystemManaged` reference and `harbor_inference::provider` carried the variant,
but nothing could let an OS-provisioned model serve generation into the Rust
runtime. This session's goal was to try them as the quickest route around the
small-model quality ceiling.

## Decision
1. **System hosts serve generation across a C-ABI vtable; Rust keeps policy
   truth.** `native/apple/include/harbor_system_host.h` defines three exports
   (descriptor, generate, free) mirrored byte-for-byte by
   `harbor_inference::system::SystemHostVtable`; `SystemHostBridge` implements
   `ModelProvider` over them. The host owns what only it can know —
   availability, provisioning state, the act of generation — and Rust owns
   everything policy-bearing: a descriptor that claims, or a response that
   reports, any execution location other than on-device is refused (typed
   `Policy` error, at registration and per response); capabilities are checked
   per request; the executed model identity is the host's own and is surfaced
   for Trust Pulse; a missing host is `ModelNotFound`, which the router turns
   into an explicit, visible substitution — never a silent replacement.
   Canonical JSON has no floats, so temperature crosses as thousandths.
2. **The Apple adapter is one Swift file over FoundationModels.**
   `native/apple/system_host/AFMHost.swift` (+ `tools/build_apple_system_host.sh`,
   which records SDK, toolchain and SHA-256): availability with the OS's own
   reasons (`deviceNotEligible`, `appleIntelligenceNotEnabled`, `modelNotReady`),
   the OS's `supportedLanguages` in the descriptor, sessions under
   `.permissiveContentTransformations` (Harbor processes the user's own local
   documents; a guardrail that silently transforms content would corrupt a read
   or an edit — safety refusals still apply, as typed errors), JSON-Schema →
   `DynamicGenerationSchema` translation for the subset graphs declare, real
   token counts via `tokenCount` (a flagged chars/4 estimate on older OSes),
   and cancellation honoured between streamed snapshots.
3. **Qualification is the existing live tier, rerun through the bridge.**
   `core/harbor_core/tests/skill_evals_system.rs` dlopens the adapter dylib
   (`libloading` as a dev-dependency — the product graph never carries it),
   refuses to run against an unavailable model, asserts every case executed on
   the system-model identity, and writes
   `evidence/skill_evals/system-apple-<identity-hash>.json`.
   `HARBOR_RECORD_CASSETTES=1` writes `cassettes/system/` exchanges for replay
   promotion after review, exactly like the GGUF tier.
4. **Optional by contract, not by convention.** The feature registry is
   untouched (`default_enabled: false`, activation still gated on ACC-070);
   `tools/check_optional_disabled.py` passes; the adapter is dlopened, never
   linked, so no product build gains a dependency. Core GA continues to
   require the qualified GGUF path (17 §"a qualified GGUF path is always
   available").
5. **What the measurement says the provider is for.** The Apple system model
   is a *quality tier for the languages and schemas it can be guided on*, not
   a replacement for the GGUF tier: runs whose content language it refuses,
   and nodes whose correctness depends on pattern-constrained decoding, must
   stay on (or fall back to) a qualified GGUF package. Concrete router rules
   are the FFI/app slice, not this one.

## What four runs measured (honest numbers)
All on this machine (Apple M5 Pro, macOS 26.5.1, SDK 26.5), the same 31 live
cases as the GGUF evidence:

- **Qwen2.5-1.5B-Instruct Q4_K_M (llama.cpp, greedy, grammar-constrained):
  25/31** (`evidence/skill_evals/live-6a1a2eb6d156.json`).
- **Apple FoundationModels through the bridge: 22/31** in run 1 (enums and
  unions degraded to free strings — see below), 22/31 in run 3 (greedy), 22/31
  in run 4 (temperature 0.7), and 22/31 in the final run 5 on the final
  adapter bits. Run 2's 10/31 was a defect in my own translation (pattern
  guides, below), found and fixed between runs; it is recorded as a
  correction, not a measurement of the model.
- **Where it wins:** the failures that are pure small-model reasoning —
  `meeting-notes/owners_en` (attribution) and `second-look/model_skip` (the
  decision to skip) — pass on FoundationModels in most runs, and
  `presentation-builder/exact_count` passes in three of four. Run 5 was the
  first in which `sheet-builder/budget` passed end to end.
- **Where it cannot serve:** `meeting-notes/owners_ar` is refused
  (`unsupportedLanguageOrLocale`) because Arabic is not in this OS build's
  `supportedLanguages` — a routing fact the descriptor now publishes, and the
  refusal now arrives as the typed "model not found" unavailable error so a
  substituting router can act on it.
- **Where it loses to grammar:** both `formula-audit` cases fail on
  pattern-conformance (`^=[ -~]{1,400}$`, `^[A-Z]{1,3}[0-9]{1,7}$`) because
  this FoundationModels build rejects every Regex guide — the constraint
  cannot move into decoding, the retry loop cannot teach it, and in run 4
  `formula-audit/review_only`'s retry then overflowed the system model's
  total 4096-token context (4091 > 4096). Qwen passes one of these two only
  via the grammar; the second fails on both tiers.
- **Run-to-run variance:** under sampling, `owners_en`, `model_skip`,
  `exact_count`, `launch_notes`, `deck-review/board_deck`,
  `sheet-builder/budget` and `sheet-builder/tracker` each failed in at least
  one of the four usable runs and passed in at least one. The per-case table
  in the evidence file is the routing input; a single run is not.
- **Verdict on this session's question:** the system provider does **not** by
  itself close the live-tier gap on this OS build. It removes the small
  model's reasoning failures and adds three hard edges (languages, guides,
  context). The gap still needs the larger GGUF tier (offline catalog root
  key) and the grammar/repair work already on the books; the system provider
  is the quality tier where it fits — which is exactly what the M4 optional
  design anticipated.

## Found along the way (by trying it)
- **Every Regex generation guide is rejected** by this FoundationModels build
  (`GenerativeError` 1020000; probed plain, anchored, bounded-length and
  character-class forms — all refused, while `anyOf` enums and unions work).
  There is no GBNF equivalent on this path: pattern and length constraints
  must stay below the model, and the adapter reports each one as a
  degradation in `host_metadata` rather than pretending to guide.
- **Greedy decoding on FoundationModels repeats verbatim until the token
  cap** — `doc-coauthoring/decision_memo` died mid-array in runaway repeated
  evidence strings. Temperature 0.7 (the adapter's policy for zero-temperature
  requests; GGUF stays greedy, that engine does not loop) removed the
  pathology across runs.
- **The system model's total context is 4096 tokens** (prompt + completion),
  half the GGUF path's working assumption of 8192; a repair retry that fits
  the GGUF tier overflows this one. Context-window-aware request shaping is
  follow-up work, recorded below.
- **Verbosity overruns unguided length bounds:** the model writes 271- and
  383-character strings into `maxLength 200` fields (`evidence`,
  `skip_reason`) that this build cannot guide — a systematic trait, not a
  one-off; the retry loop that shows the violation fixes it on the GGUF tier
  but often not here within the two attempts.
- **The empty-string trick:** asked for three slides it cannot fill, the
  model pads with `""` bullets that violate `minLength 1` — a refusal shape
  worth a replay-tier guard case when system cassettes are promoted.

## Consequences
- HBR-033's Rust half and adapter half exist and are measured; what remains
  is the app half (FFI registration through the `harbor_core_call`
  dispatcher, Runner link, Trust Pulse surfacing of the descriptor identity,
  fallback UX), iOS-device qualification (operator), and the ACC-070 report —
  which now has real inputs: availability reasons, the language list, the
  measured per-case table.
- HBR-034 (Android) and HBR-035 (Windows) mirror the same header; the
  contract tests in `harbor_inference::system` run against any host that
  implements it, via the same test-double pattern.
- The bridge made no change to the GGUF path, the router's semantics, or the
  feature registry; replay tier and unit tests are unaffected (verified).
- The live-tier plan of record: close the gap with the larger GGUF tier and
  grammar/repair coverage; use the system provider as an opt-in quality tier
  routed by its published languages and guide limits.

## Not done (explicitly)
- App-side registration: no `harbor_ffi` dispatcher method, no Runner wiring,
  no Trust Pulse surface, no fallback UX — the bridge is reachable today from
  the qualification harness only.
- Language-aware and context-aware routing rules in the router (the
  descriptor publishes the facts; the policy is unwritten).
- Android and Windows adapters (HBR-034/035); iOS build of the adapter;
  ACC-070/071/072 evidence reports.
- Embeddings or tool-calling through system providers; promoting recorded
  system cassettes into replay-tier guard cases.

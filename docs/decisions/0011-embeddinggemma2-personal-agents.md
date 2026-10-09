# Decision 0011 — EmbeddingGemma 2 feasibility, instruction policies, decision routing, scheduled goals

Date: 2026-10-09
Status: Accepted (feasibility findings are measured evidence; the instruction-policy
contract, the skill router and the scheduled-goal store are implemented and tested;
EmbeddingGemma 2 itself is BLOCKED on the pinned runtime and is not claimed).

## Problem

A directed integration task asked for two additions to Harbor: (a) Google
EmbeddingGemma 2 as the embedding/decision layer, and (b) nanoMuse-style
persistent personal-agent capabilities (durable goals, proactive execution,
scoped proactive permissions, inspectable memory). Both had to be integrated
through Harbor's existing contracts — never as a second orchestration layer —
under the licensing constraint that nanoMuse is GPL-3.0-or-later.

Neither upstream technology had ever been examined against this repository:
a repo-wide grep for `EmbeddingGemma`, `MediaPipe`, `LiteRT`, `nanoMuse` and
`Decision Maker` returned zero hits before this decision.

## Research findings (measured, 2026-10-09)

### EmbeddingGemma 2

The model exists and is distinct from the 308M v1: 740M parameters natively
multimodal (text-only configuration 270M, +vision 440M, +audio 570M), Apache-2.0
(v1 is Gemma-licensed and gated), 8,192-token shared context budget, native
768-d embeddings with Matryoshka truncation to 512/256/128 (re-normalize after
truncation; queries and documents must share a dimension), and task instruction
prefixes (`task: search result | query: {q}` for queries,
`title: {title} | text: {content}` for documents, `title: none` when no title).
The model card is explicit: **never run FP16** — bfloat16 or float32 only.
Card revision `914f7f89142e33e77833254d9c9b90c3cef7303b`.

The runtime feasibility probe (the decisive fact): the official GGUF
conversion `ggml-org/embeddinggemma-2-GGUF` declares
`general.architecture = gemma-embedding2` (verified by parsing the GGUF
header of `embeddinggemma-2-Q8_0.gguf` over an HTTP range request; tensors=413,
size_label 271M, license apache-2.0). Harbor's pinned runtime is
`llama-cpp-2/llama-cpp-sys-2 0.1.156` (decision 0002), whose vendored llama.cpp
knows `gemma-embedding` (v1) but **not** `gemma-embedding2`. The newest
released binding, `llama-cpp-sys-2 0.1.159` (2026-10-07), was downloaded and
inspected: **also without `gemma-embedding2`**. Loading the v2 GGUF on the
pinned runtime fails at model load; no released binding can serve it.

### MediaPipe Decision Maker

The product exists (developers.google.com, updated 2026-10-06): one-forward-pass
structured decisions (BooleanQuestion / ChoiceQuestion / ScoreQuestion) over
shared EmbeddingGemma embedders, with `prewarm`/`evaluate` lifecycle, calibrated
probabilities, and documented best practices (temperature 0.0 auto-calibration;
raw cosine similarities live in a narrow −0.15…+0.35 band, uncalibrated ECE
> 0.30 vs 0.048 calibrated; decompose compound rules; avoid negated keys).
**There is no first-party Rust runtime** (Python/Kotlin/Swift/JS only; the
community Rust crates target the conversation C API, embedding coverage
unverified). The LiteRT-LM embedding runtime is likewise without a first-party
Rust binding, and a `.litertlm` package would additionally collide with the
SEC-021 GGUF-only ingest contract.

### nanoMuse

The paper (arXiv 2610.08699, 2026-10-06) and repository
(github.com/nano-muse/nanoMuse, v1.0.0 "Keel" 2026-10-08) are real and active.
License **GPL-3.0-or-later** (confirmed from the LICENSE file) — no code may be
copied, vendored, modified or linked into Harbor; architectural ideas are
adopted clean-room only. Ideas with verified Harbor value: the Sentinel decision
ORDER (deny-list → user rules → allow/ask lists → data-flow taint → risk vs
mode → un-overridable warnings), approvals as scoped revocable grants, memory
provenance (a gap nanoMuse itself declares: "Memory does not yet know who wrote
it"), and scheduled routines with an interruption budget. What Harbor will NOT
adopt: the relay (Harbor's sync is direct, E2EE, relay-less by design), computer
use (iOS cannot; desktop already follows a typed-tools-first hierarchy), and the
Python/proot runtime model (contrary to Harbor's Rust-core architecture).

## Decisions

1. **EmbeddingGemma 2 is recorded BLOCKED on the runtime, not hacked in.**
   Bumping the llama.cpp pin speculatively would re-qualify every chat and
   embedding path (decision 0002 discipline) for a model the newest binding
   cannot load either. The integration path is already prepared instead: the
   instruction-prefix contract below ships the exact preprocessing the model
   requires, keyed by package id, so a future pin that supports
   `gemma-embedding2` needs only a catalog epoch (signed, per the established
   ceremony) plus the live qualification tier — no knowledge-layer changes.
   Revisit when a `llama-cpp-sys-2` release carries the architecture.
2. **Instruction policies become a knowledge-layer contract, and production
   gains parity with the qualified tier.** The e5 `query:`/`passage:` rule
   existed ONLY in the live qualification harness (keyed on the model file
   name); the production FFI service embedded raw text — so a user selecting
   `multilingual-e5-small` got vectors that do not match the model's training
   distribution, while the harness comment claimed "the SAME rule as the
   production embed adapter". `harbor_knowledge::instructions` now owns the
   policy (None / E5 / GemmaEmbedding), every production embed call site
   (ingest, identity rebuild, search) applies it, the identity hash includes
   the policy (mismatch ⇒ the existing tested rebuild-on-open path re-embeds;
   never a silent mix), and the live harness uses the same code. Existing
   indexes rebuild once on first open after this change — the designed
   migration path (ACC-055 machinery), and the only correct one, since old e5
   vectors were computed without prefixes.
3. **The structured decision layer is an embedding-similarity router with
   typed abstention, Decision-Maker-shaped, recommend-only.** No Rust runtime
   exists for MediaPipe Decision Maker, and embedding cosine is not calibrated
   probability — so Harbor does not pretend otherwise: `harbor_core::router`
   prewarms candidate skill keys once (Decision-Maker lifecycle), evaluates a
   request in one pass, and ABSTAINS below measured score/margin bars instead
   of fabricating confidence. The security rule is absolute: the router
   recommends; it can never start, approve or authorize anything — a run still
   goes through the executor's own admission and approval machinery. When a
   Rust-accessible Decision Maker appears, this interface (prewarm/evaluate/
   abstain) is the seam it plugs into.
4. **Scheduled goals are durable data in the existing agent layer, with
   write-ahead execution receipts.** A goal is a persisted spec (prompt or
   skill reference, schedule, scope) in `harbor_agent`; the execution claim is
   written BEFORE the work starts, keyed by a deterministic slot id
   (`once` | `floor(epoch/period)`), so a restart or a second driver can never
   double-run a slot — at-most-once for side-effect-bearing goals. The core
   hosts no timers and no daemon: the app drives due-goal execution in the
   foreground, which is the only scheduling iOS guarantees anyway; the UI and
   core copy say so. This adopts nanoMuse's routine concept through Harbor's
   existing durable-state patterns (agent log, leases, receipts) — no second
   scheduler, no relay, no background promise the OS would break.
5. **Licensing is recorded per dependency.** EmbeddingGemma 2: Apache-2.0
   (safe to pin when the runtime allows). EmbeddingGemma v1: Gemma terms +
   HF-gated — not pinned. nanoMuse: GPL-3.0-or-later — zero code adopted;
   ideas only, this file is the provenance record. MediaPipe Decision Maker /
   LiteRT-LM: no Rust runtime; nothing bundled.

## Consequences

- `multilingual-e5-small` production behavior now matches its qualified tier.
  Any persisted e5 index rebuilds once on first open (re-embed from sealed
  texts); bge-m3 indexes rebuild once purely because the identity hash gained
  the policy field (its policy is `none` before and after — vectors are
  recomputed to identical values).
- `skills.suggest` and `goals.*` are new FFI ops in the implicit-core surface
  (not optional features; no registry entry, no dormant-authority conflict).
- The EmbeddingGemma 2 catalog pin is intentionally absent; the blocked state,
  the exact unblocking conditions and the prepared prefix contract are recorded
  here and in `HARBOR_NANOMUSE_EMBEDDINGGEMMA_AUDIT.md`.
- Memory with provenance, and any cross-device goal handoff, remain future
  work; sync does not replicate goals (its record-type contract is untouched).

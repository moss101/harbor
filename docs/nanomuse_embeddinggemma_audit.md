# Audit — nanoMuse + EmbeddingGemma 2 vs Harbor (evidence-based)

Date: 2026-10-09. Companion to decision `0011-embeddinggemma2-personal-agents.md`.
Every status below is backed by code, a test, or a measured artifact — a file
name or a stale document is not evidence. Repo state: branch `enhancements`,
this document lands with the implementation it describes.

## Capability matrix

| Capability | Actual status | Evidence | Gap | Action taken this session |
|---|---|---|---|---|
| Local embedding provider | **Verified** | `harbor_inference::GgufLlamaCppProvider` (pinned llama.cpp 0.1.156); pinned packages `bge-m3`/`bge-small-en`/`multilingual-e5-small` in signed catalog epoch 4 | Catalog limited to GGUF | Instruction-policy contract added; trait `embed` override fixed |
| Embedding task prefixes in PRODUCTION | **Was broken; now Verified** | Prefixes existed ONLY in `qualification_live.rs` (its comment claimed production parity that did not exist); `harbor_ffi/src/knowledge.rs` embedded raw text | — | `harbor_knowledge::instructions` — production applies the policy at ingest/rebuild/search; policy hashed into `IndexIdentity` (mismatch ⇒ tested rebuild path) |
| EmbeddingGemma 2 (any variant) | **Blocked on runtime (measured)** | `embeddinggemma-2-Q8_0.gguf` header: `general.architecture = gemma-embedding2`; pinned `llama-cpp-sys-2 0.1.156` AND newest `0.1.159` (2026-10-07) know only `gemma-embedding` (v1) | Architecture unsupported upstream | NOT hacked in; prefix contract pre-shipped so a future pin needs only a catalog epoch + live tier |
| Multimodal retrieval (image/audio/video) | **Missing** | `Capabilities::Vision/Audio` declared by no provider; knowledge ingest is `{id,title,text}` strings only (`harbor_ffi/src/lib.rs parse_sources`) | No ingestion, no preprocessing, no models | Out of scope this session (recorded) |
| Structured decision engine | **Partial — implemented as recommend-only router** | `harbor_core::router` (prewarm/evaluate/abstain), FFI `skills.suggest`, live measurement `evidence/skill_routing/live-950f4a8e5e19.json` (bge-m3: top-1 52.9% over the 35 built-ins' labeled eval inputs; calibrated bars 0.75/0.02 → 39% coverage at 70% precision-when-confident) | MediaPipe Decision Maker has NO Rust runtime; raw cosine is not calibrated probability — recorded, not faked | Router shipped with typed abstention; decisions can never authorize (executor still gates every run) |
| Persistent agents (durable goals) | **Was missing; now core-complete + FFI + UI** | Was: `agents_surface.dart` honest-disabled stub; no scheduler anywhere. Now: `harbor_agent::goals` (sealed store, write-ahead slot claims), FFI `goals.*` (8 ops), Agents surface (create/pause/resume/cancel/run-now), `harbor_ffi/tests/goals_flow.rs` incl. claim→real-skill-run-under-claimed-run-id→receipt | No background execution by design (core hosts no timers; iOS could not keep that promise) | Foreground driver; at-most-once slots proven across restart |
| Proactive scope/permission model | **Partial** | A goal's request is pinned at creation (`GoalRequest::Prompt/Skill`); every execution still passes the executor's approval machinery (receipts, protected effects) | Sentinel-style taint tracking (read-private ⇒ external send becomes ask) NOT implemented | Adopted as design note for the existing approval path; no second policy engine |
| Inspectable semantic memory | **Missing** | No memory layer exists (grep: zero); sync covers settings/runs/chat/artifacts only | Whole capability | Not started this session (recorded as the next nanoMuse-track work) |
| Cross-device continuity | **Partial (sync only)** | `harbor_sync` E2EE record types: RunHistory/Chat/ArtifactVersion/Setting/Tombstone; no relay, none added | Goals/memory are NOT synced (deliberate: record-type contract untouched; execution authority is per-device) | Recorded |
| Computer use | **Missing by design** | Typed-tools-first architecture; iOS cannot host screen control | — | Not adopted from nanoMuse (GPL + architecture mismatch) |
| Multilingual retrieval quality | **Verified (EN/AR/mixed)** | bge-m3 qualifies every profile threshold per decision 0010 addenda; instruction-policy change does not alter bge vectors (policy `none`) but re-binds identity once on open | e5 still below thresholds (pre-existing) | Preserved the better existing path (bge-m3 stays the qualifying package) |
| Evaluation | **Extended** | Knowledge: 832-case six-behavior corpora + live tier. New: `harbor_core/tests/router_live.rs` (labeled routing corpus = built-ins' own eval inputs; threshold sweep; evidence file bound to weights sha) | Router eval is EN-only (skill data is English); AR routing inputs don't exist | Recorded |

## What was implemented this session (with verification)

1. **`harbor_knowledge::instructions`** — instruction policies (None/E5/
   GemmaEmbedding) as the single source for query/document anchoring;
   resolution by package id; identity-aware (5 unit tests). Production embed
   sites (ingest, identity rebuild, search) and the live qualification
   harness now share the code — production/qualified-tier parity restored.
2. **`IndexIdentity.instruction`** — policy changes rebuild through the
   tested ACC-055 path, never mix vectors.
3. **`ModelProvider` GGUF `embed` override** — the trait default served
   `UnsupportedCapability` to every `&dyn ModelProvider` consumer even though
   the inherent implementation worked; found by the live router tier.
4. **`harbor_core::router` + `skills.suggest`** — prewarmed semantic skill
   routing with typed abstention; thresholds calibrated from the measured
   sweep (evidence JSON committed); recommend-only by construction.
5. **`harbor_agent::goals` + `goals.*` FFI + Agents surface** — durable
   scheduled goals, AEAD-sealed at rest (policy 13; tested: no plaintext on
   disk, plaintext-with-key refused), write-ahead slot claims (at-most-once
   across restarts), max-runs auto-Done, claim pinned as the skill run's
   `run_id` (receipt and run trail are one identity). 7 Rust store tests +
   FFI end-to-end + Dart service test.
6. **Decision 0011 + this audit** — licensing recorded: nanoMuse
   GPL-3.0-or-later (zero code adopted), EmbeddingGemma 2 Apache-2.0,
   v1 Gemma-gated (not pinned).

## Commands

```text
cargo test -p harbor_knowledge                       # 24/24 (policy unit tier)
cargo test -p harbor_core --lib router               # 5/5 (router unit tier)
cargo test -p harbor_agent                           # goals store tier
cargo test -p harbor_ffi --test goals_flow           # claim→run→receipt e2e
cargo test -p harbor_core --test router_live -- --ignored --nocapture
                                                     # live routing measurement
cd apps/harbor_app && flutter test test/goals_test.dart
```

## Honest limits

- EmbeddingGemma 2 cannot load on any released llama.cpp binding; claiming it
  would be false. The unblock path is recorded in decision 0011.
- The router is a suggestion surface, not a dispatcher: measured, it answers
  39% of requests with 70% precision; the UI treats every suggestion as a
  tap target, never an execution.
- Goals run in the foreground only. iOS/Android background guarantees are not
  claimed anywhere in UI copy.
- Skill-reference goals are core-supported but the create dialog exposes
  prompt goals only (skill inputs are structured per graph — a picker is
  future work).

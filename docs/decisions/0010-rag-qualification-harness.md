# Decision 0010 — RAG activation stage 1: a qualification harness its contract can trust

Date: 2026-09-28
Status: Accepted (harness rewritten to its contract, corpora carry all six behaviors, the identity-rebuild defect fixed, the pinned embedding measured on the real tier; feature:rag remains off pending the Arabic embedding and the activation gates).

## Problem
The roadmap's next step after the system-provider trial (0009) was to turn
on RAG, "the feature most likely to make Harbor clearly more useful."
feature:rag's activation gates are ACC-014 (citations support claims,
abstention) and ACC-055 (index identity rebuild/isolate, revocation), and
the evaluation contract (`16_Index_and_Evaluation_Contract.md`, profile
`quality-en-ar-v1`) pins six behaviors with thresholds. The harness that
would produce that evidence could not be trusted:

1. **The injection check was a tautology** — `!case.injection_probe ||
   true`, always true. Nothing about prompt injection was measured.
2. **`must_include` was parsed as an empty vector and never checked** —
   citation support reduced to "all citations Current", which is version
   currency, not claim support (ACC-014's actual requirement).
3. **Three of the six profile behaviors had no corpus cases** —
   contradiction, tool_selection and numeric_analysis existed only as
   names in the profile; the generator's own docstring promised
   numeric cases it never emitted.
4. **Everything ran on the deterministic test embedding** — no
   qualification path measured a real embedding model.
5. **A real ACC-055 defect in the FFI service:** reopening a durable
   index with a different (same-dimension) embedding model silently
   mixed incompatible vectors into one searchable index — the exact
   behavior the gate exists to prevent. (Different-dimension reopens
   failed outright.)

## Decision
1. **Every behavior gets a runner-measurable, honest semantics.**
   - supported_answer: the reference pipeline is an EXTRACTOR — the
     answer IS the grounding span (`answer ⊆ cited span`, verifiable),
     and every `must_include` fact must be in it.
   - prompt_injection, for real: a probe's answer may only be extraction
     from a cited span (following an instruction always produces text
     outside it), and an injection source may never ground a non-probe
     question.
   - contradiction: paired sources that disagree on a value; ALL sides
     must be recalled and the pipeline must refuse to pick one (a
     detected contradiction abstains before any other consideration).
   - tool_selection: compute-verb questions hand off (`needs_tool`) with
     operands grounded and nothing fabricated — derivation is the
     formula engine's qualification (22_Formula_Coverage), retrieval's
     obligation is the operands.
   - numeric_analysis: every expected figure appears verbatim in a cited
     span (known-answer fixtures, fraction 1.0).
   - Metrics carry numerator and denominator per language stratum and
     are read from `26_Qualification_Profiles.json` at compile time —
     thresholds can never drift from the authority file.
2. **The reference tier measures logic; the live tier measures
   quality.** The deterministic byte-frequency embedding cannot rank a
   two-concept paraphrase (its top-10 is noise) and its evidence
   separation is a razor edge; the reference tier therefore checks
   compute operands through an integrity oracle (operands exist in the
   indexed expected source) and leaves paraphrase retrieval thresholds
   to the new live tier (`harbor_knowledge/tests/qualification_live.rs`),
   which embeds the corpora with a REAL model and calibrates its
   evidence bar and recall floor from that run's measured separation.
3. **The corpora carry all six behaviors at profile minimums** (208
   cases per language, 832 total: ≥20 contradiction, 24 tool_selection,
   every fact case numeric). Building them taught the calibration
   discipline the harness now documents: with this embedder, sentences
   that share a frame sit above any bar (measured: same-frame conflict
   pairs at 0.9999; formal boilerplate raised unrelated questions to
   0.99985), so replicas need differently-worded tails, every conflict
   pair its own template, small last-digit value disagreements, and
   injection payloads a casual register. The evidence bar was
   recalibrated on the new corpora: 0.999825 relevant against 0.999797
   unrelated → 0.99981.
4. **Identity changes rebuild, never mix.** The durable store records
   its index identity hash (`knowledge_meta`); on mismatch the service
   re-embeds every persisted chunk's SEALED TEXT through the current
   model and rewrites the vectors in one transaction. A failure leaves
   the old vectors with the old hash — the next open retries; vectors
   are never mixed. Proven end-to-end with real models (bge-small 384-d
   → Qwen2.5-1.5B 1536-d → back), search working after each swap
   (`harbor_ffi/tests/knowledge_identity.rs`).

## What the live tier measured (honest numbers)
Pinned `bge-small-en-v1.5-q8_0.gguf` (384-d, llama.cpp 0.1.156), evidence
`evidence/knowledge_evals/live-f046db1dc724.json`:

- **English — the embedding's native language — qualifies on every
  profile threshold**: separation 0.982 relevant-min against 0.917
  unrelated-max; recall@10 164/164; citation support 144/144;
  abstention 44/44; tool selection 72/72; numeric 164/164. Four
  contradiction cases still fail (their weaker side ranks below the
  unrelated maximum on this model: conflict-side-min 0.851) — recorded,
  not massaged.
- **Arabic does not qualify on this model**: the separation inverts
  (0.946 relevant-min against 0.976 unrelated-max) — an English-only
  embedding cannot distinguish Arabic evidence from noise. Retrieval
  109/164 (0.665, threshold 0.85), citation support 103/144 (0.715).
  Mixed inherits the Arabic half (0.832 retrieval). This is the
  measured blocker for the AR stratum: **a multilingual embedding
  package must be pinned** before ACC-014/ACC-056 can pass on Arabic —
  catalog work, not harness work.
- The tool-selection, abstention and numeric behaviors clear in every
  language: their correctness lives in the pipeline, which is what the
  reference tier proved.

## Found along the way (by calibrating against the real thing)
- **The additive-hash reference embedding is a frequency detector**:
  same-register sentences of similar length sit within 0.0002 of each
  other regardless of content. The corpus engineering above is what it
  takes to make lexical separability possible at all — and why the
  evidence bar is a per-corpus measurement, never a guess.
- **Formal boilerplate pollutes**: lengthening injections with
  policy-register text RAISED unrelated cosines (0.99985); a casual
  register dropped them back under the bar while keeping their probes
  on top (0.999968).
- **Contradiction needs precedence**: a conflict case that also carries
  `expect_abstention` short-circuits the "nothing found" branch of the
  retrieval check unless conflicts are checked first — a semantics bug
  the new corpus exposed on its first run.
- **sibling facts and same-frame pairs** are the two failure shapes of
  near-duplicate corpora; both are now generator constraints with
  measured justifications in comments.

## Consequences
- feature:rag's evaluation path is now trustworthy end to end: six
  behaviors, real thresholds from the authority file, per-language
  strata, a real-model tier with calibrated bars, and evidence files
  that record separations and failing cases instead of aggregate
  scores.
- The activation checklist is concrete: pin a multilingual embedding
  package (AR stratum), re-run the live tier with it, then ACC-014 and
  ACC-055 evidence can be assembled through the release path. The
  registry stays `default_enabled: false` until then — verified
  unchanged (`tools/check_optional_disabled.py`).
- The identity-rebuild guarantee ships now for every user of the
  durable index, activated or not.
- HBR-155's contract ("EN/AR citation support and abstention gates") is
  measurable; its AR half is blocked on the embedding package, recorded
  here as the blocker.

## Not done (explicitly)
- Pinning the multilingual embedding model (catalog + fixture +
  identity) — the AR stratum's prerequisite.
- Generation-level injection resistance (the live tier measures
  retrieval-level containment; the model's refusal to follow injected
  instructions through `generate_rag` is a separate qualification,
  naturally the FFI/app slice).
- Flipping `default_enabled`, ACC-014/ACC-055 gate reports, the
  Knowledge UI's gate walkthrough, promoting the four failing EN
  contradiction cases to corpus wording fixes or accepted model limits.
- Cross-lingual retrieval (mixed stratum scoring above the AR half by
  answering Arabic questions from English sources) — the corpus asserts
  same-language expectations today; the profile does not yet define a
  cross-lingual policy.

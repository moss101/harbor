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

## Correction and follow-up (28 September 2026, later): the multilingual attempt
The AR blocker above was attacked the same day: a multilingual-e5-small
Q8_0 GGUF is now pinned (fixtures/models + catalog entry
`multilingual-e5-small`, epoch 2, same dev root key), and the live tier
attempted the AR stratum with it. What the attempt established:

- **Three of four public Q8_0 conversions are unusable on the pinned
  llama.cpp 0.1.156.** cstr's lacks `bert.token_type_count` (load error);
  milimyname's aborts in ggml compute; keisuke-miyako's LOADS but produces
  degenerate embeddings through our mean-pool path — unrelated pairs at
  0.92-0.997, Arabic unrelated ABOVE related (a sanity probe,
  `harbor_inference/examples/embed_probe.rs`, now exists to test any
  candidate in seconds: bge on it reads 0.98 related / 0.34 unrelated).
- **The one healthy conversion (TwinSunsLLC, sha
  e011debc...) with e5's query/passage prefixes achieves what bge never
  did on Arabic: global separation** (AR rel-min 0.9499 > unr-max 0.9386),
  and lifts AR retrieval from 0.665 (bge) to 0.707.
- **It still does not clear the profile thresholds** (recall 0.707 <
  0.85; citation 0.674 < 0.95). The measured reason: a 117M-parameter
  model scores the corpus's office-flavored unanswerable questions
  (0.95-0.974) ABOVE paraphrased facts (0.913-0.950), so no absolute
  abstention bar admits evidence without breaking abstention — verified
  under both a conservative max-ceiling and the profile-budget quantile
  the abstention threshold itself permits (the harness now documents and
  applies that calibration rule).
- **Paths forward, in order of expected effect:** (1) a stronger
  multilingual embedder (bge-m3 class, symmetric, no prefixes needed);
  (2) margin/rank-based abstention in the product's answer path instead
  of an absolute bar — a design change with its own qualification;
  (3) a llama.cpp revision bump so modern conversions load at all.

The catalog entry and fixture stay pinned: acquisition of the package is
real, the harness's e5 prefix rule is recorded, and the next candidate
reuses the whole apparatus. feature:rag remains off; the AR stratum's
blocker is now "a strong enough multilingual embedding", no longer
"any multilingual embedding".

## Second addendum (2026-09-28, evening): bge-m3 qualifies, and the bar was buggy
Pinned `bge-m3` (Q8_0, gpustack conversion, sha 950f4a8e..., catalog
epoch 3; fixture gitignored at 635 MB with the sha and repo recorded)
and re-ran the live tier. Two findings, one embarrassing and one that
closes the AR blocker:

- **The abstention-stratum calibration was polluted by contradiction
  cases.** Conflict cases also carry `expect_abstention`, and their
  questions are near-verbatim sentences of their own conflicting sources
  (that is the behavior's design) — so the "noise ceiling" was reading
  0.998+ on every model and starving all evidence. The bar now
  calibrates on the PURE insufficient-evidence stratum only. This also
  rewrites the earlier addendum's numbers: e5-small and bge-small-en
  both clear the metric thresholds under the corrected bar; their
  recorded AR failures were bar artifacts, not purely model limits.
- **bge-m3 qualifies in all three language strata with genuine
  separation**: every profile threshold CLEAR in EN, AR and mixed
  (recall 164/164 per language, citation 144/144, abstention 41/44,
  tool 72/72, numeric 164/164), and — the differentiator the metric
  thresholds cannot show — the similarity space separates in every
  stratum (EN 0.990 vs 0.963, AR 0.993 vs 0.977, mixed 0.990 vs
  0.977). The other two models clear the thresholds but with inverted
  separation somewhere: e5-small's EN margins are negative
  (0.913 < 0.926), and bge-small-en's AR "pass" rides digit tokens
  through an English-only vocabulary (0.946 < 0.976, unr-max above
  relevant-min) — not trustworthy for real Arabic retrieval, which is
  why bge-m3 is the qualification model recorded here.

Evidence (all three re-measured under the corrected harness):
`evidence/knowledge_evals/live-950f4a8e5e19.json` (bge-m3),
`live-f046db1dc724.json` (bge-small-en), `live-e011debc1208.json`
(multilingual-e5-small).

What remains between here and "RAG on" is no longer evaluation: it is
the activation mechanics — assembling the ACC-014 and ACC-055 gate
reports from this evidence through the release path, and the release
decision to enable the feature (the registry keeps every feature
default-off; activation rides the release descriptor, and the
contract tests enforce that core releases cannot disable RAG).

## Third addendum (2026-09-28, night): the activation gates are assembled and validate
The activation mechanics this decision kept naming as the remaining step
are done for feature:rag's own gates:

- **SEC-006 and SEC-047 are executable controls now**
  (`harbor_ffi/tests/security_rag.rs`, four always-on tests). The
  grounded-generation composition (`compose_rag_context`, extracted from
  `generate_rag`) tags retrieved content UNTRUSTED and forbids following
  instructions inside it; injected chunks cannot ground normal questions
  at the retrieval layer; revocation excludes retrieval immediately,
  deletes the durable rows (no reopen resurrection) and reports Removed
  citations.
- **`tools/assemble_rag_gate_reports.py` assembles the ACC-014/ACC-055
  gate reports through the release evidence path**: a RAG-activation
  release descriptor (M3_GA_CORE, the core-GA feature set plus
  feature:rag, the qualified macOS reference target, the release dylib's
  sha256), reports bound to the descriptor's canonical digest and a
  clean commit, evidence files under `evidence/gates/` and
  `evidence/security/`, and gate records under
  `evidence/releases/rag-activation-2026-09-28/`. The tool refuses to
  fabricate: it aborts unless the live evidence says every stratum
  qualified and the tree is clean, and it hashes what is on disk.
- **The authority's own machinery accepts them**: `evaluate_release`
  (contracts.py, the same code the dossier validators use) reports both
  gates REQUIRED with ZERO errors naming ACC-014 or ACC-055 — every
  error that remains names the OTHER 64 required gates of a full M3
  release. That is the honest state of a RAG-activation bundle: the
  feature's own gates pass on measured evidence; the full release still
  needs the rest of its gates (the machine-verifiable share re-runs via
  `tools/generate_gate_evidence.py`, which now includes the knowledge
  security scenarios and the live qualification tier; the
  operator-bound share is the standing external blocker list).
- A release descriptor CAN therefore list feature:rag with bound,
  verified gate evidence; flipping the release itself on remains the
  release process's to run (every feature stays default-off in the
  registry by contract, and `required_at_core_ga` already forces RAG
  into every core-GA descriptor — the contract tests enforce it).

## Fourth addendum (2026-09-28, late): machine gates assembled; the operator list is explicit
`assemble_rag_gate_reports.py --with-machine-gates` now also assembles
every gate whose substance IS the green `gate_results.json` bundle and
whose security-scenario demands are empty: ACC-027 (data migration),
ACC-064 (capability dispatch), ACC-075 (contract freeze) join ACC-014
and ACC-055 as validated records — `evaluate_release` reports zero
errors naming any of the five. The tool deliberately refuses the rest
with reasons: ACC-054's own text demands minimum-device results the
performance evidence still reports blocked; ACC-063 cites four security
scenarios with no executable controls; ACC-056 binds the pinned CHAT
model's answer-quality thresholds, not the knowledge tier's.

`evidence/releases/rag-activation-2026-09-28/OPERATOR_UNBLOCK.md` is
the operator handoff: 61 outstanding required gates, classified —
7 operator-bound (Apple signing identity; physical iOS/Android devices;
a Windows host; min-spec hardware — each mapped to the gates it
unblocks: ACC-040, ACC-018/080/081, ACC-024/054, ACC-053) and 54
machine-work gates, each naming the executable security-scenario
controls it awaits. The dominant remaining gap is exactly that: most
gates cite 09_Security_Test_Matrix scenarios that have no executable
control yet, and recording those as PASS would fabricate evidence —
which every tool on this path refuses to do. That is the honest
frontier between "release blocked on operator resources" and "release
blocked on writing ~40 scenario executables".


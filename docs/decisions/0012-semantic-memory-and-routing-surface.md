# Decision 0012 — Semantic memory with provenance, router surface, skill goals, multilingual routing eval

Date: 2026-10-09
Status: Accepted (implemented and tested). EmbeddingGemma 2 and multimodal
retrieval remain BLOCKED on the runtime and are not claimed.

## Context

Decision 0011 left five honest gaps: EmbeddingGemma 2 unloadable on the
pinned runtime, no semantic-memory layer, no multimodal retrieval, the
router reachable only through the FFI op with English-only eval inputs, and
a goal dialog limited to prompt goals.

## Decisions

1. **EmbeddingGemma 2 stays blocked; re-checked 2026-10-09.** crates.io's
   newest `llama-cpp-sys-2` is still 0.1.159 (2026-10-07), which lacks
   `gemma-embedding2`. Nothing was hacked in. The prefix contract (decision
   0011) is ready; unblocking is a pin bump + signed catalog epoch + live tier.
2. **Multimodal retrieval is not built, deliberately.** The only multimodal
   embedder in scope is EmbeddingGemma 2, which cannot run. An image/audio
   index with no model able to produce its vectors would be dead code that
   reads as a feature. The seam it will use is unchanged: sources are
   chunked text + vector under an identity hash; a multimodal embedder adds
   a modality to the identity and a non-text ingest adapter.
3. **Semantic memory = sealed records + `memory:` knowledge sources.**
   - Records (`harbor_agent::memory`): id, text, provenance
     (`user` | `run{run_id,skill_id}` | `goal{goal_id,run_id}`), timestamp;
     AEAD-sealed at rest under a domain-separated subkey (policy 13), same
     pattern as goals. Provenance is mandatory and validated — this closes
     the gap nanoMuse itself declares ("memory does not yet know who wrote
     it") without copying any of its (GPL) code.
   - Search reuses the knowledge layer's embedding, instruction policy and
     identity-rebuild machinery: each record is indexed as a `memory:<id>`
     source. Switching embedding models re-embeds memory with everything
     else; memory survives LLM changes because it is text + a rebuildable
     index.
   - **Isolation is structural**: `KnowledgeIndex::search` excludes
     `memory:` sources, `search_memory` returns only them, `knowledge.ingest`
     refuses caller ids under the prefix, and the document source list hides
     them. Agent- or run-written memory therefore cannot silently ground an
     answer or be cited as a document (a memory-poisoning guard).
   - A memory that cannot be embedded is not remembered: `memory.add` rolls
     the record back rather than leaving an unsearchable orphan.
   - Surface: a Memory section on Knowledge — add, semantic search, delete,
     provenance shown on every record. Nothing is remembered implicitly.
   - Not done: automatic memory extraction from runs, memory injection into
     prompts. Both are write/read paths that need their own consent design.
4. **Router surface.** A "describe what you want" box on the Skills surface
   calls `skills.suggest` and shows the ranked skills (or the typed
   abstention). Recommend-only: a chip opens the skill's detail sheet;
   running still goes through the skill's run and approval path.
5. **Multilingual routing eval.** `evals/skill_routing/multilingual.json`
   holds 36 Arabic/French requests over 18 skills; `router_live` routes them
   with the English cases and records per-language rates. First live run on
   bge-m3 at the shipped bars: ar top-1 83% / precision-when-confident 100%
   (coverage 56%), fr top-1 78% / 82% (61%), en top-1 53% / 70% (39%).
   **Not comparable across languages**: the Arabic/French inputs are direct
   task requests written for this eval, while the English cases are the
   skills' subtler own eval inputs, and the multilingual set covers 18
   skills, not 35. The result shows bge-m3 routes Arabic and French at least
   as well as the thresholds assume — it does not show the shipped bars are
   tuned for them. Re-measure with a language-matched English set before
   moving thresholds.
6. **Skill goals.** The goal dialog now offers Prompt or Skill. Only skills
   whose single required input is free text qualify (email-drafting,
   meeting-notes, presentation-builder, second-look, sheet-builder,
   thread-summary): a goal stores one string and an unattended slot has no
   file handle to give a skill that needs one. The claimed run id pins the
   skill run; a run that reaches an approval is left WAITING in Activity —
   a goal never approves anything.

## Consequences

- New ops: `memory.add|list|delete|search`. New workspace file
  `db/memory.json` (sealed).
- Sync does not replicate memory (record-type contract untouched).
- Licensing unchanged: no nanoMuse code or text adopted.

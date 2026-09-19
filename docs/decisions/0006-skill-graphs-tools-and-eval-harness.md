# Decision 0006 — Skill graphs, the tool layer and the eval harness

Date: 2026-09-18
Status: Accepted (implemented; four built-in skills decomposed; replay tier is a CI gate; live tier measured).

## Problem
Until this decision nothing in the core executed a skill: `skills.list` displayed 30
prose manifests, the only generation path was a fixed RAG prompt, there was no tool
registry, no executor, and the `eval_cases` on manifests were prose. Two designs were on
the table for making skills real — a ReAct-style loop where the model chooses the next
action each step, or a declared graph where control flow is data and the model only
fills typed slots. Harbor's contracts (durable run v3 with hash-chained events, protected
effects with receipts, skills-as-data with a closed tool catalog) favour the graph, and
the production model class (1–4 B parameters on device) cannot drive a free-form loop.

## Decision
1. **Graphs, not loops.** `schemas/graph.schema.json` defines `harbor.graph/v1`: node
   kinds `tool.call`, `model.structured`, `model.text`, `branch`, `map`, `approval`,
   `const`, `end`; edges are declared, and a back-edge must carry `max_iterations` and an
   `exhausted` continuation. `harbor.skill/v2` manifests embed or reference a graph;
   `v1` manifests stay valid as declarations and the UI says so.
   Structural consequences enforced by `harbor_core::graph`:
   - the tool allowlist is exactly the set of tools named by `tool.call` nodes — the
     model never emits a tool name (SEC-005 by construction);
   - every run is bounded before it starts (budgets + bounded cycles);
   - `/input` and `/host` are read-only blackboard roots; a `map` body is private;
   - `model.structured` output schemas must be grammar-convertible on the pinned
     runtime (anchored patterns, no pattern+length), checked at validation time.
2. **Tool contract as code.** `harbor_core::tools`: `ToolSpec` (JSON Schema, risk class,
   requirements, timeout, output limit), `ToolRegistry` (allowlist check → schema
   validation → sorted-key canonical hash → cooperative deadline → output limit).
   Built-ins are `read` or `propose` class only. Host resources reach tools through
   `ArtifactSource` / `KnowledgeSearch` traits so the same tools run under the FFI, the
   harness and unit tests.
3. **Executor over the durable substrate.** `harbor_core::executor` runs one graph per
   run on `harbor_agent`: generation-fenced lease (released whenever control returns),
   `run.step_started/completed` per node with node id and io hashes, budgets checked
   against durable counters before every node, blackboard persisted after every node
   (`RunStateStore`; the product path is `BlobStateStore` over the encrypted workspace
   blob store — policy 13), approvals as nodes (`run.effect_prepared` +
   `run.approval_requested` → `WAITING_APPROVAL`; `decide` continues from the declared
   edge), cancellation acknowledged by the executor. Structured model outputs are
   grammar-constrained when the provider declares `StructuredOutput` and validated
   below the model regardless, with bounded retries.
4. **Structured output on the GGUF path.** `ChatRequest` gains `response_schema` and
   `trace_key`; the llama.cpp provider converts the schema to a grammar
   (`json_schema_to_grammar` + grammar sampler) and declares `StructuredOutput`.
   Verified on the real runtime with the 260K-parameter fixture: the grammar alone
   yields schema-valid JSON.
5. **Eval harness with three tiers.** `harbor.skill_eval/v1` cases with typed assertions
   (`run_state`, `outcome`, `state_*`, `values_appear_in`, `batch_proposed`,
   `approval_requested`, `tools_within_graph`, `replay_verified`, `no_cassette_misses`, …).
   *Replay* answers model nodes from a cassette (`harbor.cassette/v1`, matched by trace
   key, then request hash) — runs in CI with no weights and is a test gate. *Live* runs
   the same cases against a real provider and binds the model/runtime identity.
   *Record* writes cassettes from live runs.
6. **Four skills decomposed** (`core/harbor_core/src/graphs/`): Formula Audit & Repair,
   Placeholder & Form Fill, Second Look, Meeting Notes. The remaining 26 stay prose
   declarations until decomposed.
7. **Invocation.** FFI: `skills.list` (graph facts, `runnable`), `tools.list`,
   `op.start_skill_run` (artifacts as bytes, background op), `run.decide`,
   `run.snapshot`, `eval.run_skill`. Flutter: the Skills surface stops claiming
   "signed … available to every run"; cards and the detail sheet show *Runnable graph*
   vs *Declaration only*; runnable skills get a Run sheet whose form is generated from
   the input schema, with live progress, the node trail and the approval flow.

## What the harness measured (honest numbers)
- **Replay tier:** 11/11 cases across the four skills pass in ~0.3 s with zero weights
  (`cargo test -p harbor_core --test skill_evals`). This proves tools, approvals,
  budgets, cancellation and chain replay; it does not measure model quality.
- **Live tier on Qwen2.5-1.5B-Instruct Q4_K_M** (`evidence/skill_evals/live-6a1a2eb6d156.json`,
  runtime `llama.cpp/llama-cpp-sys-2@0.1.156`): first design 5/10, after moving work
  from the model into deterministic nodes 6/11. The deterministic skill
  (Placeholder & Form Fill) passes 3/3 with no model. The remaining live failures are
  model facts, each pinned by an assertion: the 1.5B model marks unchanged formulas as
  "fixes" (rejected deterministically by `formula.build_operations`, so the run
  completes with a review list instead of proposing a bad batch), normalizes dates and
  invents an owner (caught by `text.verify_fields` / `values_appear_in`), and nudges a
  trivial lookup (`second-look/model_skip`).
- The design lesson generalises: every live failure was fixed or contained by adding a
  deterministic node, not by prompting harder. Model calls per skill: 0 (fill), 1
  (audit, notes, second look).

## Consequences
- Adding a skill is again data work — a graph, cases and cassettes — but the graph is
  real: it runs, it is replayable, and it is tested without a model.
- The run-event schema change is additive (see `28_Correction_Log.md` addendum);
  existing streams parse and replay unchanged.
- Nothing commits an artifact yet: approval ends with a bound proposal; the safe-commit
  path under the receipt (`harbor_artifacts::commit`) is the next integration step,
  together with the Work-surface diff view.
- The live tier needs a model on the qualification machine; `HARBOR_LIVE_MODEL_GGUF`
  selects it. The recommended small instruct model for that tier (Qwen3.5-0.8B) is not
  yet pinned as a test fixture; the tier ran on the locally present Qwen2.5-1.5B.

## Not done (explicitly)
- Safe-commit of approved batches through the FFI/UI; Work-surface diff of a proposal.
- Decomposition of the other 26 skills.
- A `SKILL.md` frontmatter importer (HBR-072) — the graph is the target format for it.
- Resume-after-crash is implemented (`Executor::resume`) but only its wrong-state
  refusal is covered by tests; a kill/restart test is pending.

## Verification
`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace` (incl. `graph_executor` 7, `skill_evals` 3, `skill_runs` 2,
`schema_grammar_probe` 3, `gguf_provider` 6 on the real runtime), app `flutter test`
27/27 against the rebuilt debug core, `flutter analyze` clean, dossier validator PASS.

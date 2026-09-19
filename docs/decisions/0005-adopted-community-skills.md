# Decision 0005 — Community skill methodology adopted as Harbor Skill v1 manifests

Date: 2026-09-17
Status: Accepted (nine built-in skills added; no schema, catalog or code change to the skill runtime).

## Problem
Two public skill collections — `anthropics/skills` and `MiniMax-AI/skills` — hold
well-tested working methods for document, spreadsheet, presentation and writing
tasks. Harbor's skill layer (`03_Architecture_Contracts.md` §6, HBR-070/071,
ACC-077) is deliberately narrower than the Agent Skills format both repos use:
a Harbor skill is data only, draws tools from a closed host catalog, cannot bundle
scripts, and cannot widen workspace privacy. The question was which of the 36
upstream skills carry methodology that survives that narrowing, and which do not.

## Decision
Adopt the *method*, not the files. Each adopted skill is rewritten as an original
`harbor.skill/v1` manifest in `core/harbor_core/src/builtin_skills.json`, restricted to
the catalog tools (`artifact.read`, `artifact.propose_batch`, `knowledge.search`,
`fs.read_workspace_file`, `clipboard.read`, `model.ask`) and Harbor's invariants
(verified numbers only, abstain instead of invent, typed batches bound to content
hashes, approvals for effects). No upstream text, scripts, templates or assets are
copied into the repository.

| Harbor skill (id) | Family | Method adopted from | Upstream license |
|---|---|---|---|
| Document Co-Authoring (`doc-coauthoring`) | Document Intelligence | `anthropics/skills` `doc-coauthoring`: context gathering → section-by-section brainstorm/curate/draft → cold reader test | no LICENSE file in that folder (see review) |
| Team Update (`team-update`) | Internal Communications | `anthropics/skills` `internal-comms`: 3P (Progress/Plans/Problems), newsletter clustering, FAQ format | Apache-2.0 |
| Second Look (`second-look`) | Second Look | `anthropics/skills` `discernment-nudge`: 2–3 specific fact/reasoning/context questions, once, with skip rules | Apache-2.0 |
| Skill Author (`skill-author`) | Skill Author | `anthropics/skills` `skill-creator` + Agent Skills spec: description as trigger, imperative instructions, minimal allowlist, eval cases | Apache-2.0 |
| Financial Model Review (`financial-model-review`) | Spreadsheet Analyst | `MiniMax-AI/skills` `minimax-xlsx` format guide: formula-first, color-coded cell roles, number-format matrix, fragile external links | MIT |
| Formula Audit & Repair (`formula-audit`) | Spreadsheet Analyst | `MiniMax-AI/skills` `minimax-xlsx` validation guide: static scan of the seven error types → recalculation → deterministic fixes only → human-review list | MIT |
| Placeholder & Form Fill (`placeholder-fill`) | Document Intelligence | `MiniMax-AI/skills` `minimax-docx` scenario B + `minimax-pdf` FILL: inventory first, "first do no harm", no revision marks for completion, diff check | MIT |
| Deck Review & QA (`deck-review`) | Presentation Builder | `MiniMax-AI/skills` `pptx-generator` QA/pitfalls + slide types: bug-hunt stance, placeholder grep, hierarchy and alignment rules, fix-and-verify cycle | MIT |
| Document Style Review (`document-style-review`) | Document Intelligence | `MiniMax-AI/skills` `minimax-docx` design principles: six-principle checklist, priority order, direct-formatting contamination | MIT |

Where the artifact engine has no typed operation for a fix (cell styling, style
definitions, slide layouts), the manifest says the finding is *reported for the user
to apply*; only text, table, cell and slide-text corrections are proposed as batches.
This keeps every instruction executable against `schemas/artifact_batch.schema.json`
as it stands.

## Not adopted, and why
- **`anthropics/skills` `docx`, `pdf`, `pptx`, `xlsx`** — source-available under
  Anthropic's terms, which prohibit derivative works and redistribution. Not read for
  method; nothing from them is in Harbor.
- **`theme-factory`, `brand-guidelines`, `canvas-design`, `algorithmic-art`,
  `frontend-design`, `slack-gif-creator`, `web-artifacts-builder`, `webapp-testing`,
  `mcp-builder`, `claude-api`, `academy-guide`** — require code execution, external
  services, or are Claude-product specific. Theme/brand application is deferred until
  the artifact engine exposes a style operation; without one a "theme" skill could only
  describe changes it cannot propose.
- **MiniMax `minimax-multimodal-toolkit`, `minimax-music-gen`, `minimax-music-playlist`,
  `buddy-sings`, `gif-sticker-maker`, `vision-analysis`** — depend on MiniMax or OpenAI
  cloud APIs. A skill cannot authorize egress (`13_Network_and_Storage_Policy.md`), and
  no vision-capable provider is registered in `25_Feature_Registry.json`, so an image
  analysis skill would describe a capability Harbor does not have.
- **MiniMax `frontend-dev`, `fullstack-dev`, `android-native-dev`,
  `ios-application-dev`, `flutter-dev`, `react-native-dev`, `shader-dev`** — developer
  tooling. Harbor's scope has no coding use case. (They may be useful as *developer*
  skills for building Harbor itself; that is a Claude Code plugin decision, not a
  product skill decision, and is out of scope here.)
- **`minimax-pdf` CREATE/REFORMAT and FILL for PDF forms** — PDF form fields are
  outside the bounded Office feature matrix (`core/harbor_render/src/pdf.rs`); DOCX
  placeholder filling covers the same user need inside scope.

## License review (required by authority)
- MIT (`MiniMax-AI/skills`) and Apache-2.0 (the adopted Anthropic skills) are both
  compatible with Harbor's Apache-2.0 distribution. Because no upstream text is
  reproduced, no notice or attribution clause is triggered; the source attribution above
  is kept for provenance.
- `doc-coauthoring` in `anthropics/skills` carries no `LICENSE.txt` and the repository
  has no root license. Only its three-stage *process* (a generic co-writing workflow)
  is used, expressed in original prose; process is not protected expression. If the
  upstream later declares restrictive terms, the manifest stands on its own wording.
- Financial color-role conventions and the seven spreadsheet error types are
  industry facts, not MiniMax expression.

## Validation
- `cargo test -p harbor_core skills`: every manifest passes `SkillManifest::validate`
  (catalog allowlist, privacy policy, code-marker rejection); new tests assert unique
  ids, eval-case coverage and the presence of the adopted set.
- `apps/harbor_app` shell test (`skills surface lists the real builtin skills`) still
  passes: the surface groups by family, so the adopted skills appear under their
  authority families (Document Intelligence ×4, Spreadsheet Analyst ×3, Presentation
  Builder ×2) plus three new family headers.
- The Agent Skills `SKILL.md` format itself was **not** adopted as an import format.
  HBR-072/UX-027 already call for a "YAML/Markdown skill schema" for user skills; a
  frontmatter-compatible importer that maps `name`/`description`/body onto
  `harbor.skill/v1` is the natural M3 implementation, recorded here as the follow-up.

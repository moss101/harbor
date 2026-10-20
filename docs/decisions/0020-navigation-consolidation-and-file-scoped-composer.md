# Decision 0020 — Seven surfaces, one composer with scopes, a recommend-only request router, and "Ask about this file"

Date: 2026-10-20
Status: Accepted (implemented; verification scope stated in section 6). Follows 0017–0019.

## 1. Context

Three things were wrong with the app's shape, not with its core:

- **Work had no input.** A file could be opened and read, but nothing could be asked or
  asked of it. The only way to question a file was to leave Work, index it into Knowledge,
  and ask on the separate Ask surface, which searched everything.
- **The Home composer only logged.** It produced a durable run record and went nowhere;
  a typed request never reached a model or a skill.
- **Ten destinations** (Home, Ask, Work, Knowledge, Models, Store, Skills, Agents,
  Activity, Settings, with Goals inside Agents) was more navigation than the work
  justified on a phone, split one journey ("ask, then act") across four of them, and
  exposed Agents although agent orchestration is a disabled optional capability
  (`N/A_DISABLED` in `docs/release_checklist_status.md`).

## 2. Decision

### 2.1 Seven surfaces

| Surface | What it holds now |
|---|---|
| **Home** | One conversation: questions are answered inline (this absorbs Ask), tasks open a skill's run sheet. The dashboard is its empty state. |
| **Work** | The open file, plus a command bar scoped to it. |
| **Library** | The Knowledge index (the surface previously named Knowledge). |
| **Models** | Recommended, Installed, Hugging Face, **Store**, Benchmark (Store is a tab, not a destination). |
| **Skills** | unchanged |
| **Activity** | Runs, operations and **scheduled Goals** (Goals moved here from Agents). |
| **Settings** | unchanged |

Home, Work, Library and Models are primary; Skills, Activity and Settings are the rail's
lower group (the More sheet on phones). **Agents is hidden**, not deleted: while agent
orchestration is off it is not a destination, and its working part, goals, lives in
Activity. Keyboard: Cmd/Ctrl+1..7 in rail order.

### 2.2 `HarborComposer` with scopes

One composer widget serves Home and Work. A scope says what a request is about: Home's is
the Library (or a file attached to the turn); Work's is **this file** (`This file · <name>`).
A mode switch lets the person choose **Ask** (a grounded answer) or **Change** (a task);
with no mode picked the request is routed (2.3). The composer submits nothing on its own
authority: Change opens the skill's own run sheet with the file pre-attached and the
request prefilled, and nothing runs until the person presses Run there and approves what
the run proposes (decision 0008 approval/commit path is unchanged).

### 2.3 `RequestRouter` is recommend-only

`lib/services/request_router.dart`. Order of authority: (1) a mode the person picked;
(2) `decision.choose` (decision 0019 section 3) names the KIND of action - `run_skill` is a task,
`search_documents` / `general_chat` / `recall_memory` are questions; on abstention, another
kind, or no index, a punctuation/opener heuristic decides; (3) a task is ranked by
`skills.suggest`, and with a file in context only skills that take an `artifact_id` qualify
(without a file, only skills that do not require one). It returns a decision for the caller
to SHOW (`RouteAsk`, `RouteSkill`, `RouteNone` with a reason) and never starts or authorizes
anything. Without an index it offers lexical name matches as choices, never as a pick.

The default chat model is the person's choice when still installed, else the first
installed package that is not an embedder (`chatPackageFor`). That second clause was added
after the live test (section 6) showed the previous rule - "first installed" - selecting
`bge-small-en-v1.5` over a chat model and failing every question with a chat-template
error. The core does not yet say which package is an embedder, so the app recognizes
embedders by id; a core-side `role` on `models.installed` would be the proper fix.

### 2.4 The `document` scope on `op.start_generate`

"Ask about this file" is the existing grounded-generation op with a second evidence
source. **Optional argument:**

```
document: { name: string,
            passages: [ { id: string, text: string, locator: string } ] }
```

When present, the answer is grounded ONLY on those passages and the Knowledge index is
not searched (it need not even be open). The host extracts the passages from the core's
own preview IR (`lib/services/file_passages.dart`): DOCX one per non-empty paragraph
(`para-<i>`, `¶<i>`); PDF one per page (`page-<n>`, `page <n>`); PPTX one per slide
(`slide-<n>`); XLSX one per non-empty cell of the shown sheet (`cell-<A1>`, `<Sheet>!<A1>`).
The file's text goes to the core for that call only; nothing is indexed or kept.

**Caps** (validated in `core/harbor_ffi/src/doc_scope.rs`, refusal names the limit): at most
**5000 passages**; **64 KiB** of UTF-8 per passage; **4 MiB** total passage text; 512 bytes
per name/id/locator. The app stays below these (1500 passages, 1500 chars per passage,
3 MiB) so a large workbook is cut by the app, not refused by the core.

**Response:** the usual generation result plus `scope: {kind: "document", name, ranking,
passages_used, ...}`. Each citation carries `passage_id`, `locator` and `snippet`, plus the
usual `title` (the document name), `state` and `score`, so the UI maps a citation back to a
place in the canvas and a person can check it. A question the passages cannot answer
returns `INSUFFICIENT_EVIDENCE` (the same abstention as the Library path). A supplied
`run_id` records the step as `[document-scoped: <name>]` in the run trail.

**Ranking** (reported as `scope.ranking`):
- `all` - the whole document fits the evidence budget (~2400 characters, the same as the
  Library path's three chunks): every passage is evidence, in document order. Used for short
  files; no embedding involved.
- `embedding` - the document is longer: a lexical pre-filter picks 64 candidates and the
  open embedding model re-ranks them by cosine; the best passage is always taken,
  truncated if needed.
- `lexical` - the same selection by BM25 when no embedding model is open or it failed.
At most 8 passages are cited in one answer.

## 3. What this deliberately does not do

- It does not make Work an editor. Change always goes through a skill's run sheet,
  approval and the safe-commit journal; "Read-only preview" still means that.
- The router does not choose for the person, and the chat model never decides routing.
- Agents are not removed from the core or the catalog; only the destination is hidden.
- `07_Screen_Inventory.csv` is not edited (frozen authority); see section 5.

## 4. Consequences

- Asking about one file no longer requires indexing it, and a file's answer cannot leak
  from or into other Library sources (a test proves the Library is not searched).
- Phones keep four primary destinations plus More, as before; two destinations were removed
  from the rail (Ask, Store) and one made reachable only through Activity (Goals).
- The authority screen inventory names Ask, Store, Agents and Knowledge as surfaces; their
  new homes are mapped in `28_Correction_Log.md` until the CSV is re-issued.
- The `document` argument is a new additive field of `op.start_generate`; existing callers
  are unaffected. Hosts other than Flutter must build passages themselves.
- Embedder detection by package id is a heuristic and will need the core `role` field.

## 5. Dossier

Affected inventory rows (UX-004/005/006/007/014/015/016/019/020/028/029/030) and their new
homes are listed in an addendum to `28_Correction_Log.md`. The CSV is pinned by hash and is
re-issued with the next authority revision.

## 6. Verified, and not verified

Verified:
- Fake-service widget tests (`work_command_bar_test.dart`, `home_conversation_test.dart`,
  `navigation_test.dart`, `request_router_test.dart`): the bar's presence only with a file
  open, the exact `document` payload sent, a citation tap marking the paragraph, the
  no-model guidance, Change opening the run sheet with the file attached and nothing run.
- Rust, real weights (`core/harbor_ffi/tests/document_scope_live.rs`, Qwen2.5-1.5B +
  EmbeddingGemma 2): grounded and cited with and without the index open, abstention,
  `scope` shape, `embedding` ranking over a 30-passage file, no Library leakage, run-trail
  step name.
- Live-core Flutter tests (`apps/harbor_app/test/work_live_test.dart`, real service + real
  core + real model; skipped when the fixtures are absent). Results are recorded in
  `docs/STATUS.md` for this date, including what failed first.
- Phone header: `work_header_test.dart` (390x844, English and Arabic) - the header's actions
  wrap instead of scrolling out of view (`HarborSurfaceHeader`).

Not verified:
- Any of this on a physical iOS or Android device, or on Windows. The picker, the file
  access and the Work canvas on touch were exercised only on simulators and desktop.
- Passage extraction on large or unusual files (scanned PDFs with no text layer yield no
  passages, and the bar says the file has no readable text rather than guessing).
- Arabic questions about Arabic files with the 1.5B model; the router's Arabic openers are
  unit-tested, not measured for accuracy.
- Routing quality on real user phrasing beyond the decision-head benchmarks of 0019.

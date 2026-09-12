# Harbor specification gap analysis

Reviewed on 12 September 2026.

Harbor has a well-developed architectural direction, but the current package is not ready for an implementation contract freeze. The most consequential gaps are inconsistent release applicability, milestone dependencies, and schemas that do not enforce several of the invariants promised by the prose. These should be corrected before implementation teams treat the package as executable authority.

This is a review of the supplied specifications, including both Word authorities, the CSV ledgers, JSON schemas, policies, tokens, and manifest. There is no application source or implementation evidence in this workspace. Findings describe specification defects and risks, not demonstrated vulnerabilities in a running product. Original authority files were left unchanged. Word content was extracted for substantive review; page layout and actual product UI were not visually certified.

**Priority:** P1 means resolve before contract freeze or implementation of the affected subsystem. P2 means resolve before the relevant qualification milestone.

## Checks performed

| Check | Observed result |
|---|---|
| Package integrity | All 42 manifest entries match their recorded sizes and SHA-256 hashes |
| Ledger counts | 114 tasks, 63 gates, 47 screens, 47 security scenarios |
| Referenced IDs | All checked task, gate, screen, and security references resolve |
| Task dependency graph | Acyclic, but 25 direct dependencies point to later milestones |
| Gate timing | 7 gate-to-task links, affecting 6 gates, require later-milestone tasks |
| Release matrix projection | File 18 matches the corresponding fields in file 05 |
| Security traceability | 10 scenarios have no acceptance-gate mapping, including 6 P0 scenarios |
| Task traceability | 36 tasks have no direct acceptance-gate mapping, including 20 marked release blocking; some may be covered indirectly |
| Screen traceability | 38 of 47 screens reference only HBR-160, the route-mapping task |
| Schema probes | All 5 schemas are valid Draft 2020-12 schemas; 7 invalid contract examples nevertheless validate |
| Additional schema probes | Explicit build-digest and canonical-arguments fields are rejected because they are not defined |
| Published semantic contrast pairs | All 10 supplied status/action pairs reproduce the reported passing ratios |

Schema probes used a Draft 2020-12 validator with date-time format checking enabled. Ledger checks used parsed CSV values rather than text matching. These checks establish the results above; they do not run the future Harbor gate evaluator.

## Findings

### R1 P1 Correct the core and optional feature assignments

The release fields contradict the capability they describe:

- ACC-009, which prohibits protected effects without approval, is conditional on `feature:hf_public`. A configuration with public Hub acquisition disabled drops this particular gate, although protected effects still exist. Other approval gates remain, so this is a coverage defect rather than proof that all approval protection disappears.
- HBR-100, the central Egress Broker, is M4, nonblocking, and conditional on connectors. Core privacy gates ACC-002 and ACC-038 require it, and the network policy requires it for all app-controlled traffic.
- HBR-103 implements mail/calendar connectors but is conditional on remote inference.
- ACC-023 is a core M3 sync gate whose implementing tasks are optional M4 sync tasks. ACC-008 and ACC-030 also mix core protections with optional authentication/diagnostics implementation.

**Improve:** make the broker and protected-effect checks unconditional core requirements. Put connector implementation under the connector flag. Separate core checks that optional services remain unavailable from enabled-service qualification. Apply the same distinction to authentication, sync, and diagnostics.

Evidence: [ACC-009](/Users/mohsin/projects/harbor/05_Acceptance_Matrix.csv:10), [HBR-100](/Users/mohsin/projects/harbor/04_Implementation_Backlog.csv:69), [HBR-103](/Users/mohsin/projects/harbor/04_Implementation_Backlog.csv:72), [ACC-023](/Users/mohsin/projects/harbor/05_Acceptance_Matrix.csv:24), [egress authority](/Users/mohsin/projects/harbor/13_Network_and_Storage_Policy.md:16).

### R2 P1 Add an activation gate for every optional capability

At M4, enabling any of `connectors`, `diagnostics`, `hf_auth`, or `hf_gated` adds **zero** gates under the documented selection rule. The three system-provider flags also add none. Only sync and remote inference have feature-specific M4 rows. This does not implement the promise that optional capabilities require their own qualification.

The selection rule also needs a configuration-validity step: an M3 descriptor with sync enabled would exclude M4 sync gates by milestone unless a separate rule rejects that configuration.

**Improve:** add a typed feature registry with dependencies, earliest activation milestone, and mandatory qualification gates. Reject unknown flags, invalid combinations, and enabled features with no gate coverage. Test feature-on and feature-off configurations, including the disabled default. A disabled-default test should still run when the feature is off.

Evidence: [release algorithm](/Users/mohsin/projects/harbor/01_Release_and_Scope_Authority.md:6), [complete gate catalog](/Users/mohsin/projects/harbor/05_Acceptance_Matrix.csv), [optional-service checklist](/Users/mohsin/projects/harbor/10_Release_Checklist.md).

### R3 P1 Make milestone ordering executable

An acyclic graph is not enough to make the schedule achievable. There are 25 direct dependencies on later milestones. For example, HBR-148 is M0 but depends on the M3 workspace store HBR-080. HBR-064 needs that store at M1. HBR-161 is an M0 end-to-end prototype that depends on M1 inference and Office pipelines. ACC-059 requires accessibility evidence at M2 while one implementing task, HBR-135, is scheduled for M3.

**Improve:** split design work from implementation/proof work. Keep schema and policy definitions in M0, deliver the minimal persistence, file capabilities, inference, and Office path in M1, and schedule platform expansion afterward. Enforce `dependency milestone <= dependent milestone`, or define an explicit earlier deliverable when a larger task spans milestones. Check the same constraint for gates and implementing tasks.

Evidence: [HBR-148](/Users/mohsin/projects/harbor/04_Implementation_Backlog.csv:102), [HBR-080](/Users/mohsin/projects/harbor/04_Implementation_Backlog.csv:52), [HBR-161](/Users/mohsin/projects/harbor/04_Implementation_Backlog.csv:115), [release checklist](/Users/mohsin/projects/harbor/10_Release_Checklist.md).

### R4 P1 Prevent evidence-free release results from validating

The following record passes the supplied release-result schema:

```json
{
  "schema": "harbor.gate_result/v1",
  "gate_id": "ACC-041",
  "status": "PASS",
  "commit_sha": "",
  "evidence": [""]
}
```

Platform, enabled features, and completion time are optional. There is no explicit build artifact digest or qualification target. Adding a `build_sha256` property is rejected. Consequently, schema validation alone cannot establish the build-bound, platform-specific evidence required by the release authority.

**Improve:** require a release descriptor identity, platform/architecture, feature configuration, commit identity, and exact build digest. Define evidence objects with nonempty locations and content hashes. Apply status-specific requirements, including an exclusion reason for N/A. The evaluator must verify evidence existence, integrity, gate identity, and target compatibility; JSON Schema cannot establish those external facts by itself.

Evidence: [release-result schema](/Users/mohsin/projects/harbor/schemas/release_gate.schema.json:5).

### R5 P1 Bring runtime and artifact schemas up to their stated contracts

Three tested records pass validation despite violating the intended contract:

- A dispatched effect using `provider_key` mode with no provider key or approval binding.
- An executor authority event without a lease generation and with an empty payload.
- An artifact batch with repeated operation IDs, an unknown operation kind, and empty preconditions.

The effect schema stores only the arguments hash and has no defined canonical-arguments field or immutable arguments reference. Adding `canonical_args` is rejected. The package also lacks a machine-readable approval-receipt contract despite relying on expiry, scope, and effect binding.

**Improve:** define typed event and operation variants, conditional authority fields, an immutable intent payload/reference, and a versioned approval-receipt contract. Specify canonicalization and hash binding. Enforce operation-ID uniqueness, lease ownership, state transitions, expiry, and monotonic counters in semantic validators and transactional runtime checks. Keep these checks separate from structural validation where necessary. JSON Schema supports conditional required fields through `if`/`then` and related keywords. [Official JSON Schema reference](https://json-schema.org/understanding-json-schema/reference/conditionals).

Evidence: [effect schema](/Users/mohsin/projects/harbor/schemas/effect_intent.schema.json:5), [run schema](/Users/mohsin/projects/harbor/schemas/run_event.schema.json:5), [batch schema](/Users/mohsin/projects/harbor/schemas/artifact_batch.schema.json), [runtime authority](/Users/mohsin/projects/harbor/02_Runtime_Effect_and_Artifact_Contracts.md:13).

### R6 P1 Make the model reference variants enforceable

The model schema accepts an `installed_package` with no files. It also accepts a weight file with no path and an invalid SHA-256 string, and a file whose path is `../../outside.gguf`. The `system_managed` and `remote_endpoint` variants do not require a provider or a defined provider/model identity.

These are schema weaknesses, not evidence that a future installer would accept the same data after its own safety checks.

**Improve:** use distinct required fields for installed, system-managed, and remote references. Installed packages need a nonempty file set, valid digests, bounded sizes, normalized relative paths, and package completeness checks. Enforce containment, duplicate/case-colliding paths, and links during extraction/install. System and remote variants should identify a configured provider and model explicitly.

Evidence: [model-package schema](/Users/mohsin/projects/harbor/schemas/model_package.schema.json:5), [installation authority](/Users/mohsin/projects/harbor/17_Model_Provider_and_Package_Contract.md).

### R7 P1 Connect security scenarios to release evidence

Ten security scenarios have no gate mapping: SEC-004, SEC-005, SEC-015, SEC-017, SEC-020, SEC-022, SEC-023, SEC-024, SEC-025, and SEC-026. Six are P0, including skill prompt injection, clipboard access, deletion mistakes, and cross-workspace context leakage.

Some protections are mentioned in task prose or broader gates. That does not provide deterministic scenario-level evidence coverage, and no separate security-result evaluator is supplied.

**Improve:** map each applicable scenario to a gate and executable test identifier. Alternatively, define a mandatory security-suite gate that inventories the complete scenario set and fails on missing results. Optional scenarios need feature applicability and explicit exclusions. Do not require one gate per task; require a traceable route from each safety invariant to evidence.

Evidence: [security matrix](/Users/mohsin/projects/harbor/09_Security_Test_Matrix.csv), [acceptance matrix](/Users/mohsin/projects/harbor/05_Acceptance_Matrix.csv).

### R8 P2 Repair screen identities and state coverage

The route mappings contain semantic errors that valid IDs cannot detect:

- ACC-047 maps artifact commit/replay evidence to UX-018, Models recommended.
- ACC-052 maps DOCX/PPTX rendering to UX-016, Agent run detail, and UX-018, Models recommended.
- ACC-061 maps Work Canvas layout to agent/model screens while omitting the document, spreadsheet, and presentation workspaces UX-008–010.
- Run lifecycle states are assigned to UX-014, Agents gallery. UX-016, Agent run detail, has only generic loading/empty/ready/offline/error states.
- UX-009 has no explicit formula-unverified, proposal, saving, or conflict states; several of these appear on the approval sheet instead.

Also, 38 screens reference only the mapping task HBR-160, which does not identify the actual feature implementation.

**Improve:** correct the route references, attach feature implementation tasks, and define screen-state coverage as route × relevant state × platform × language. Shared recovery screens can remain shared, but their entry points and originating-screen behavior must be mapped.

Evidence: [screen inventory](/Users/mohsin/projects/harbor/07_Screen_Inventory.csv:10), [ACC-047](/Users/mohsin/projects/harbor/05_Acceptance_Matrix.csv:48), [ACC-052](/Users/mohsin/projects/harbor/05_Acceptance_Matrix.csv:53), [ACC-061](/Users/mohsin/projects/harbor/05_Acceptance_Matrix.csv:62).

### R9 P2 Resolve the mobile Work Canvas width contradiction

The design authority requires an editable Work Canvas of at least 640 logical pixels, while compact layouts are under 600 pixels and use 16-pixel gutters. At a 390-pixel viewport, only 358 pixels remain. Collapsing the rail and Lens cannot satisfy the unconditional minimum. ACC-061 applies the minimum at every supported width.

**Improve:** restrict the 640-pixel minimum to the desktop editing arrangement. Define the compact alternative explicitly: a responsive editor, a scrollable document surface with viewport-sized controls, or a preview plus structured editing actions. State which dimensions refer to the viewport versus document content. Add narrow-width and enlarged-text acceptance cases.

Evidence: [design authority, sections 2 and 4](/Users/mohsin/projects/harbor/Harbor_UI_UX_Design_Authority_Reconciled.docx), [layout tokens](/Users/mohsin/projects/harbor/06_Design_Tokens.json), [ACC-061](/Users/mohsin/projects/harbor/05_Acceptance_Matrix.csv:62).

### R10 P1 Specify the final artifact commit and recovery protocol

The specification requires a last-minute base check and atomic replacement, but does not define how external writers are excluded between that check and replacement. Consider: Harbor verifies version A, another editor writes B, then Harbor replaces the file with C. A replacement being indivisible does not by itself make it conditional on version A.

The Word authority also persists committed version metadata after replacement. The recovery decision for a crash between replacement and metadata commit is not specified, particularly if another editor changes the output before recovery.

**Improve:** document the coordination or conditional-write primitive for each supported storage provider. Where the required guarantee cannot be established, use the existing new-copy fallback. Persist commit preparation and intended output identity before replacement, then define recovery for each durable boundary. Extend ACC-046/047 and SEC-043 to test the final check-to-replace interval and replacement-to-metadata interval. This is a missing protocol detail, not a demonstrated data-loss bug.

Evidence: [artifact contract](/Users/mohsin/projects/harbor/02_Runtime_Effect_and_Artifact_Contracts.md:22), [implementation authority, section 9](/Users/mohsin/projects/harbor/Harbor_Implementation_Authority_Reconciled.docx).

### R11 P2 Define qualification procedures before collecting results

The performance file lists metrics, but does not pin the starter model, context size, input/output lengths, artifact fixture hashes, sampling/repetition method, or exact reference device configurations. The evaluation contract names datasets and thresholds without supplying their manifests or values. Office rendering promises a supported subset without quantitative fidelity tolerances or an enumerated subset for entries such as headers, footers, and page sections.

The package already discloses that performance is blocked and all 71 formula target entries are unqualified. Those disclosures are appropriate; they are not new defects. The additional gap is the reproducible measurement and acceptance procedure.

**Improve:** pin benchmark workloads and fixture versions, define metric direction and statistical treatment, publish per-platform target identities, and set approval rules for thresholds before GA qualification runs. Define numerical tolerances, spreadsheet date/error/coercion behavior, and visual-difference tolerances. Deliver a small known-answer workbook-to-deck proof before broadening the Office promise.

Evidence: [performance contract](/Users/mohsin/projects/harbor/15_Performance_Qualification.yaml), [evaluation contract](/Users/mohsin/projects/harbor/16_Index_and_Evaluation_Contract.md:7), [Office matrix](/Users/mohsin/projects/harbor/21_Office_Feature_Matrix.csv), [formula manifest](/Users/mohsin/projects/harbor/22_Formula_Coverage.json).

### R12 P2 Make the structural validation reproducible and accurately scoped

Files 19 and 20 report no issues, and file 23 marks several affected specification gaps resolved. The package includes validation outputs, but no script or command that regenerates them. Current checks evidently do not cover the semantic inconsistencies identified above.

The reported 60 core GA gates is a cross-platform set. Interpreting `Desktop/All` as All, the declared core/public-HF/RAG/Arabic-OCR configuration yields 58 gates for each mobile platform and 59 for each desktop platform. Those are gate-definition counts, not architecture/device qualification results.

**Improve:** ship a reproducible package validator, its version, and the input manifest digest. Add milestone ordering, feature closure, activation coverage, security coverage, route semantics, and negative schema fixtures. Report aggregate and target-specific counts separately, normalize platform selectors, and reopen affected reconciliation entries until the new checks pass.

Evidence: [structural validation](/Users/mohsin/projects/harbor/19_Structural_Validation.json), [readiness report](/Users/mohsin/projects/harbor/20_Readiness_Validation_Report.md), [reconciliation status](/Users/mohsin/projects/harbor/23_Review_Reconciliation_Matrix.md).

## Additional improvements to schedule

**Cancellation recovery:** define cancellation during CREATED/PLANNING, acknowledgement timeout, and recovery when a provider dies before acknowledging cancellation. Separate the fact that execution stopped from the unresolved outcome of an already dispatched external effect. The current CANCELLING rule only permits termination after acknowledgement.

**Temporary plaintext:** define approved decrypted working windows, crash cleanup, backup exclusions, and platform-specific protection. Removing encryption keys does not erase an already decrypted temporary file. The existing leak tests should distinguish encrypted blobs from authorized plaintext working files and disclose residual cleanup limits.

**Optional sync:** before activation, specify the transfer handshake that fences an old executor, the maximum offline-device horizon, tombstone collection and re-enrollment behavior, and permitted settings fields. The phrases “explicitly transferred” and “long enough to reach known devices” do not define executable behavior under partitions or indefinite absence.

**Component contrast:** the ten published semantic pairs pass. A separate computed combination, light `inkMuted` on `brandSoft`, is approximately 4.218:1 and should not be used for normal text. This is a token-combination warning, not an observed UI failure. Define allowed text/surface pairs and verify actual component states.

## Recommended implementation sequence

1. Correct release flags, feature activation coverage, and milestone ordering. Regenerate the release matrix from one authority.
2. Tighten the five schemas and specify semantic validators, approval receipts, and artifact commit recovery.
3. Repair security and screen traceability, including the mobile editing contract. Regenerate validation and reconciliation status.
4. Prove the narrow local workbook-to-deck workflow with pinned fixtures on a desktop and mobile reference device, then expand platform and feature coverage.

The architecture does not need a wholesale redesign. The next useful investment is making its existing promises precise, enforceable, and reproducible.

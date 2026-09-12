# Harbor review reconciliation matrix

Revision 3 addresses the 12 September review. Contract corrected means that requirements and validation now address the finding; it does not mean the future Harbor runtime or product gates have passed.

| Finding | Contract correction | Implementation and qualification | Verification |
|---|---|---|---|
| R1 Core and optional assignments | Core approvals/broker; connector flag; separate default/activation checks | HBR-100/147/150; ACC-009/023/030/064 | Core flag and gate/task applicability regressions |
| R2 Optional activation | Registry with dependencies, platforms, earliest milestones and activation gates | File 25; ACC-064/066–074 | Each feature on each supported platform; early/unknown/dependency failures |
| R3 Milestone ordering | Contracts M0; runtime/store/Office M1; platform breadth M2; GA quality M3 | File 04; ACC-075/076/081 | No later dependencies or gate-before-task links |
| R4 Evidence binding | Descriptor, build/commit/target/features and hashed evidence | Descriptor v1; gate result v2; HBR-147 | Missing/changed/wrong-build evidence rejected |
| R5 Runtime envelopes | v3 typed records, canonical intent, single-effect receipts and semantic checks | File 02; approval v1; ACC-042–047/076 | Approval, lease, sequence, counter and operation fixtures |
| R6 Model variants | Required files/path/hash/source/license/storage; system/remote identity | Model v3; HBR-157; ACC-058/070–072 | Missing/escaping/colliding/private-storage cases rejected |
| R7 Security coverage | Scenario applicability, test IDs and owning gates | File 09; ACC-065 and feature gates | Coverage/backlinks; runtime tests still required |
| R8 Screen traceability | Real tasks, corrected workspace/run routes and recovery links | File 07; HBR-160; ACC-061/062 | Semantic mappings and required states |
| R9 Compact layout | Desktop-only 640 px minimum; viewport-sized compact controls | File 06; design authority; ACC-061 | 320/390 px and 200% scale qualification requirements |
| R10 Artifact publication | Durable prepare, exact-destination recovery, conditional overwrite or new copy | File 27; commit journal v1; ACC-046/047/081 | Schema/crash fixtures; actual OS adapter evidence required |
| R11 Measurement procedure | Sizes/repetitions/identity/tolerance/language rules and source cases | Files 15/21/22/26; ACC-051–056/063/081 | Fixture hashes; missing production bindings block product gates |
| R12 Reproducibility | Packaged validator/regressions, input digest and target counts | tools/; files 19/20/24 | python3 tools/validate_dossier.py |
| Cancellation | Five-second recovery pause; terminal acknowledgement/termination proof | File 02; ACC-042/076 | Terminal/proof/transition fixtures; device timing remains evidence |
| Plaintext | Operation windows, backup exclusions, restart cleanup and erasure limits | File 13; SEC-036/037/038 | Contract supplied; filesystem/OS leak tests remain required |
| Sync | Source-stop acknowledgement, 90-day expiry and >=120-day tombstones | File 11; ACC-057 | Contract supplied; partition/restore tests remain required |
| Contrast | Darkened muted text and allowed text/surface pairs | File 06; ACC-059 | Unrounded token checks; actual component audit still required |

All findings are corrected at the specification level subject to the current validation output. Product evidence remains BLOCKED where missing. No formula or external overwrite adapter is implicitly qualified. Initial source cases do not replace the complete platform, security, Office and evaluation corpora.

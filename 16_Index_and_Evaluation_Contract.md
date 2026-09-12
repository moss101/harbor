# Harbor index and evaluation contract

`IndexIdentity` includes embedding provider/model content hash, vector dimension, chunker ID/version/config, tokenizer ID/version, normalization policy, language policy and encryption scope. A changed identity never reuses an old vector namespace. Harbor builds a new index, atomically switches after validation, then retires the old one.

Removing/revoking a source immediately excludes it from future retrieval and schedules chunk/cache deletion. Existing chat text remains historical unless the user selects purge-derived-content; its citation resolves as `source_removed` and is not treated as current evidence.

The pinned evaluation suite contains English, Arabic and mixed-language cases for retrieval recall, citation support, contradictory sources, insufficient-evidence abstention, prompt injection, tool selection, numerical calculation and protected-effect avoidance. Passing requires zero unauthorized effects. Numerical and citation gates use exact expected fixtures where available rather than subjective grading. Dataset version, evaluator version, model/runtime identity and thresholds are stored with results.


## Reproducible evaluation
The evaluation protocol in 26_Qualification_Profiles.json fixes language strata, metric denominators, seed, allowed tolerances and baseline acceptance thresholds. Missing pinned corpus/model/runtime identities block evaluation. Security outcomes are binary: no unauthorized effect is allowed. Citations are scored per cited claim against the exact versioned span, abstention on insufficient-evidence cases, and tool selection against fixture expectations. Report every language stratum; a combined score cannot hide an Arabic failure. Numeric fixtures compare independently specified expected results, including dependency chains and stale caches.

Source updates create new source versions. Index chunks and citations retain source-version/content hashes. Retrieval filters source authorization/version before ranking and again before context assembly; revoked results already queued for assembly are removed. Derived-content purge is an explicit cascade over retained artifacts, caches and chat copies with a preview, while exported copies remain outside Harbor control.

# Harbor — Model Licenses (release candidate 1.0.0)

Harbor bundles **no model weights** in the application binary. Model
packages are downloaded on demand from Hugging Face at the user's explicit
request. The signed catalog records the upstream license of each package
and the app displays it before acquisition. Current catalog entries:

| Package | Upstream license | Upstream owner | Source |
| --- | --- | --- | --- |
| `qwen2.5-1.5b-instruct` (GGUF) | Apache-2.0 | Alibaba Cloud / Qwen team | huggingface.co/Qwen/Qwen2.5-1.5B-Instruct |
| `bge-small-en-v1.5` | MIT | BAAI | huggingface.co/BAAI/bge-small-en-v1.5 |
| `stories260k` | Apache-2.0 | tinyllamas | acquired as `tinyllamas/stories260K.gguf` |

User responsibilities (shown in-app before first download):

- Each model's weights remain subject to its upstream license. The user is
  responsible for complying with the license of any model they acquire,
  including any use restrictions or attribution terms.
- Model behavior is determined by the upstream model, not by Harbor;
  Harbor's qualification covers runtime behavior (determinism, fit,
  cancellation, evaluation corpus), not the truthfulness of model output.
- The signed catalog epoch and per-file hashes protect integrity of what
  is downloaded; they do not change any license.

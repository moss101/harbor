# What EmbeddingGemma 2 actually gave Harbor

Date: 2026-10-10. Every number marked **measured** was produced on this machine
(Mac17,8, M5 Pro, 24 GB, Metal, Q8_0 weights) in this repository, by a command
named in the last section. Numbers marked **published** are Google's, as reported
by the model card and secondary coverage; I could not reach the card's benchmark
table directly, so treat those as unverified until checked against
`ai.google.dev/gemma/docs/embeddinggemma/model_card_2`.

## 1. What the model is

Published: 740M parameters in total (a 270M text model plus ~170M vision and ~300M
audio encoders), Apache-2.0, one 768-dimensional space for text (including code),
images, video and audio, Matryoshka truncation to 512/256/128, task-prefixed inputs.
Harbor runs the text tower alone at 310 MB (Q8_0) and adds the vision/audio towers
through the `mmproj` projector when the multimodal package is installed.

## 2. What we gained (measured)

### 2.1 One index for text, pictures and sound - the capability BGE-M3 does not have
- Text -> image Recall@1 **0.874** over 200 Flickr8k photos; text <-> audio Recall@1
  **1.000** over 73 LibriSpeech clips (small pools - decision 0015); video as sampled
  frames embeds as one item. A scanned invoice, a photo of a receipt and a voice memo
  can sit in the same Knowledge index as the contracts.
- Runs on the phone: a real iPhone (A19 Pro GPU) reproduced the Mac's text scores and
  embedded images at ~270 ms each; a real Android phone (CPU) matched the Mac, at
  ~3.5 s per image (decisions 0013, 0015, 0016; not re-run in this session).

### 2.2 Smaller and lighter than BGE-M3 (same passages, same machine)

| Embedder | Weights | Dim | Load | p50 / p95 per passage | Passages/min | Peak RSS |
|---|---|---|---|---|---|---|
| EmbeddingGemma 2 | 310 MB | 768 | 0.3 s | 26.5 / 36.8 ms | 2,125 | 772 MB |
| BGE-M3 | 635 MB | 1024 | 0.25 s | 17.6 / 20.7 ms | 3,334 | 1,373 MB |
| multilingual-e5-small | 132 MB | 384 | 0.25 s | 10.2 / 12.6 ms | 5,826 | 406 MB |

Half the weights and ~44 % less memory than BGE-M3 is what makes a phone viable. The
**cost is speed**: EmbeddingGemma 2 is ~1.5x slower per passage than BGE-M3 here, and
2.6x slower than e5-small. (RSS is a `ps` sample, an upper-bound proxy.) Matryoshka
512/256 indexes shrink vectors by a third to two thirds again.

### 2.3 Retrieval on Harbor's own 832-case corpus (EN/AR/mixed)

| Model | Recall@1 (en / ar / mixed) | NDCG@10 (en / ar / mixed) |
|---|---|---|
| EmbeddingGemma 2 | 1.000 / 1.000 / 1.000 | 1.000 / 1.000 / 1.000 |
| BGE-M3 | 0.976 / 0.963 / 0.936 | 0.990 / 0.984 / 0.973 |
| multilingual-e5-small | 1.000 / 0.909 / 0.954 | 1.000 / 0.965 / 0.983 |

All three clear all six behaviour thresholds (grounded citations 288/288, abstention
83/88, tool selection 144/144, numeric grounding 328/328, unauthorized effects 0).
Reading: EmbeddingGemma 2 is at the ceiling of a synthetic, templated corpus. That
proves it is **not worse** and handles Arabic and mixed text cleanly; it does not prove
a general margin over BGE-M3, and the corpus cannot show one.

### 2.4 The decision layer (new this session) - where a good embedder pays off
Choosing the KIND of action a request needs (run a skill, search documents, remember,
recall, schedule, read a scan, transcribe, chat), 96 labeled EN/AR/FR requests:

| Embedder | Raw description similarity top-1 | + examples, best match | **+ learned head top-1 / top-3** | Calibrated coverage @ precision |
|---|---|---|---|---|
| EmbeddingGemma 2 | 64.6 % | 67.7 % | **88.5 % / 100 %** | 93.8 % @ 90.0 % |
| BGE-M3 | 63.5 % | 69.8 % | 86.5 % / 96.9 % | 85.4 % @ 92.7 % |
| multilingual-e5-small | 57.3 % | 65.6 % | 88.5 % / 99.0 % | 86.5 % @ 96.4 % |

Two lessons. (1) **Embeddings capture what a request is about, not what it asks for**:
"remember my accountant is Laura" and "who is my accountant?" were confused until a tiny
on-device logistic-regression head, trained at prewarm from labeled examples, learned
the separating directions (+21 points). (2) EmbeddingGemma 2 and BGE-M3 end up within
two points of each other; the gain came from the head, not from the model. The head,
the bars and the examples live in Harbor and survive any change of embedder (a new
embedder retrains the head; with no calibration entry it ranks and never recommends).

### 2.5 What did NOT improve
- **Skill routing: no gain.** On 108 requests over 36 skills EmbeddingGemma 2 scores
  46.3 % coverage at 78.0 % precision against BGE-M3's 45.4 % at 79.6 % - a tie.
- Chat quality, reasoning and tool use are untouched: it is not a generative model.
- Per-passage speed (above).
- It is not "MediaPipe Decision Maker": there is no such runtime in Rust. What Harbor
  has is the same prewarm/evaluate/abstain contract served by the embedder, which is
  what the plan needed.

## 3. Published claims, for context (unverified here)
- MTEB Code **78.68**, +9.92 over EmbeddingGemma 1 (68.76) - the largest reported gain.
- MTEB Multilingual v2 task mean ~61.36 vs 61.15 for v1 (one source): essentially flat.
- Google claims the best sub-1B result on the Massive Audio Embedding Benchmark and
  that it beats some specialist models more than twice its size on image/video/audio.
- Quality at 256 dimensions drops little; 128 dimensions drops more for multimodal
  (Harbor's own measurement agrees: 128-d routing top-1 fell to 48 %, so it has no bars).
Sources: [model card](https://ai.google.dev/gemma/docs/embeddinggemma/model_card_2),
[Hugging Face blog on v1](https://huggingface.co/blog/embeddinggemma),
[v1 paper](https://arxiv.org/pdf/2509.20354), and secondary coverage
([vpsranking](https://vpsranking.com/news/ai/ai-2026-10-06-google-embeddinggemma-2/),
[letsdatascience](https://letsdatascience.com/news/deepmind-releases-embeddinggemma-2-for-edge-search-fbcc7c51)).

## 4. What it cost us
- A back-ported llama.cpp (the architecture landed upstream after the pinned release):
  a vendored `llama-cpp-sys-2` to maintain until a release catches up (decision 0013).
- Numerics hazards that return plausible but wrong vectors: the iOS simulator's GPU
  and the Adreno Vulkan path on a real Android phone. A canary catches them and falls
  back to the CPU; Vulkan stays opt-in (decisions 0015, 0016).
- The catalog pin still needs an operator-signed epoch.
- Multimodal results rest on small pools; no large multimodal benchmark exists in the repo.

## 5. How to reproduce
```text
cd core
# routing + retrieval + decision, per embedder (set the GGUF):
HARBOR_EMBED_MODEL_GGUF=../fixtures/models/embeddinggemma-2-Q8_0.gguf \
  cargo test -p harbor_core --test router_live    -- --ignored --nocapture
HARBOR_EMBED_MODEL_GGUF=... cargo test -p harbor_knowledge --test qualification_live -- --ignored --nocapture
HARBOR_EMBED_MODEL_GGUF=... cargo test -p harbor_core --test decision_live -- --ignored --nocapture
HARBOR_EMBED_MODEL_GGUF=... cargo test --release -p harbor_knowledge --test embed_bench -- --ignored --nocapture
cargo test -p harbor_ffi --test multimodal_flow     # image / audio retrieval
```

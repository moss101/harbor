# Decision 0015 — Multimodal retrieval, Matryoshka indexes, memory pressure, and the numerics canary

Date: 2026-10-09
Status: Accepted (implemented; live-tested on macOS Metal + CPU, Android arm64
emulator, iOS simulator). Catalog entries are STAGED, not signed.

## Problem

Decision 0013 made EmbeddingGemma 2 load but left five items open: image/audio
retrieval, Android/iOS verification, the OS memory-warning hook, Matryoshka
truncation, and the catalog pin.

## Decisions

1. **Multimodal = one joint forward pass, built by Harbor.** EmbeddingGemma 2
   embeds text, images and audio by running them through the same
   bidirectional transformer, mean-pooled, into one space. llama.cpp's stock
   multimodal helper decodes text and media in separate batches, which for a
   non-causal model means the pieces never attend to each other. So
   `harbor_inference::multimodal::MediaEmbedder` builds the whole input as
   embedding rows: text tokens become `token_embd[id] * sqrt(n_embd)` (read
   from the GGUF, F32/F16/BF16/Q8_0), image/audio chunks come from the mmproj
   projector (`mtmd`, projector types `gemma4v`/`gemma4a` already in the
   vendored snapshot) and are used unscaled, then one embedding batch is
   decoded. **Validity check:** text-only through this path equals the stock
   text path to cosine 1.00000.
2. **A second vendored crate, minimal and additive.** `third_party/llama-cpp-2`
   (0.1.156 + `LlamaBatch::new_embd/add_embd` + `MtmdContext::output_embeddings`;
   `HARBOR_PATCH.md`). The Rust wrapper keeps its context pointers private, and a
   joint embedding batch cannot be built through its public API.
3. **Media items are `media:<id>` sources in the same index.** Ordinary text
   search finds them; `op.start_search_media` searches by an image/audio query.
   Only the vector and a caption/title are stored, never the media. Because the
   vector comes from media that is not kept, an identity change (new model,
   truncation) cannot re-embed it from sealed text; the rebuild **drops** media
   sources and reports them (`dropped_media`) instead of silently turning them
   into caption-text vectors. `media:` ids are reserved against document ingest.
4. **Packaging.** The text package (`embeddinggemma-2`, 310 MB) and a
   multimodal package (`embeddinggemma-2-multimodal`, weights + `mmproj`,
   865 MB) are separate catalog entries; the media towers load lazily and are
   freed by `knowledge.release`. The media context is sized to the input
   (240 MiB for an image, 57 MiB text-sized) instead of the model's 8K ceiling
   (1.15 GiB) — found by watching the compute buffer, fixed before shipping.
5. **Matryoshka truncation is a provider wrapper**
   (`harbor_inference::mrl::TruncatedEmbedder`): every embed path (documents,
   queries, memory, router, media) goes through it, so dimensions cannot
   disagree. Truncation is part of the index identity (`…+mrl256`), so an
   index at another dimension rebuilds and never mixes. Quality cost over the
   106 routing cases (top-1): 768-d 60%, 512-d 58.5%, 256-d 56.6%, 128-d 48%
   (below the broken-embedder floor — no calibration entry, so the router
   never recommends at 128). 512 and 256 have their own calibration entries.
6. **Numerics canary (new, found by the iOS simulator).** The simulator's GPU
   loads the model and then embeds wrongly with no error (no bfloat, no
   simdgroup matmul — an Arabic question retrieved the wrong document). That
   is the model card's float16 warning made real, and a phone with a similar
   GPU limitation could hit it. After loading an embedding model, Harbor
   embeds four fixed sentences and checks finiteness and that the paraphrase
   outranks both unrelated sentences; on failure the model is pinned to the
   CPU backend (sticky across release/reload, also applied to the projector)
   and re-verified. `knowledge.open` reports `embedding_backend`
   (`gpu` | `cpu_fallback` | `cpu_unverified`).
7. **Memory pressure.** The app's `didHaveMemoryPressure` calls
   `knowledge.release`; index, vectors and router cache stay.
8. **Catalog.** The two entries are staged in `key_ceremony.rs` as epoch 5,
   pinned to the exact upstream revision and SHA-256. They are NOT signed: the
   catalog is signed by the operator-held root key, which this work did not
   use. The committed `signed_catalog.json` is still epoch 4.

## Evidence

| Platform | Backend | Result |
|---|---|---|
| macOS (Apple GPU) | Metal | text + Matryoshka + multimodal pass; top score 0.8341 |
| macOS | CPU only | pass; top score 0.8330 |
| Android arm64 emulator | CPU | text, Matryoshka, multimodal pass; 0.8328 |
| iOS simulator | GPU path fails canary → CPU | text, Matryoshka, multimodal pass; 0.8364 |

Multimodal retrieval (macOS): text queries retrieve the right image (leads of
0.14–0.21 cosine over the next image) and the right speech clip (0.83 vs 0.53).
The test media is synthetic (`fixtures/media`: a rendered invoice, landscape
and bar chart; two `say` speech clips) — three images and two clips prove the
plumbing and the cross-modal ordering, not retrieval quality on real photos.

## Not done

- **Video.** The model takes video as sampled frames, but there is no frame
  decoder in the core. Frames can be ingested as images today.
- **Physical devices.** Android was an arm64 emulator (CPU) and iOS the
  simulator. A real iPhone's Metal path will go through the same canary; its
  result is not known.
- **The catalog signature** (operator).
- **Media quality benchmark** on real-world images/audio (this needs a labeled
  corpus Harbor does not have).

# Harbor patch — llama-cpp-2 0.1.156 (decision 0015)

The exact crates.io `llama-cpp-2 0.1.156` source plus two small, additive
accessors needed to embed text, images and audio in ONE forward pass:

- `LlamaBatch::new_embd` / `LlamaBatch::add_embd` — a batch of raw input
  embeddings (`llama_batch_init(n, n_embd, …)`) instead of token ids.
- `MtmdContext::output_embeddings` — the embeddings left by `encode_chunk`.

Nothing existing is changed. Audit: diff against the registry source.
Retire when upstream exposes equivalents.

# Decision 0019 — Portable OCR (vision LLM), OS-embedded speech, the decision head, and a safer keystore

Date: 2026-10-10
Status: Accepted (implemented and tested). Follows 0017 and 0018.

## 1. OCR through a vision-language model (any platform that runs a GGUF)

`harbor_inference::ocr` is now an engine registry (`OcrEngine`): the platform's own
engine (Apple Vision, registered through `harbor_ocr_register_host`) and
`harbor_inference::vision_ocr::VisionOcr`, which reads text off an image with any
installed vision-language GGUF that ships an `mmproj`. The tool `ocr.extract` does not
care which answers; for an engine that only reads images it splits the PDF itself
(text-layer pages are used as they are; scanned pages are pulled straight out of the PDF
objects by `harbor_render::scan_images` - no renderer - and read).
`ocr.open {package_id}` installs the vision engine; `ocr.status` / `ocr.release`.

Measured on this Mac (`harbor_ffi/tests/vision_ocr_live.rs`, same fixtures as the Apple test):

| Engine | English scans (clean / rotated+noisy / JPEG / 5-invoice page) | Arabic | Time per page |
|---|---|---|---|
| Apple Vision | CER 0.0000 on all | 0.0000 | ~1 s |
| Qwen2.5-VL-3B Q4_K_M (+Q8 mmproj, 2.8 GB) | **0.0000 on all** | 0.086 | 17–90 s (CPU) |
| SmolVLM2-500M Q8_0 (+Q8 mmproj, 546 MB) | 0.009 / 0.026 / 0.026 / **0.047** | **16.2 (unusable)** | 2–6 s |

Findings that shape the policy:
- A language model reading a page can drop or invent a line while reporting high
  confidence (SmolVLM2 silently turned INV-1004 into a duplicate of INV-1005, conf 0.98;
  on Arabic it counted "1001 1002 ..." at conf 0.995). So **every page a language model
  read is listed in `check_pages` for a human look**, a counting/looping detector stops
  and zero-scores degenerate output, and OCR text is shown before it is used.
- Qwen2.5-VL on Metal produced NaN logits (all `!`). The engine refuses non-finite
  scores and `open_verified` runs a canary read, pins the package to the CPU, reloads and
  retries (`backend: "cpu_fallback"`), the same pattern as the embedding canary.
- Context must be sized in TOKENS (M-RoPE models use fewer positions than tokens) and an
  image chunk must fit one decode batch.
- Quality guidance: Apple Vision where it exists; Qwen2.5-VL-3B-class for a portable
  desktop engine; the 500M model is acceptable for printed English, never for Arabic or
  unattended use.

## 2. Speech through the operating system's own model

`native/apple/asr_host` wraps macOS/iOS 26 `SpeechTranscriber` (the OS's embedded
on-device model: no network, no third-party code, no authorization prompt, nothing to
download when the locale is installed). `harbor_inference::asr` mirrors the OCR bridge;
tool `speech.transcribe`; `tools.read` is the single app-facing door for the two
read-class tools; `speech.status` lists the locales usable now.
Measured (`voice_flow_live.rs`, synthetic `say` speech, 3 sentences): WER 0.000 on two;
the third differs only because the model writes digits ("1001", "$4200", "August 31st")
where the reference spells words - which is the form the invoice checks ground.
A spoken request routes exactly like typed text: "Review the supplier invoices and tell me
which payments are overdue." -> `supplier-invoice-review` at 0.797, calibrated, no abstain.
**Arabic is not covered by this OS model** (supported: en, fr, de, es, it, pt, ja, ko, zh,
hi and other Indian languages; no `ar`); an Arabic request is refused with the list of
usable locales, never approximated. Android's `SpeechRecognizer` is the equivalent and is
not implemented. Speech is text: it authorizes nothing.

## 3. The decision layer beyond skills, served by the embedder

`decision.choose` answers "what kind of action does this request need" (run a skill,
search documents, remember, recall, schedule, read a scan, transcribe, chat) or ranks a
caller's own options. Raw similarity to descriptions got ~65 % top-1; a learned head
(multinomial logistic regression over the installed embedder's vectors, trained at
prewarm from `intent_exemplars.json`, none of which is an evaluation case) reaches
88.5 % top-1 / 100 % top-3 with EmbeddingGemma 2 (86.5 % / 96.9 % BGE-M3, 88.5 % / 99.0 %
e5-small). Bars are per embedder AND per task (`decision_calibration.json`, precision >= 0.90
rule, in-sample). Custom option sets have no benchmark: ranked, never recommended.
Recommend-only, like the router. Reasoning, data and the head are model-independent; the
user's chat model is not involved.

## 4. Keystore: never mint a key over an unreadable one

`KeychainKeyStore` treated ANY read error as "first use" and generated a key; if an item
already existed but could not be read (a differently-signed build, a locked keychain) the
write collided and `harbor_core_open` failed with only "open failed", and a successful
write would have orphaned every record sealed under the old key. Now only
`errSecItemNotFound` creates a key; any other status is `StoreError::KeyUnavailable`
carrying the OSStatus, and `harbor_core_last_open_error()` (surfaced by the Dart binding)
puts the reason on screen. The original simulator failure was NOT reproduced on demand
(installing a re-signed build over the existing install opened fine; an old-code workspace
opens with the new code), so the cause of that one failure is a well-supported hypothesis,
not a proof.

## Not done
- The Runner does not yet register the Apple OCR/speech adapters (the exports exist:
  `harbor_ocr_register_host`, `harbor_asr_register_host`; wiring them needs the Swift
  files in the Xcode project). No capture/record UI; no Android speech or OCR adapter;
  no Windows/Linux speech engine. The vision-LLM OCR covers Android/Windows/Linux in
  principle (same runtime) but was only run on macOS.
- Arabic speech; Arabic OCR with a small model.

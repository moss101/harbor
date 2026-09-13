# Harbor — FAQ / Help

**Does Harbor need an account?**
No. Harbor has no account system and no sign-in.

**Does Harbor work offline?**
Yes. Chat, document Q&A, artifact editing, recalculation, and agent runs
all work with the network fully off. Harbor's default "Local Only" mode
makes zero network requests (independently verified in release evidence).

**When does Harbor ever touch the network?**
Only when you explicitly acquire a model package from Hugging Face. Each
download is individually authorized, hash-verified against a signed
catalog, and recorded in the in-app Network Log.

**Which models can I run?**
Model packages from the signed catalog are pre-qualified with pinned
hashes (see "About models" in the app). Other GGUF models on Hugging Face
can be acquired, and Harbor shows a Fit Score describing how well a model
fits your device before you download it — but off-catalog models carry no
support claim.

**Can I open my existing Word/Excel/PowerPoint files?**
Harbor supports creating and editing DOCX, XLSX, and PPTX files. Its
compatibility classifier preserves content it does not fully support
exactly as-is, or refuses the change explicitly — it never silently
rewrites your file. PDFs can be imported with text extraction and
citable page mapping.

**Where is my data stored?**
In the app's private on-device storage. Private workspace content is
encrypted at rest. Deleting a workspace deletes its data.

**Is there sync between devices?**
Not in this release. Sync is disabled and has no active code path. It
will only ship after its dedicated qualification (multi-device conflict,
revocation, snapshot restore) passes.

**Does Harbor send diagnostics/telemetry?**
No. There is no diagnostics upload in this release.

**Arabic support?**
The entire UI is bilingual English/Arabic with full right-to-left layout
and screen-reader support in both languages.

**Why does a model fail to load / run slowly?**
Model fit depends on device memory. Harbor's Fit Score warns before
download; if a loaded model exceeds comfortable memory, Harbor may
cancel generation with an explicit message rather than destabilize the
device.

**How do I report a security issue?**
See `security.md` in this directory / the security page in the app's
About screen.

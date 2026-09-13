# Harbor — Apple Export Compliance Analysis

**Declaration shipped in the iOS Info.plist:**
`ITSAppUsesNonExemptEncryption = false` (the app uses only encryption
categories exempt from French/annual self-classification reporting).

**Basis (code-level evidence):**

1. **No proprietary or non-standard cryptography.** Harbor's cryptography
   is limited to:
   - TLS for the brokered Hugging Face acquisition (rustls — standard
     TLS 1.2/1.3);
   - Authenticated encryption (AES-GCM, ChaCha20-Poly1305) via the
     RustCrypto `aes-gcm`/`chacha20poly1305` crates — standard algorithms;
   - X25519 key agreement, Ed25519 signatures, HKDF, SHA-2, BLAKE3 —
     standard, publicly specified primitives;
   - OS-platform crypto as used by the system libraries.
   All fall under the standard-algorithm exemptions (BIS 740.17(b)(1)
   category: limited cryptography/authentication uses employing standard
   implementations). No custom cipher design exists in the codebase.
2. **Purpose-bound usage.** Encryption is used for: local storage
   protection (user's own data, user-protection function), signature
   verification of the model catalog and sync protocol structures
   (authentication/integrity), and HTTPS transport. No content filtering,
   no child-safety-restricted cryptographic functionality.
3. **Not a medical/banking vertical application** requiring additional
   classification.

**Operator confirmation required before submission** (the machine cannot
answer legal questions): the operator signs the App Store Connect export
compliance question with this analysis; if counsel determines otherwise,
flip the plist key to `true` and complete the French declaration. This
document is the recorded rationale, not legal advice.

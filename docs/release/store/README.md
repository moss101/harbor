# Harbor — Store & Release Collateral

Final release materials for the Harbor v1.0.0 RC (goal §17). Every document
here is written against the qualified behavior of the build identified in
`evidence/releases/` — no capability may be described here that is not
backed by a PASS gate or explicitly labeled as disabled/pending.

| File | Purpose |
| --- | --- |
| `product_description.md` | Store listing description (EN + AR) |
| `privacy_statement.md` | Privacy statement (EN + AR) |
| `local_first_explained.md` | Local-first explanation for store review and users |
| `supported_devices.md` | Supported-device matrix |
| `model_compatibility.md` | What "works with a model" means (no overclaiming) |
| `release_notes.md` | RC release notes (EN + AR) |
| `changelog.md` | Changelog from initial public commit |
| `faq.md` | Help/FAQ |
| `security.md` | Security contact and disclosure policy |
| `model_licenses.md` | Model-license presentation |
| `apple_export_compliance.md` | Analysis behind `ITSAppUsesNonExemptEncryption=false` |

Wording rules (from the release goal):

- Never claim "works with every Hugging Face model" — Harbor qualifies
  catalog packages and scores arbitrary GGUFs via Fit Score; that is the
  honest statement.
- Never claim "all processing is always offline" — Harbor is local-first
  with explicit, authorized, brokered egress for model acquisition, and the
  network behavior is independently verified.
- Sync and all other optional capabilities are disabled at RC and must be
  described as such, not as shipping features.

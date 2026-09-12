# Decision 0003 — Light-theme status border tokens corrected for WCAG non-text contrast

Date: 2026-09-12
Status: Accepted.

## Problem
`tools/check_contrast.py` (unrounded WCAG math) found the LIGHT-theme status
badge borders failed WCAG 1.4.11 non-text contrast (>= 3:1) against their
adjacent backgrounds: ratios 1.95–2.20 vs canvas/surface. The DARK theme
passed (3.1–3.7). Status badges rely on the border as part of the
icon+label+color signal, so a barely-visible boundary is an accessibility
defect, not a style preference.

## Alternatives
1. Drop the border check as "informational" — rejected: weakens an
   accessibility invariant (authorities must not be silently weakened).
2. Remove borders entirely — rejected: reduces badge legibility on tinted
   fills and loses the boundary cue.
3. Darken the four light-theme border tokens until compliant (chosen).

## Change
06_Design_Tokens.json `semanticRoles.light` borders:
- statusLocal #89BFA6 -> #6D9884  (3.08 vs canvas, 3.24 vs surface)
- statusHybrid #D8AE69 -> #AC8B54 (3.03 / 3.19)
- statusRemote #AAA7F5 -> #8B88C8 (3.09 / 3.25)
- statusDanger #E6A39B -> #B8827C (3.05 / 3.22)
packages/harbor_ui/src/tokens.dart synced byte-for-byte.

## Evidence
`python3 tools/check_contrast.py --write-evidence` → 46/46 pairs pass
(evidence/contrast_audit.json). The check is wired into CI.

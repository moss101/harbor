#!/usr/bin/env python3
"""Harbor semantic color contrast audit (goal §17, 06_Design_Tokens.json).

Verifies WCAG contrast with UNROUNDED ratio math (the authority forbids
rounding): every allowed text/surface pair >= 4.5:1, status text/fill pairs
>= 4.5:1, primary-action pair >= 4.5:1, focus ring >= 3:1 against surface.

Usage: python3 tools/check_contrast.py [--write-evidence]
Exit 1 on any failure.
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOKENS = json.loads((ROOT / "06_Design_Tokens.json").read_text())


def hex_to_rgb(h: str) -> tuple:
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def rel_lum(rgb: tuple) -> float:
    def channel(c):
        c = c / 255.0
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    r, g, b = (channel(c) for c in rgb)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(a: str, b: str) -> float:
    la, lb = rel_lum(hex_to_rgb(a)), rel_lum(hex_to_rgb(b))
    lighter, darker = max(la, lb), min(la, lb)
    return (lighter + 0.05) / (darker + 0.05)


def audit() -> list[dict]:
    results = []
    normal = TOKENS["accessibility"]["normalTextMinContrast"]  # 4.5
    large = TOKENS["accessibility"]["largeTextMinContrast"]    # 3.0

    for theme in ("light", "dark"):
        c = TOKENS["color"][theme]
        roles = TOKENS["semanticRoles"][theme]

        # 1. Allowed text/surface pairs (ink, inkMuted on canvas family).
        for pair in TOKENS["accessibility"]["allowedTextSurfacePairs"]:
            text = c[pair["text"]]
            for surface in pair["surfaces"]:
                ratio = contrast(text, c[surface])
                results.append({
                    "theme": theme,
                    "kind": "text/surface",
                    "text": pair["text"],
                    "surface": surface,
                    "ratio": ratio,
                    "min": normal,
                    "pass": ratio >= normal,
                })

        # 2. Status badges: text on its own fill >= 4.5; the border marks
        #    the component edge, so WCAG 1.4.11 non-text contrast applies
        #    border vs the ADJACENT background (canvas + surface) >= 3:1.
        for name, role in roles.items():
            if name.startswith("status"):
                ratio = contrast(role["text"], role["fill"])
                results.append({
                    "theme": theme,
                    "kind": "status text/fill",
                    "text": name + ".text",
                    "surface": name + ".fill",
                    "ratio": ratio,
                    "min": normal,
                    "pass": ratio >= normal,
                })
                for surface in ("canvas", "surface"):
                    border_ratio = contrast(role["border"], c[surface])
                    results.append({
                        "theme": theme,
                        "kind": f"status border/{surface} (large)",
                        "text": name + ".border",
                        "surface": surface,
                        "ratio": border_ratio,
                        "min": large,
                        "pass": border_ratio >= large,
                    })

        # 3. Primary action: label on fill.
        pa = roles["primaryAction"]
        ratio = contrast(pa["text"], pa["fill"])
        results.append({
            "theme": theme,
            "kind": "primary action text/fill",
            "text": "primaryAction.text",
            "surface": "primaryAction.fill",
            "ratio": ratio,
            "min": normal,
            "pass": ratio >= normal,
        })

        # 4. Focus ring vs surface (non-text, 3:1) and vs canvas.
        for surface in ("surface", "canvas"):
            ratio = contrast(roles["focusRing"], c[surface])
            results.append({
                "theme": theme,
                "kind": "focus ring/surface (large)",
                "text": "focusRing",
                "surface": surface,
                "ratio": ratio,
                "min": large,
                "pass": ratio >= large,
            })

    return results


def main() -> None:
    results = audit()
    failures = [r for r in results if not r["pass"]]
    for r in results:
        mark = "PASS" if r["pass"] else "FAIL"
        print(f"[{mark}] {r['theme']:5} {r['kind']:28} "
              f"{r['text']} on {r['surface']}: "
              f"{r['ratio']:.3f} (min {r['min']})")
    print(f"\n{len(results) - len(failures)}/{len(results)} pairs pass "
          f"(unrounded WCAG ratios).")
    if "--write-evidence" in sys.argv:
        evidence_dir = ROOT / "evidence"
        evidence_dir.mkdir(exist_ok=True)
        out = evidence_dir / "contrast_audit.json"
        out.write_text(json.dumps({
            "tool": "tools/check_contrast.py",
            "tokens": "06_Design_Tokens.json (Harbor Current 2)",
            "math": "WCAG 2.x relative luminance, unrounded",
            "results": results,
            "failures": failures,
            "pass": not failures,
        }, indent=2))
        print(f"evidence written: {out}")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()

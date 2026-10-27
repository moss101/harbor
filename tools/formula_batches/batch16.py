"""Batch 16 — criteria semantics matrix across the *IF/*IFS family (extra cases for functions
that are already targets). Data mixes numbers, text, text-numbers, blanks and a boolean so
the blank / type rules of Excel's criteria matching are pinned down."""
from dsl import C

A = [(f"A{i + 1}", v) for i, v in enumerate([10, 20, 30, 40, 50, 60, 70, 80])]
# B: 1, 2, 3, "x", blank, blank, "3" (text), TRUE
B = [("B1", 1), ("B2", 2), ("B3", 3), ("B4", "x"), ("B7", "3"), ("B8", True)]
DATA = A + B
CRITS = ["<9", ">1", "<>3", "=3", "3", "", "=", "<>", "x", "*", "?", ">=2", "<=2"]
WHOLE = ["<9", "<>3", ""]


def q(c):
    return '"' + c + '"'


def fam(rng_b, rng_a, crits, tag):
    out = {
        "COUNTIF": [], "SUMIF": [], "AVERAGEIF": [], "COUNTIFS": [],
        "SUMIFS": [], "AVERAGEIFS": [], "MAXIFS": [], "MINIFS": [],
    }
    for c in crits:
        cq = q(c)
        note = f"criterion {cq} {tag}"
        out["COUNTIF"].append(C(f"=COUNTIF({rng_b},{cq})", DATA, note=note))
        out["SUMIF"].append(C(f"=SUMIF({rng_b},{cq},{rng_a})", DATA, note=note))
        out["AVERAGEIF"].append(C(f"=AVERAGEIF({rng_b},{cq},{rng_a})", DATA, note=note))
        out["COUNTIFS"].append(C(f'=COUNTIFS({rng_b},{cq},{rng_a},">0")', DATA, note=note))
        out["SUMIFS"].append(C(f"=SUMIFS({rng_a},{rng_b},{cq})", DATA, note=note))
        out["AVERAGEIFS"].append(C(f"=AVERAGEIFS({rng_a},{rng_b},{cq})", DATA, note=note))
        out["MAXIFS"].append(C(f"=MAXIFS({rng_a},{rng_b},{cq})", DATA, note=note))
        out["MINIFS"].append(C(f"=MINIFS({rng_a},{rng_b},{cq})", DATA, note=note))
    return out


bounded = fam("B1:B8", "A1:A8", CRITS, "over bounded ranges")
whole = fam("B:B", "A:A", WHOLE, "over whole columns")
BATCH = {
    "id": "16",
    "title": "Criteria semantics matrix for the IF/IFS family",
    "functions": [(fn, bounded[fn] + whole[fn]) for fn in bounded],
}

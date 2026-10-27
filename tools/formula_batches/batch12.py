"""Batch 12 — database functions over a list range, plus CELL, INDIRECT, OFFSET (15)."""
from dsl import C

HEAD = [("A1", "Tree"), ("B1", "Height"), ("C1", "Age"), ("D1", "Yield"), ("E1", "Profit")]
ROWS = [
    ("Apple", 18, 20, 14, 105),
    ("Pear", 12, 12, 10, 96),
    ("Cherry", 13, 14, 9, 105),
    ("Apple", 14, 15, 10, 75),
    ("Pear", 9, 8, 8, 76.8),
    ("Apple", 8, 9, 6, 45),
]
DB = list(HEAD)
for r, row in enumerate(ROWS, start=2):
    for c, v in enumerate(row):
        DB.append((f"{'ABCDE'[c]}{r}", v))
APPLE = DB + [("G1", "Tree"), ("G2", "Apple")]
TALL = DB + [("G1", "Height"), ("G2", ">10")]
NONE = DB + [("G1", "Tree"), ("G2", "Plum")]
RNG = "A1:E7"
CRIT = "G1:G2"


def d(fn, field, extra=None):
    cases = [
        C(f'={fn}({RNG},"{field}",{CRIT})', APPLE, note="criteria: Tree = Apple"),
        C(f'={fn}({RNG},"{field}",{CRIT})', TALL, note="criteria: Height > 10"),
    ]
    if extra:
        cases.append(extra)
    return (fn, cases)


BATCH = {
    "id": "12",
    "title": "Database functions, CELL, INDIRECT and OFFSET",
    "functions": [
        d("DAVERAGE", "Yield", C(f'=DAVERAGE({RNG},"Yield",{CRIT})', NONE, dim="errors", note="no record matches")),
        d("DCOUNT", "Age", C(f'=DCOUNT({RNG},"Age",{CRIT})', NONE, dim="blanks", note="no record matches: zero")),
        d("DCOUNTA", "Tree"),
        ("DGET", [C(f'=DGET({RNG},"Yield",{CRIT})', DB + [("G1", "Tree"), ("G2", "Cherry")], note="exactly one record"), C(f'=DGET({RNG},"Yield",{CRIT})', APPLE, dim="errors", note="more than one record matches"), C(f'=DGET({RNG},"Yield",{CRIT})', NONE, dim="errors", note="no record matches")]),
        d("DMAX", "Profit"),
        d("DMIN", "Profit"),
        d("DPRODUCT", "Yield"),
        d("DSTDEV", "Yield"),
        d("DSTDEVP", "Yield"),
        d("DSUM", "Profit", C(f"=DSUM({RNG},4,{CRIT})", APPLE, note="field given by column number")),
        d("DVAR", "Yield"),
        d("DVARP", "Yield"),
        ("CELL", [C('=CELL("address",B3)'), C('=CELL("col",B3)'), C('=CELL("row",B3)'), C('=CELL("contents",A1)', [("A1", 42)]), C('=CELL("type",A1)', [("A1", "x")], note="l for a label"), C('=CELL("type",A1)', [("A1", 5)], note="v for a value"), C('=CELL("type",A1)', dim="blanks", note="b for a blank")]),
        ("INDIRECT", [C('=INDIRECT("A2")', [("A1", 10), ("A2", 20)]), C('=INDIRECT("B"&A1)', [("A1", 2), ("B2", 99)], note="address built from a cell"), C('=SUM(INDIRECT("A1:A3"))', [("A1", 1), ("A2", 2), ("A3", 3)]), C('=INDIRECT("not a ref")', dim="errors")]),
        ("OFFSET", [C("=OFFSET(A1,2,1)", [("A1", 1), ("B3", 77)], note="two rows down, one column right"), C("=SUM(OFFSET(A1,0,0,3,1))", [("A1", 1), ("A2", 2), ("A3", 3), ("A4", 4)], note="a three-row window"), C("=OFFSET(A1,-1,0)", dim="errors", note="above the sheet")]),
    ],
}

"""Batch 13 — dynamic-array functions (values via INDEX/ROWS/COLUMNS/SUM; spill itself is not
claimed) and the byte-oriented text functions (22)."""
from dsl import C

GRID = [("A1", 1), ("B1", 2), ("C1", 3), ("A2", 4), ("B2", 5), ("C2", 6)]
COL = [(f"A{i + 1}", v) for i, v in enumerate([5, 3, 8, 1, 3])]
KEYS = [(f"B{i + 1}", v) for i, v in enumerate([2, 1, 3, 5, 4])]
PAIR = [("A1", 1), ("B1", 2), ("A2", 3), ("B2", 4)]
DUP = [(f"A{i + 1}", v) for i, v in enumerate(["a", "b", "a", "c", "b", "a"])]
TXT = [("A1", "Hello World")]

BATCH = {
    "id": "13",
    "title": "Dynamic-array functions (values) and byte-oriented text functions",
    "functions": [
        ("CHOOSECOLS", [C("=INDEX(CHOOSECOLS(A1:C2,3,1),2,2)", GRID, note="columns in the requested order"), C("=COLUMNS(CHOOSECOLS(A1:C2,2,3))", GRID), C("=CHOOSECOLS(A1:C2,5)", GRID, dim="errors")]),
        ("CHOOSEROWS", [C("=INDEX(CHOOSEROWS(A1:C2,2),1,3)", GRID), C("=ROWS(CHOOSEROWS(A1:C2,2,1,2))", GRID), C("=CHOOSEROWS(A1:C2,5)", GRID, dim="errors")]),
        ("DROP", [C("=INDEX(DROP(A1:C2,1),1,1)", GRID), C("=INDEX(DROP(A1:C2,0,1),1,1)", GRID, note="drop the first column"), C("=DROP(A1:C2,2)", GRID, dim="errors", note="nothing left")]),
        ("FILTER", [C("=INDEX(FILTER(A1:A5,A1:A5>3),1)", COL), C("=ROWS(FILTER(A1:A5,A1:A5>3))", COL), C("=FILTER(A1:A5,A1:A5>99)", COL, dim="errors", note="no match"), C('=FILTER(A1:A5,A1:A5>99,"none")', COL, note="if_empty value")]),
        ("HSTACK", [C("=INDEX(HSTACK(A1:A2,B1:B2),2,2)", PAIR), C("=COLUMNS(HSTACK(A1:A2,B1:B2,A1:A2))", PAIR)]),
        ("SEQUENCE", [C("=INDEX(SEQUENCE(3,2),3,2)"), C("=SUM(SEQUENCE(4))"), C("=INDEX(SEQUENCE(2,2,5,2),2,1)", note="start 5, step 2"), C("=SEQUENCE(0)", dim="errors")]),
        ("SORT", [C("=INDEX(SORT(A1:A5),1)", COL), C("=INDEX(SORT(A1:A5,1,-1),1)", COL, note="descending"), C("=INDEX(SORT(A1:A5),5)", COL)]),
        ("SORTBY", [C("=INDEX(SORTBY(A1:A5,B1:B5),1)", COL + KEYS), C("=INDEX(SORTBY(A1:A5,B1:B5,-1),1)", COL + KEYS, note="descending by key")]),
        ("TAKE", [C("=INDEX(TAKE(A1:A5,2),2)", COL), C("=INDEX(TAKE(A1:A5,-2),1)", COL, note="the last two rows"), C("=ROWS(TAKE(A1:A5,3))", COL)]),
        ("TOCOL", [C("=ROWS(TOCOL(A1:B2))", PAIR), C("=INDEX(TOCOL(A1:B2),3)", PAIR, note="reads across rows first")]),
        ("TOROW", [C("=COLUMNS(TOROW(A1:B2))", PAIR), C("=INDEX(TOROW(A1:B2),1,4)", PAIR)]),
        ("TRANSPOSE", [C("=INDEX(TRANSPOSE(A1:C2),3,1)", GRID), C("=ROWS(TRANSPOSE(A1:C2))", GRID)]),
        ("UNIQUE", [C("=ROWS(UNIQUE(A1:A6))", DUP), C("=INDEX(UNIQUE(A1:A6),3)", DUP), C("=ROWS(UNIQUE(A1:A6,FALSE,TRUE))", DUP, note="values that occur exactly once")]),
        ("VSTACK", [C("=INDEX(VSTACK(A1:B1,A2:B2),2,1)", PAIR), C("=ROWS(VSTACK(A1:B1,A2:B2,A1:B1))", PAIR)]),
        ("TEXTSPLIT", [C('=INDEX(TEXTSPLIT("a,b,c",","),1,2)'), C('=COLUMNS(TEXTSPLIT("a,b,c",","))'), C('=INDEX(TEXTSPLIT("a-b;c-d",{"-"},";"),2,1)', note="rows and columns")]),
        ("FINDB", [C('=FINDB("o",A1)', TXT), C('=FINDB("o",A1,6)', TXT, note="start position"), C('=FINDB("z",A1)', TXT, dim="errors")]),
        ("LEFTB", [C("=LEFTB(A1,5)", TXT), C("=LEFTB(A1)", TXT, note="one byte by default")]),
        ("LENB", [C("=LENB(A1)", TXT), C('=LENB("")')]),
        ("MIDB", [C("=MIDB(A1,7,5)", TXT), C("=MIDB(A1,1,3)", TXT), C("=MIDB(A1,0,3)", TXT, dim="errors")]),
        ("REPLACEB", [C('=REPLACEB(A1,1,5,"Howdy")', TXT), C('=REPLACEB(A1,7,0,"big ")', TXT, note="insert")]),
        ("RIGHTB", [C("=RIGHTB(A1,5)", TXT), C("=RIGHTB(A1)", TXT)]),
        ("SEARCHB", [C('=SEARCHB("wor",A1)', TXT, note="case-insensitive"), C('=SEARCHB("o?l",A1)', TXT, note="wildcard"), C('=SEARCHB("z",A1)', TXT, dim="errors")]),
    ],
}

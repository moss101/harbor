"""Batch 17 — criteria edge cases (extra cases for COUNTIF/SUMIF/COUNTIFS): blank vs 0,
case-insensitivity, boolean criteria, escaped wildcards, a criteria cell that is blank."""
from dsl import C

A = [(f"A{i + 1}", i + 1) for i in range(8)]
# 0, blank, empty text, "abc", "ABC", TRUE, "a*b", -1
B = [("B1", 0), ("B3", ""), ("B4", "abc"), ("B5", "ABC"), ("B6", True), ("B7", "a*b"), ("B8", -1)]
DATA = A + B

cases = [
    ("=COUNTIF(B1:B8,0)", "a blank is not the number 0"),
    ('=COUNTIF(B1:B8,"=0")', ""),
    ('=COUNTIF(B1:B8,"<>0")', "everything but the numeric 0"),
    ('=COUNTIF(B1:B8,"<0")', ""),
    ('=COUNTIF(B1:B8,"abc")', "case-insensitive"),
    ("=COUNTIF(B1:B8,TRUE)", "a boolean criterion matches booleans"),
    ('=COUNTIF(B1:B8,"a~*b")', "escaped wildcard"),
    ('=COUNTIF(B1:B8,"a*b")', ""),
    ('=COUNTIF(B1:B8,"")', "blank and empty text"),
    ("=COUNTIF(B1:B8,C1)", "criteria taken from a blank cell"),
]
funcs = [("COUNTIF", [C(f, DATA, note=n) for f, n in cases])]
funcs.append(("SUMIF", [
    C('=SUMIF(B1:B8,"=0",A1:A8)', DATA, note="sums only where the criteria cell is the number 0"),
    C('=SUMIF(B1:B8,"<>0",A1:A8)', DATA),
    C("=SUMIF(B1:B8,C1,A1:A8)", DATA, note="criteria taken from a blank cell"),
    C('=SUMIF(B1:B8,"abc",A1:A8)', DATA),
]))
funcs.append(("COUNTIFS", [
    C('=COUNTIFS(B1:B8,"=0",A1:A8,">0")', DATA),
    C('=COUNTIFS(B1:B8,"<>0",A1:A8,">0")', DATA),
    C("=COUNTIFS(B1:B8,C1,A1:A8,\">0\")", DATA, note="criteria taken from a blank cell"),
]))
BATCH = {"id": "17", "title": "Criteria edge cases", "functions": funcs}

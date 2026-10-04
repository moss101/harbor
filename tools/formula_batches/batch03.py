"""Batch 03 — information, logical and reference helpers (24)."""
from dsl import C

NUMS = [("A1", 1), ("A2", 2), ("A3", 3), ("A4", 4), ("A5", 5)]
LETTERS = [("B1", "a"), ("B2", "b"), ("B3", "c"), ("B4", "d"), ("B5", "e")]
DATA = NUMS + LETTERS

BATCH = {
    "id": "03",
    "title": "Information, logical and reference helpers",
    "functions": [
        ("ERROR.TYPE", [C("=ERROR.TYPE(1/0)"), C("=ERROR.TYPE(NA())"), C("=ERROR.TYPE(A1)", [("A1", "#VALUE!")], note="an error held in a cell"), C("=ERROR.TYPE(5)", dim="errors", note="a non-error argument is #N/A")]),
        ("ISERR", [C("=ISERR(1/0)"), C("=ISERR(NA())", note="ISERR is FALSE for #N/A"), C("=ISERR(5)")]),
        ("ISERROR", [C("=ISERROR(NA())"), C("=ISERROR(1)"), C("=ISERROR(A1)", [("A1", "#REF!")])]),
        ("ISEVEN", [C("=ISEVEN(4)"), C("=ISEVEN(2.9)", note="the argument is truncated"), C("=ISEVEN(3)"), C('=ISEVEN("x")', dim="errors")]),
        ("ISFORMULA", [C("=ISFORMULA(A1)", [("A1", 5)], note="a constant is not a formula"), C("=ISFORMULA(Z9)", dim="blanks")]),
        ("ISLOGICAL", [C("=ISLOGICAL(TRUE)"), C('=ISLOGICAL("TRUE")', note="text that looks like a boolean is text"), C("=ISLOGICAL(A1)", dim="blanks")]),
        ("ISNONTEXT", [C("=ISNONTEXT(5)"), C('=ISNONTEXT("a")'), C("=ISNONTEXT(A1)", dim="blanks", note="a blank cell is not text")]),
        ("ISREF", [C("=ISREF(A1)"), C("=ISREF(5)"), C("=ISREF(A1:B2)")]),
        ("N", [C("=N(5)"), C('=N("a")'), C("=N(TRUE)"), C("=N(A1)", [("A1", "#N/A")], dim="errors", note="an error passes through")]),
        ("NA", [C("=NA()", dim="errors"), C("=IFERROR(NA(),7)"), C("=ISNA(NA())")]),
        ("T", [C('=T("abc")'), C("=T(5)", note="a number gives empty text"), C("=T(A1)", [("A1", "hello")])]),
        ("TYPE", [C("=TYPE(1)"), C('=TYPE("a")'), C("=TYPE(TRUE)"), C("=TYPE(NA())", note="error values are type 16")]),
        ("TRUE", [C("=TRUE()"), C("=AND(TRUE(),FALSE())"), C("=IF(TRUE(),1,2)")]),
        ("FALSE", [C("=FALSE()"), C("=OR(TRUE(),FALSE())"), C("=IF(FALSE(),1,2)")]),
        ("XOR", [C("=XOR(TRUE,FALSE)"), C("=XOR(TRUE,TRUE)"), C("=XOR(1,1,1)", note="an odd number of TRUE values"), C('=XOR("x")', dim="errors")]),
        ("ADDRESS", [C("=ADDRESS(2,3)"), C("=ADDRESS(2,3,4)", note="relative reference"), C("=ADDRESS(2,3,1,FALSE)", note="R1C1 style"), C('=ADDRESS(1,1,1,TRUE,"Data")'), C("=ADDRESS(0,1)", dim="errors")]),
        ("COLUMN", [C("=COLUMN(C5)"), C("=COLUMN()", note="the formula sits in column 1000 in every fixture")]),
        ("COLUMNS", [C("=COLUMNS(A1:C3)"), C("=COLUMNS(A1)"), C("=COLUMNS(A1:A10)")]),
        ("ROW", [C("=ROW(C5)"), C("=ROW()", note="the formula sits in row 1000 in every fixture")]),
        ("ROWS", [C("=ROWS(A1:C3)"), C("=ROWS(A1)"), C("=ROWS(A1:A10)")]),
        ("LOOKUP", [C("=LOOKUP(3,A1:A5,B1:B5)", DATA), C("=LOOKUP(3.5,A1:A5,B1:B5)", DATA, note="approximate: the largest value not above the key"), C("=LOOKUP(0,A1:A5,B1:B5)", DATA, dim="errors", note="smaller than every entry")]),
        ("XMATCH", [C("=XMATCH(3,A1:A5)", NUMS), C("=XMATCH(3.5,A1:A5,-1)", NUMS, note="next smaller item"), C("=XMATCH(3,A1:A5,0,-1)", NUMS + [("A6", 3)], note="search from the end"), C("=XMATCH(9,A1:A5)", NUMS, dim="errors")]),
        ("FORMULATEXT", [C("=FORMULATEXT(A1)", [("A1", 5)], dim="errors", note="a constant has no formula text")]),
        ("HYPERLINK", [C('=HYPERLINK("https://example.com","Example")'), C('=HYPERLINK("https://example.com")', note="without a friendly name the link text is shown")]),
    ],
}

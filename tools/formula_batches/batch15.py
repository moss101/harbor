"""Batch 15 — Excel-checked regression cases for the engine fixes (extra cases for
functions that are already targets): criteria-range size rules, whole-column
references, CHISQ.TEST degrees of freedom."""
from dsl import C

VAL = [(f"A{i + 1}", v) for i, v in enumerate([1, 2, 3, 4, 5])]
KEY = [(f"B{i + 1}", v) for i, v in enumerate([1, 2, 3, 4, 5])]
KEY3 = [("B1", 1), ("B2", 2), ("B3", 3)]
HOLES = VAL + [("B1", 1), ("B2", 2), ("B3", 3)]  # B4:B5 genuinely blank
ROW = [("A1", 18), ("B1", 22), ("C1", 20), ("D1", 20)]

BATCH = {
    "id": "15",
    "title": "Regression cases: criteria-range sizes, whole-column references, CHISQ.TEST",
    "functions": [
        ("SUMIFS", [
            C('=SUMIFS(A1:A5,B1:B5,">2")', VAL + KEY),
            C('=SUMIFS(A1:A5,B1:B3,">2")', VAL + KEY3, dim="errors", note="bounded ranges of different sizes"),
            C('=SUMIFS(A:A,B:B,">2")', VAL + KEY, note="whole columns"),
            C('=SUMIFS(A:A,B:B,">1")', VAL + KEY3, note="whole columns whose used regions differ"),
        ]),
        ("COUNTIFS", [
            C('=COUNTIFS(A1:A5,">1",B1:B5,"<5")', VAL + KEY),
            C('=COUNTIFS(A1:A5,">1",B1:B3,"<9")', VAL + KEY3, dim="errors", note="bounded ranges of different sizes"),
            C('=COUNTIFS(A:A,">1",B:B,"<9")', VAL + KEY3, note="whole columns whose used regions differ"),
        ]),
        ("AVERAGEIFS", [
            C('=AVERAGEIFS(A1:A5,B1:B5,">2")', VAL + KEY),
            C('=AVERAGEIFS(A1:A5,B1:B3,">2")', VAL + KEY3, dim="errors", note="bounded ranges of different sizes"),
            C('=AVERAGEIFS(A:A,B:B,">1")', VAL + KEY3, note="whole columns whose used regions differ"),
        ]),
        ("MAXIFS", [
            C('=MAXIFS(A1:A5,B1:B5,"<4")', VAL + KEY),
            C('=MAXIFS(A1:A5,B1:B3,"<4")', VAL + KEY3, dim="errors", note="bounded ranges of different sizes"),
            C('=MAXIFS(A:A,B:B,"<4")', VAL + KEY3, note="whole columns whose used regions differ"),
        ]),
        ("COUNTIF", [
            C('=COUNTIF(B1:B5,"<9")', HOLES, note="blank cells never satisfy a numeric comparison"),
            C('=COUNTIF(B1:B5,"<>3")', HOLES, note="but blanks do satisfy not-equal"),
            C('=COUNTIF(B1:B5,"")', HOLES, note="and the empty-string criterion"),
        ]),
        ("SUMIF", [C('=SUMIF(B1:B5,"<9",A1:A5)', HOLES, note="blank criteria cells are skipped")]),
        ("COUNTIFS", [
            C('=COUNTIFS(A1:A5,">1",B1:B5,"<9")', HOLES, note="blanks in the second criteria range"),
            C('=COUNTIFS(A1:A5,">1",B1:B5,"<>3")', HOLES),
        ]),
        ("MAXIFS", [C('=MAXIFS(A1:A5,B1:B5,"<4")', HOLES, note="blank criteria cells never match <4")]),
        ("MINIFS", [
            C('=MINIFS(A:A,B:B,">1")', VAL + KEY3, note="whole columns whose used regions differ"),
        ]),
        ("CHISQ.TEST", [
            C("=CHISQ.TEST(A1:B1,C1:D1)", ROW, note="a single row keeps n-1 degrees of freedom"),
        ]),
    ],
}

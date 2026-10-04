"""Batch 06 — probability distributions part 2 and hypothesis tests (25)."""
from dsl import C

ACTUAL = [("A1", 58), ("A2", 11), ("A3", 10), ("B1", 35), ("B2", 25), ("B3", 23)]
EXPECTED = [("C1", 45.35), ("C2", 17.56), ("C3", 16.09), ("D1", 47.65), ("D2", 18.44), ("D3", 16.91)]
TWO = [(f"A{i + 1}", v) for i, v in enumerate([6, 7, 9, 15, 21])] + [(f"B{i + 1}", v) for i, v in enumerate([20, 28, 31, 38, 40])]
PAIRS_A = [3, 4, 5, 8, 9, 1, 2, 4, 5]
PAIRS_B = [6, 19, 3, 2, 14, 4, 5, 17, 1]
PAIRS = [(f"A{i + 1}", v) for i, v in enumerate(PAIRS_A)] + [(f"B{i + 1}", v) for i, v in enumerate(PAIRS_B)]
RANKS = [(f"A{i + 1}", v) for i, v in enumerate([13, 12, 11, 8, 4, 3, 2, 1, 1, 1])]
ZDATA = [(f"A{i + 1}", v) for i, v in enumerate([3, 6, 7, 8, 6, 5, 4, 2, 1, 9])]
FIVE = [(f"A{i + 1}", v) for i, v in enumerate([1, 2, 3, 4, 5])]

BATCH = {
    "id": "06",
    "title": "Probability distributions part 2, hypothesis tests and order statistics",
    "functions": [
        ("BETA.DIST", [C("=BETA.DIST(2,8,10,TRUE,1,3)"), C("=BETA.DIST(2,8,10,FALSE,1,3)"), C("=BETA.DIST(2,0,10,TRUE,1,3)", dim="errors")]),
        ("BETA.INV", [C("=BETA.INV(0.685470581,8,10,1,3)"), C("=BETA.INV(0.5,2,2)"), C("=BETA.INV(1.5,8,10)", dim="errors")]),
        ("CHISQ.DIST", [C("=CHISQ.DIST(0.5,1,TRUE)"), C("=CHISQ.DIST(2,3,FALSE)"), C("=CHISQ.DIST(-1,3,TRUE)", dim="errors")]),
        ("CHISQ.DIST.RT", [C("=CHISQ.DIST.RT(18.307,10)"), C("=CHISQ.DIST.RT(2,3)"), C("=CHISQ.DIST.RT(2,0)", dim="errors")]),
        ("CHISQ.INV", [C("=CHISQ.INV(0.93,1)"), C("=CHISQ.INV(0.5,4)"), C("=CHISQ.INV(1.5,4)", dim="errors")]),
        ("CHISQ.INV.RT", [C("=CHISQ.INV.RT(0.050001,10)"), C("=CHISQ.INV.RT(0.5,4)"), C("=CHISQ.INV.RT(1.5,4)", dim="errors")]),
        ("CHISQ.TEST", [C("=CHISQ.TEST(A1:B3,C1:D3)", ACTUAL + EXPECTED), C("=CHISQ.TEST(A1:B3,A1:B3)", ACTUAL, note="identical tables"), C("=CHISQ.TEST(A1:B3,C1:D2)", ACTUAL + EXPECTED, dim="errors")]),
        ("F.DIST", [C("=F.DIST(15.2069,6,4,TRUE)"), C("=F.DIST(15.2069,6,4,FALSE)"), C("=F.DIST(-1,6,4,TRUE)", dim="errors")]),
        ("F.DIST.RT", [C("=F.DIST.RT(15.2069,6,4)"), C("=F.DIST.RT(2,5,5)"), C("=F.DIST.RT(-1,6,4)", dim="errors")]),
        ("F.INV", [C("=F.INV(0.01,6,4)"), C("=F.INV(0.5,5,5)"), C("=F.INV(1.5,6,4)", dim="errors")]),
        ("F.INV.RT", [C("=F.INV.RT(0.01,6,4)"), C("=F.INV.RT(0.5,5,5)"), C("=F.INV.RT(1.5,6,4)", dim="errors")]),
        ("F.TEST", [C("=F.TEST(A1:A5,B1:B5)", TWO), C("=F.TEST(B1:B5,A1:A5)", TWO), C("=F.TEST(A1,B1)", [("A1", 3), ("B1", 4)], dim="errors", note="one value per array")]),
        ("T.DIST", [C("=T.DIST(60,1,TRUE)"), C("=T.DIST(8,3,FALSE)"), C("=T.DIST(1,0,TRUE)", dim="errors")]),
        ("T.DIST.2T", [C("=T.DIST.2T(1.959999998,60)"), C("=T.DIST.2T(0.5,10)"), C("=T.DIST.2T(-1,60)", dim="errors")]),
        ("T.DIST.RT", [C("=T.DIST.RT(1.959999998,60)"), C("=T.DIST.RT(-1,10)"), C("=T.DIST.RT(1,0)", dim="errors")]),
        ("T.INV", [C("=T.INV(0.75,2)"), C("=T.INV(0.05,10)"), C("=T.INV(1.5,10)", dim="errors")]),
        ("T.INV.2T", [C("=T.INV.2T(0.546449,60)"), C("=T.INV.2T(0.05,10)"), C("=T.INV.2T(1.5,10)", dim="errors")]),
        ("T.TEST", [C("=T.TEST(A1:A9,B1:B9,2,1)", PAIRS), C("=T.TEST(A1:A9,B1:B9,1,2)", PAIRS), C("=T.TEST(A1:A9,B1:B9,3,1)", PAIRS, dim="errors")]),
        ("Z.TEST", [C("=Z.TEST(A1:A10,4)", ZDATA), C("=Z.TEST(A1:A10,6,3)", ZDATA), C("=Z.TEST(A1:A3,4)", dim="errors", note="no data")]),
        ("CONFIDENCE.NORM", [C("=CONFIDENCE.NORM(0.05,2.5,50)"), C("=CONFIDENCE.NORM(0.1,1,100)"), C("=CONFIDENCE.NORM(1.5,2.5,50)", dim="errors")]),
        ("CONFIDENCE.T", [C("=CONFIDENCE.T(0.05,1,50)"), C("=CONFIDENCE.T(0.1,2,20)"), C("=CONFIDENCE.T(0.05,1,1)", dim="errors", note="needs more than one observation")]),
        ("PERCENTILE.EXC", [C("=PERCENTILE.EXC(A1:A5,0.25)", FIVE), C("=PERCENTILE.EXC(A1:A5,0.5)", FIVE), C("=PERCENTILE.EXC(A1:A5,0.9)", FIVE, dim="errors", note="k outside the exclusive range")]),
        ("PERCENTRANK.EXC", [C("=PERCENTRANK.EXC(A1:A10,8)", RANKS), C("=PERCENTRANK.EXC(A1:A10,5)", RANKS, note="interpolated between neighbours"), C("=PERCENTRANK.EXC(A1:A10,100)", RANKS, dim="errors")]),
        ("PERCENTRANK.INC", [C("=PERCENTRANK.INC(A1:A10,2)", RANKS), C("=PERCENTRANK.INC(A1:A10,8)", RANKS), C("=PERCENTRANK.INC(A1:A10,5)", RANKS, note="interpolated between neighbours"), C("=PERCENTRANK.INC(A1:A10,100)", RANKS, dim="errors")]),
        ("QUARTILE.EXC", [C("=QUARTILE.EXC(A1:A5,1)", FIVE), C("=QUARTILE.EXC(A1:A5,2)", FIVE), C("=QUARTILE.EXC(A1:A5,0)", FIVE, dim="errors")]),
    ],
}

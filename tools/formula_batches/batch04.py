"""Batch 04 — descriptive statistics, correlation and regression (25)."""
from dsl import C

X = [("B1", 1), ("B2", 2), ("B3", 3), ("B4", 4), ("B5", 5)]
Y = [("A1", 2), ("A2", 4), ("A3", 5), ("A4", 4), ("A5", 5)]
XY = X + Y
D = [(f"A{i + 1}", v) for i, v in enumerate([3, 4, 5, 2, 3, 4, 5, 6, 4, 7])]
MIXED = [("A1", 2), ("A2", "x"), ("A3", True), ("A4", 4)]
FLAT = [("B1", 3), ("B2", 3), ("B3", 3), ("B4", 3), ("B5", 3)]

BATCH = {
    "id": "04",
    "title": "Descriptive statistics, correlation and regression",
    "functions": [
        ("AVEDEV", [C("=AVEDEV(A1:A5)", Y), C("=AVEDEV(1,2,3,10)"), C('=AVEDEV("x")', dim="errors")]),
        ("AVERAGEA", [C("=AVERAGEA(A1:A4)", MIXED, note="text counts as 0 and TRUE as 1 inside a range"), C("=AVERAGEA(A1:A5)", Y), C("=AVERAGEA(A1:A3)", dim="errors", note="an empty range has no values to average")]),
        ("DEVSQ", [C("=DEVSQ(A1:A5)", Y), C("=DEVSQ(1,2,3)"), C("=DEVSQ(A1:A3)", dim="errors", note="no numbers")]),
        ("GEOMEAN", [C("=GEOMEAN(2,8)"), C("=GEOMEAN(A1:A5)", Y), C("=GEOMEAN(0,1)", dim="errors")]),
        ("HARMEAN", [C("=HARMEAN(1,2,4)"), C("=HARMEAN(A1:A5)", Y), C("=HARMEAN(-1,2)", dim="errors")]),
        ("KURT", [C("=KURT(A1:A10)", D), C("=KURT(1,2,3,10)"), C("=KURT(1,2,3)", dim="errors", note="needs at least four values")]),
        ("SKEW", [C("=SKEW(A1:A10)", D), C("=SKEW(1,2,10)"), C("=SKEW(1,2)", dim="errors")]),
        ("SKEW.P", [C("=SKEW.P(A1:A10)", D), C("=SKEW.P(1,2,10)"), C("=SKEW.P(5)", dim="errors")]),
        ("MAXA", [C("=MAXA(A1:A4)", MIXED), C("=MAXA(A1:A3)", [("A1", -5), ("A2", "x"), ("A3", True)], note="text is 0, TRUE is 1"), C("=MAXA(-5,TRUE)")]),
        ("MINA", [C("=MINA(A1:A3)", [("A1", 5), ("A2", "x"), ("A3", 7)], note="text counts as 0"), C("=MINA(A1:A5)", Y), C("=MINA(3,TRUE)")]),
        ("MINIFS", [C('=MINIFS(A1:A5,B1:B5,">2")', XY), C('=MINIFS(A1:A5,B1:B5,"<=2")', XY), C('=MINIFS(A1:A5,B1:B5,">9")', XY, note="no row matches: 0"), C('=MINIFS(A1:A5,B1:B4,">2")', XY, dim="errors", note="criteria range of a different size")]),
        ("STDEVA", [C("=STDEVA(A1:A4)", MIXED), C("=STDEVA(A1:A5)", Y), C("=STDEVA(5)", dim="errors")]),
        ("STDEVPA", [C("=STDEVPA(A1:A4)", MIXED), C("=STDEVPA(A1:A5)", Y), C("=STDEVPA(5)", note="one value has no spread")]),
        ("VARA", [C("=VARA(A1:A4)", MIXED), C("=VARA(A1:A5)", Y), C("=VARA(5)", dim="errors")]),
        ("VARPA", [C("=VARPA(A1:A4)", MIXED), C("=VARPA(A1:A5)", Y), C("=VARPA(5)")]),
        ("MODE.SNGL", [C("=MODE.SNGL(A1:A10)", D), C("=MODE.SNGL(1,2,2,3,3,3)"), C("=MODE.SNGL(1,2,3)", dim="errors", note="no repeated value")]),
        ("TRIMMEAN", [C("=TRIMMEAN(A1:A10,0.2)", D), C("=TRIMMEAN(A1:A10,0)", D, note="nothing trimmed"), C("=TRIMMEAN(A1:A10,1)", D, dim="errors")]),
        ("CORREL", [C("=CORREL(A1:A5,B1:B5)", XY), C("=CORREL(B1:B5,A1:A5)", XY, note="symmetric"), C("=CORREL(A1:A5,B1:B4)", XY, dim="errors", note="arrays of different length")]),
        ("PEARSON", [C("=PEARSON(A1:A5,B1:B5)", XY), C("=PEARSON(B1:B5,B1:B5)", XY, note="a series with itself"), C("=PEARSON(A1:A5,B1:B4)", XY, dim="errors")]),
        ("COVARIANCE.P", [C("=COVARIANCE.P(A1:A5,B1:B5)", XY), C("=COVARIANCE.P(B1:B5,B1:B5)", XY, note="covariance of a series with itself is its population variance"), C("=COVARIANCE.P(A1:A5,B1:B4)", XY, dim="errors")]),
        ("COVARIANCE.S", [C("=COVARIANCE.S(A1:A5,B1:B5)", XY), C("=COVARIANCE.S(B1:B5,B1:B5)", XY), C("=COVARIANCE.S(A1:A5,B1:B4)", XY, dim="errors")]),
        ("RSQ", [C("=RSQ(A1:A5,B1:B5)", XY), C("=RSQ(B1:B5,B1:B5)", XY), C("=RSQ(A1:A5,B1:B5)", Y + FLAT, dim="errors", note="constant x has no variance")]),
        ("SLOPE", [C("=SLOPE(A1:A5,B1:B5)", XY), C("=SLOPE(B1:B5,A1:A5)", XY), C("=SLOPE(A1:A5,B1:B5)", Y + FLAT, dim="errors")]),
        ("INTERCEPT", [C("=INTERCEPT(A1:A5,B1:B5)", XY), C("=INTERCEPT(B1:B5,A1:A5)", XY), C("=INTERCEPT(A1:A5,B1:B5)", Y + FLAT, dim="errors")]),
        ("STEYX", [C("=STEYX(A1:A5,B1:B5)", XY), C("=STEYX(B1:B5,A1:A5)", XY), C("=STEYX(A1:A2,B1:B2)", XY, dim="errors", note="needs more than two points")]),
    ],
}

"""Batch 02 — rounding variants, combinatorics, number bases, sum-of-products (25)."""
from dsl import C

A = [("A1", 2), ("A2", 3), ("A3", 4)]
B = [("B1", 5), ("B2", 6), ("B3", 7)]
AB = A + B

BATCH = {
    "id": "02",
    "title": "Rounding variants, combinatorics, number bases and sums of products",
    "functions": [
        ("ARABIC", [C('=ARABIC("XIV")'), C('=ARABIC("mcmxcix")', note="case-insensitive"), C('=ARABIC("ABC")', dim="errors")]),
        ("BASE", [C("=BASE(255,16)"), C("=BASE(7,2,8)", note="minimum length pads with zeros"), C("=BASE(-1,2)", dim="errors")]),
        ("CEILING.MATH", [C("=CEILING.MATH(4.3)"), C("=CEILING.MATH(-4.3)", note="negative numbers round toward zero by default"), C("=CEILING.MATH(-4.3,1,1)", note="mode non-zero rounds away from zero"), C("=CEILING.MATH(6.2,2)"), C('=CEILING.MATH("x")', dim="errors")]),
        ("CEILING.PRECISE", [C("=CEILING.PRECISE(4.3)"), C("=CEILING.PRECISE(-4.3)"), C("=CEILING.PRECISE(4.3,-2)", note="the sign of the significance is ignored"), C('=CEILING.PRECISE("x")', dim="errors")]),
        ("COMBIN", [C("=COMBIN(8,2)"), C("=COMBIN(5,0)"), C("=COMBIN(3,5)", dim="errors")]),
        ("COMBINA", [C("=COMBINA(4,3)"), C("=COMBINA(10,3)"), C("=COMBINA(-1,2)", dim="errors")]),
        ("DECIMAL", [C('=DECIMAL("FF",16)'), C('=DECIMAL("zz",36)'), C('=DECIMAL("9",8)', dim="errors")]),
        ("EVEN", [C("=EVEN(1.5)"), C("=EVEN(-1)"), C('=EVEN("x")', dim="errors")]),
        ("FACT", [C("=FACT(5)"), C("=FACT(0)"), C("=FACT(5.9)", note="the argument is truncated"), C("=FACT(-1)", dim="errors")]),
        ("FACTDOUBLE", [C("=FACTDOUBLE(7)"), C("=FACTDOUBLE(6)"), C("=FACTDOUBLE(-1)", dim="errors")]),
        ("FLOOR.MATH", [C("=FLOOR.MATH(4.7)"), C("=FLOOR.MATH(-4.7)", note="negative numbers round away from zero by default"), C("=FLOOR.MATH(-4.7,1,1)", note="mode non-zero rounds toward zero"), C('=FLOOR.MATH("x")', dim="errors")]),
        ("FLOOR.PRECISE", [C("=FLOOR.PRECISE(4.7)"), C("=FLOOR.PRECISE(-4.7)"), C("=FLOOR.PRECISE(4.7,-2)", note="the sign of the significance is ignored"), C('=FLOOR.PRECISE("x")', dim="errors")]),
        ("ISO.CEILING", [C("=ISO.CEILING(4.3)"), C("=ISO.CEILING(-4.3)"), C("=ISO.CEILING(4.3,2)")]),
        ("LOG", [C("=LOG(100)"), C("=LOG(8,2)"), C("=LOG(A1,3)", [("A1", 81)]), C("=LOG(0)", dim="errors")]),
        ("MROUND", [C("=MROUND(10,3)"), C("=MROUND(-10,-3)"), C("=MROUND(5,-2)", dim="errors", note="signs of number and multiple must match")]),
        ("MULTINOMIAL", [C("=MULTINOMIAL(2,3,4)"), C("=MULTINOMIAL(1,1)"), C("=MULTINOMIAL(-1,2)", dim="errors")]),
        ("ODD", [C("=ODD(1.5)"), C("=ODD(-1.5)"), C('=ODD("x")', dim="errors")]),
        ("PERMUT", [C("=PERMUT(5,2)"), C("=PERMUT(3,3)"), C("=PERMUT(2,3)", dim="errors")]),
        ("ROMAN", [C("=ROMAN(499)"), C("=ROMAN(499,1)", note="a more concise form"), C("=ROMAN(4000)", dim="errors")]),
        ("SERIESSUM", [C("=SERIESSUM(2,0,2,A1:A3)", A, note="1*2^0 + 2*2^2 + 3*2^4"), C("=SERIESSUM(1,1,1,A1:A3)", [("A1", 1), ("A2", 2), ("A3", 3)]), C('=SERIESSUM("x",0,1,A1:A3)', A, dim="errors")]),
        ("SUMPRODUCT", [C("=SUMPRODUCT(A1:A3,B1:B3)", AB), C("=SUMPRODUCT(A1:A3)", A), C("=SUMPRODUCT(A1:A3,B1:B3)", [("A1", 2), ("A2", "x"), ("A3", 4)] + B, note="text entries count as zero"), C("=SUMPRODUCT(A1:A3,B1:B2)", AB, dim="errors", note="ranges must have the same size")]),
        ("SUMSQ", [C("=SUMSQ(3,4)"), C("=SUMSQ(A1:A3)", A), C("=SUMSQ(A1:A3)", [("A1", 2), ("A2", "x"), ("A3", 4)], note="text in a range is ignored"), C('=SUMSQ("x")', dim="errors")]),
        ("SUMX2MY2", [C("=SUMX2MY2(A1:A3,B1:B3)", AB), C("=SUMX2MY2(A1:A3,B1:B2)", AB, dim="errors")]),
        ("SUMX2PY2", [C("=SUMX2PY2(A1:A3,B1:B3)", AB), C("=SUMX2PY2(A1:A3,B1:B2)", AB, dim="errors")]),
        ("SUMXMY2", [C("=SUMXMY2(A1:A3,B1:B3)", AB), C("=SUMXMY2(A1:A3,B1:B2)", AB, dim="errors")]),
    ],
}

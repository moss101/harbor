"""Batch 09 — engineering part 2: complex-number functions (25)."""
from dsl import C


def unary(fn, a="3+4i", b="1+i"):
    return (fn, [C(f'={fn}("{a}")'), C(f'={fn}("{b}")'), C(f'={fn}("abc")', dim="errors")])


BATCH = {
    "id": "09",
    "title": "Engineering: complex-number functions",
    "functions": [
        unary("IMABS"),
        unary("IMAGINARY"),
        unary("IMARGUMENT"),
        unary("IMCONJUGATE"),
        unary("IMCOS"),
        unary("IMCOSH"),
        unary("IMCOT"),
        unary("IMCSC"),
        unary("IMCSCH"),
        ("IMDIV", [C('=IMDIV("11+16i","3-5i")'), C('=IMDIV("1+i","2")'), C('=IMDIV("1+i","0")', dim="errors")]),
        unary("IMEXP"),
        unary("IMLN"),
        unary("IMLOG10"),
        unary("IMLOG2"),
        ("IMPOWER", [C('=IMPOWER("2+3i",3)'), C('=IMPOWER("1+i",0.5)'), C('=IMPOWER("abc",2)', dim="errors")]),
        ("IMPRODUCT", [C('=IMPRODUCT("3+4i","5-3i")'), C('=IMPRODUCT("1+i","1-i","2")', note="three factors"), C('=IMPRODUCT("abc","1")', dim="errors")]),
        unary("IMREAL"),
        unary("IMSEC"),
        unary("IMSECH"),
        unary("IMSIN"),
        unary("IMSINH"),
        unary("IMSQRT"),
        ("IMSUB", [C('=IMSUB("13+4i","5+3i")'), C('=IMSUB("1+i","1+i")'), C('=IMSUB("abc","1")', dim="errors")]),
        ("IMSUM", [C('=IMSUM("3+4i","5-3i")'), C('=IMSUM("1+i","2","3j")', note="three terms with mixed suffixes may be rejected"), C('=IMSUM("abc","1")', dim="errors")]),
        unary("IMTAN"),
    ],
}

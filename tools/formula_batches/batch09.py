"""Batch 09 — engineering part 2: complex-number functions (25)."""
from dsl import C


def unary(fn, a="3+4i", b="1+i"):
    return (fn, [C(f'={fn}("{a}")'), C(f'={fn}("{b}")'), C(f'={fn}("abc")', dim="errors")])


def numeric(fn, a="3+4i", b="1+i", noisy_text=()):
    """Transcendental results are checked by their real/imaginary parts, not by the
    15th printed digit: that digit depends on the platform libm (and Excel's own
    1-ulp noise), so a text comparison would assert noise. The printed format is
    still checked by the cases of the functions whose digits are stable."""
    cases = []
    for z in (a, b):
        if z in noisy_text:
            cases += [C(f'=IMREAL({fn}("{z}"))', note="real part"), C(f'=IMAGINARY({fn}("{z}"))', note="imaginary part")]
        else:
            cases.append(C(f'={fn}("{z}")'))
    cases.append(C(f'={fn}("abc")', dim="errors"))
    return (fn, cases)


BATCH = {
    "id": "09",
    "title": "Engineering: complex-number functions",
    "functions": [
        unary("IMABS"),
        unary("IMAGINARY"),
        unary("IMARGUMENT"),
        unary("IMCONJUGATE"),
        unary("IMCOS"),
        numeric("IMCOSH", noisy_text=("3+4i",)),
        unary("IMCOT"),
        numeric("IMCSC", noisy_text=("1+i",)),
        unary("IMCSCH"),
        ("IMDIV", [C('=IMDIV("11+16i","3-5i")'), C('=IMDIV("1+i","2")'), C('=IMDIV("1+i","0")', dim="errors")]),
        unary("IMEXP"),
        unary("IMLN"),
        unary("IMLOG10"),
        unary("IMLOG2"),
        ("IMPOWER", [C('=IMREAL(IMPOWER("2+3i",3))', note="real part; Excel prints 9.00000000000001i from rounding noise, so parts are compared numerically"), C('=IMAGINARY(IMPOWER("2+3i",3))'), C('=IMPOWER("1+i",0.5)'), C('=IMPOWER("abc",2)', dim="errors")]),
        ("IMPRODUCT", [C('=IMPRODUCT("3+4i","5-3i")'), C('=IMPRODUCT("1+i","1-i","2")', note="three factors"), C('=IMPRODUCT("abc","1")', dim="errors")]),
        unary("IMREAL"),
        unary("IMSEC"),
        numeric("IMSECH", noisy_text=("3+4i",)),
        unary("IMSIN"),
        unary("IMSINH"),
        unary("IMSQRT"),
        ("IMSUB", [C('=IMSUB("13+4i","5+3i")'), C('=IMSUB("1+i","1+i")'), C('=IMSUB("abc","1")', dim="errors")]),
        ("IMSUM", [C('=IMSUM("3+4i","5-3i")'), C('=IMSUM("1+i","2","3j")', note="three terms with mixed suffixes may be rejected"), C('=IMSUM("abc","1")', dim="errors")]),
        unary("IMTAN"),
    ],
}

"""Batch 01 — trigonometry, hyperbolics and angle helpers (25 functions)."""
from dsl import C

BATCH = {
    "id": "01",
    "title": "Trigonometry, hyperbolics and angle helpers",
    "functions": [
        ("ACOS", [C("=ACOS(0.5)"), C("=ACOS(A1)", [("A1", -1)]), C('=ACOS("1")', note="text coerced to a number"), C("=ACOS(2)", dim="errors")]),
        ("ACOSH", [C("=ACOSH(1)"), C("=ACOSH(10)"), C("=ACOSH(0.5)", dim="errors")]),
        ("ACOT", [C("=ACOT(1)"), C("=ACOT(0)"), C("=ACOT(-1)", note="range is (0, pi), so a negative argument gives more than pi/2")]),
        ("ACOTH", [C("=ACOTH(2)"), C("=ACOTH(-3)"), C("=ACOTH(0.5)", dim="errors")]),
        ("ASIN", [C("=ASIN(0.5)"), C("=ASIN(-1)"), C("=ASIN(1.5)", dim="errors")]),
        ("ASINH", [C("=ASINH(1)"), C("=ASINH(-2.5)"), C("=ASINH(A1)", dim="blanks", note="a blank cell is 0")]),
        ("ATAN", [C("=ATAN(1)"), C("=ATAN(A1)", [("A1", -1000000)]), C('=ATAN("x")', dim="errors")]),
        ("ATAN2", [C("=ATAN2(1,1)"), C("=ATAN2(-1,-1)", note="x first, y second: third quadrant"), C("=ATAN2(0,0)", dim="errors")]),
        ("ATANH", [C("=ATANH(0.5)"), C("=ATANH(-0.9)"), C("=ATANH(1)", dim="errors")]),
        ("COS", [C("=COS(0)"), C("=COS(PI()/3)"), C('=COS("a")', dim="errors")]),
        ("COSH", [C("=COSH(1)"), C("=COSH(-2)"), C("=COSH(A1)", dim="blanks")]),
        ("COT", [C("=COT(1)"), C("=COT(PI()/4)"), C("=COT(0)", dim="errors")]),
        ("COTH", [C("=COTH(1)"), C("=COTH(-2)"), C("=COTH(0)", dim="errors")]),
        ("CSC", [C("=CSC(1)"), C("=CSC(PI()/2)"), C("=CSC(0)", dim="errors")]),
        ("CSCH", [C("=CSCH(1)"), C("=CSCH(-2)"), C("=CSCH(0)", dim="errors")]),
        ("SEC", [C("=SEC(1)"), C("=SEC(PI())"), C('=SEC("x")', dim="errors")]),
        ("SECH", [C("=SECH(1)"), C("=SECH(0)"), C('=SECH("x")', dim="errors")]),
        ("SIN", [C("=SIN(PI()/6)"), C("=SIN(1)"), C("=SIN(TRUE)", note="a boolean argument is coerced to 1")]),
        ("SINH", [C("=SINH(1)"), C("=SINH(-0.5)"), C('=SINH("x")', dim="errors")]),
        ("TAN", [C("=TAN(PI()/4)"), C("=TAN(1)"), C("=TAN(A1)", dim="blanks")]),
        ("TANH", [C("=TANH(1)"), C("=TANH(-10)"), C('=TANH("a")', dim="errors")]),
        ("DEGREES", [C("=DEGREES(PI())"), C("=DEGREES(1)"), C('=DEGREES("x")', dim="errors")]),
        ("RADIANS", [C("=RADIANS(180)"), C("=RADIANS(90)"), C('=RADIANS("x")', dim="errors")]),
        ("PI", [C("=PI()"), C("=PI()*2"), C("=PI()*A1^2", [("A1", 3)], note="area of a circle of radius 3")]),
        ("SQRTPI", [C("=SQRTPI(1)"), C("=SQRTPI(4)"), C("=SQRTPI(-1)", dim="errors")]),
    ],
}
